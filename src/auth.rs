use crate::db::{Connection, Pool};
use crate::web;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Json, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rand_core::OsRng;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, sync::Arc};
use uuid::Uuid;

const SESSION_COOKIE: &str = "ib_session";
const SESSION_MAX_AGE: i64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Arc<Pool>,
}

pub(crate) struct CurrentUser {
    pub user_id: String,
    pub email: String,
    pub email_verified: bool,
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct VerifyRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ResendRequest {
    email: String,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    user_id: String,
    email: String,
    email_verified: bool,
}

#[derive(Debug, Serialize)]
struct MessageResponse {
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

pub async fn serve(pool: Pool, address: &str) {
    let address: SocketAddr = address
        .parse()
        .unwrap_or_else(|_| panic!("invalid server address: {address}"));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {address}: {e}"));
    let state = AppState { db: Arc::new(pool) };
    let app = Router::new()
        .route("/", get(web::index))
        .route("/login", get(web::index))
        .route("/register", get(web::index))
        .route("/assets/index.css", get(web::styles))
        .route("/assets/app.js", get(web::app_js))
        .route("/manifest.webmanifest", get(web::manifest))
        .route("/sw.js", get(web::service_worker))
        .route("/icons/icon-192.png", get(web::icon_192))
        .route("/icons/icon-512.png", get(web::icon_512))
        .route("/icons/icon.svg", get(web::icon_svg))
        .route("/api/health", get(health))
        .route("/api/auth/register", post(register))
        .route("/api/auth/verify", post(verify))
        .route("/api/auth/resend-verification", post(resend_verification))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .merge(crate::trading::router())
        .with_state(state);

    println!("simulation auth API listening on http://{address}");
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("HTTP server failed: {e}"));
}

async fn run_db<F>(state: AppState, task: F) -> Response
where
    F: FnOnce(&Pool) -> Response + Send + 'static,
{
    match tokio::task::spawn_blocking(move || task(&state.db)).await {
        Ok(response) => response,
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
    }
}

/// Precomputed Argon2 hash used to equalize login timing for unknown emails,
/// so response latency does not reveal whether an account exists.
fn dummy_password_hash() -> &'static String {
    static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        hash_password("ib-timing-equalizer").expect("dummy hash generation failed")
    });
    &DUMMY_HASH
}

async fn health(State(state): State<AppState>) -> Response {
    run_db(state, |pool| {
        let alive = pool
            .get()
            .and_then(|conn| {
                conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                    .map_err(|_| "database unavailable".to_string())
            })
            .is_ok_and(|value| value == 1);
        if alive {
            Json(MessageResponse { message: "ok" }).into_response()
        } else {
            error(StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
        }
    })
    .await
}

async fn register(State(state): State<AppState>, Json(request): Json<AuthRequest>) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Err(message) = validate_password(&request.password) {
        return error(StatusCode::BAD_REQUEST, message);
    }

    // Blocking DB work stays off Tokio workers; the Resend HTTP send
    // happens afterwards in async context.
    let outcome = {
        let state = state.clone();
        let password = request.password.clone();
        match tokio::task::spawn_blocking(move || register_in_db(&state.db, &email, &password))
            .await
        {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(response)) => return response,
            Err(_) => {
                return error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error");
            }
        }
    };

    // Registration never signs the user in: the account stays unverified
    // until the emailed token is confirmed. Send failures only leave the
    // account unverified; use resend-verification to retry.
    if crate::email::is_configured() {
        if let Err(send_error) =
            crate::email::send_verification(&outcome.email, &outcome.verify_token).await
        {
            eprintln!(
                "resend verification email failed for {}: {send_error}",
                outcome.email
            );
        }
    }

    (StatusCode::CREATED, Json(outcome.user)).into_response()
}

struct RegisterOutcome {
    user: UserResponse,
    email: String,
    verify_token: String,
}


fn register_in_db(pool: &Pool, email: &str, password: &str) -> Result<RegisterOutcome, Response> {
    // Argon2 hashing costs tens of milliseconds; this runs in spawn_blocking.
    let password_hash = match hash_password(password) {
        Ok(hash) => hash,
        Err(_) => {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create account",
            ));
        }
    };
    let user_id = Uuid::new_v4().to_string();
    let conn = match pool.get() {
        Ok(conn) => conn,
        Err(_) => {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database unavailable",
            ));
        }
    };

    if conn.execute_batch("BEGIN IMMEDIATE").is_err() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database unavailable",
        ));
    }
    // A single INSERT lets the unique EMAIL index settle concurrent
    // registrations; no check-then-insert race window remains.
    if let Err(insert_error) = conn.execute(
        "INSERT INTO USERS (USER_ID, EMAIL, PASSWORD_HASH) VALUES (?1, ?2, ?3)",
        params![user_id.as_str(), email, password_hash.as_str()],
    ) {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(if is_unique_violation(&insert_error) {
            error(StatusCode::CONFLICT, "email is already registered")
        } else {
            error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable")
        });
    }

    ensure_verification_table(&conn);
    let verify_token = match store_verification_token(&conn, &user_id) {
        Ok(token) => token,
        Err(_) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create verification email",
            ));
        }
    };
    if conn.execute_batch("COMMIT").is_err() {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database unavailable",
        ));
    }

    Ok(RegisterOutcome {
        user: UserResponse {
            user_id: user_id.clone(),
            email: email.to_owned(),
            email_verified: false,
        },
        email: email.to_owned(),
        verify_token,
    })
}

async fn verify(State(state): State<AppState>, Json(request): Json<VerifyRequest>) -> Response {
    let token = request.token.trim().to_owned();
    if token.is_empty() {
        return error(StatusCode::BAD_REQUEST, "verification token is required");
    }
    run_db(state, move |pool| {
        let conn = match pool.get() {
            Ok(conn) => conn,
            Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
        };
        ensure_verification_table(&conn);
        let token_hash = hash_token(&token);
        let (user_id, expired): (String, bool) = match conn.query_row(
            "SELECT USER_ID, EXPIRES_AT <= datetime('now') FROM EMAIL_VERIFICATIONS \
             WHERE TOKEN_HASH = ?1",
            params![token_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(found) => found,
            Err(_) => return error(StatusCode::BAD_REQUEST, "invalid verification token"),
        };
        if expired {
            let _ = conn.execute(
                "DELETE FROM EMAIL_VERIFICATIONS WHERE TOKEN_HASH = ?1",
                params![token_hash],
            );
            return error(StatusCode::GONE, "verification token has expired");
        }
        if conn
            .execute(
                "UPDATE USERS SET EMAIL_VERIFIED = 1 WHERE USER_ID = ?1",
                params![user_id.as_str()],
            )
            .is_err()
        {
            return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable");
        }
        let _ = conn.execute(
            "DELETE FROM EMAIL_VERIFICATIONS WHERE USER_ID = ?1",
            params![user_id.as_str()],
        );
        match conn.query_row(
            "SELECT USER_ID, EMAIL FROM USERS WHERE USER_ID = ?1",
            params![user_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ) {
            Ok((user_id, email)) => Json(UserResponse {
                user_id,
                email,
                email_verified: true,
            })
            .into_response(),
            Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
        }
    })
    .await
}

async fn resend_verification(
    State(state): State<AppState>,
    Json(request): Json<ResendRequest>,
) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    let minted = {
        let state = state.clone();
        match tokio::task::spawn_blocking(move || mint_verification_token(&state.db, &email)).await
        {
            Ok(minted) => minted,
            Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
        }
    };
    // Never reveal whether an address is registered: unknown emails and
    // already-verified accounts get the same generic success.
    let (email, token) = match minted {
        Some(pair) => pair,
        None => return Json(MessageResponse { message: "ok" }).into_response(),
    };
    if !crate::email::is_configured() {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "verification email is not configured",
        );
    }
    match crate::email::send_verification(&email, &token).await {
        Ok(()) => Json(MessageResponse { message: "ok" }).into_response(),
        Err(send_error) => {
            eprintln!("resend verification email failed for {email}: {send_error}");
            error(
                StatusCode::BAD_GATEWAY,
                "could not send verification email",
            )
        }
    }
}

/// Mint a fresh verification token for an unverified user.
/// Returns `None` for unknown emails and already-verified accounts so the
/// caller can answer with a generic success without leaking account state.
fn mint_verification_token(pool: &Pool, email: &str) -> Option<(String, String)> {
    let conn = pool.get().ok()?;
    ensure_verification_table(&conn);
    let (user_id, verified): (String, i64) = conn
        .query_row(
            "SELECT USER_ID, EMAIL_VERIFIED FROM USERS WHERE EMAIL = ?1 AND STATUS = 'ACTIVE'",
            params![email],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()?;
    if verified == 1 {
        return None;
    }
    // One outstanding token per user: replace any previous email's token so
    // only the newest link verifies.
    let _ = conn.execute(
        "DELETE FROM EMAIL_VERIFICATIONS WHERE USER_ID = ?1",
        params![user_id.as_str()],
    );
    let token = store_verification_token(&conn, &user_id).ok()?;
    Some((email.to_owned(), token))
}

/// Create the verification table on demand so databases initialized before
/// migration 003 keep working without a manual re-migration.
fn ensure_verification_table(conn: &Connection) {
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS EMAIL_VERIFICATIONS (
            VERIFICATION_ID TEXT NOT NULL,
            USER_ID         TEXT NOT NULL,
            TOKEN_HASH      TEXT NOT NULL,
            EXPIRES_AT      TEXT NOT NULL,
            CREATED_AT      TEXT DEFAULT (datetime('now')) NOT NULL,
            CONSTRAINT PK_EMAIL_VERIFICATIONS PRIMARY KEY (VERIFICATION_ID),
            CONSTRAINT UQ_EMAIL_VERIFICATIONS_TOKEN UNIQUE (TOKEN_HASH),
            CONSTRAINT FK_EMAIL_VERIFICATIONS_USER FOREIGN KEY (USER_ID) REFERENCES USERS (USER_ID)
        );
        CREATE INDEX IF NOT EXISTS IX_EMAIL_VERIFICATIONS_USER ON EMAIL_VERIFICATIONS (USER_ID);",
    );
}

fn store_verification_token(conn: &Connection, user_id: &str) -> Result<String, rusqlite::Error> {
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_token(&token);
    let verification_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO EMAIL_VERIFICATIONS (VERIFICATION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
         VALUES (?1, ?2, ?3, datetime('now', '+24 hours'))",
        params![verification_id, user_id, token_hash],
    )?;
    Ok(token)
}

async fn login(State(state): State<AppState>, Json(request): Json<AuthRequest>) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };

    run_db(state, move |pool| {
        let conn = match pool.get() {
            Ok(conn) => conn,
            Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
        };
        let (user_id, password_hash, verified): (String, String, i64) = match conn.query_row(
            "SELECT USER_ID, PASSWORD_HASH, EMAIL_VERIFIED FROM USERS \
             WHERE EMAIL = ?1 AND STATUS = 'ACTIVE'",
            params![email.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ) {
            Ok(row) => row,
            // Burn an Argon2 verification so unknown-email responses take the
            // same time as wrong-password ones.
            Err(_) => {
                verify_password(&request.password, dummy_password_hash());
                return error(StatusCode::UNAUTHORIZED, "invalid email or password");
            }
        };
        if !verify_password(&request.password, &password_hash) {
            return error(StatusCode::UNAUTHORIZED, "invalid email or password");
        }
        // Verification is enforced only when the server can actually send
        // verification emails; otherwise local/dev logins stay usable and
        // the account remains unverified.
        if verified != 1 && crate::email::is_configured() {
            return error(
                StatusCode::FORBIDDEN,
                "email not verified; check your inbox for the verification email",
            );
        }

        let token = match insert_session(&conn, &user_id) {
            Ok(token) => token,
            Err(_) => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not create session",
                )
            }
        };

        let response = UserResponse {
            user_id,
            email,
            email_verified: verified == 1,
        };
        with_session_cookie(StatusCode::OK, token, Json(response))
    })
    .await
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        if let Some(token) = session_token(&headers) {
            if let Ok(conn) = pool.get() {
                let token_hash = hash_token(&token);
                let _ = conn.execute(
                    "DELETE FROM SESSIONS WHERE TOKEN_HASH = ?1",
                    params![token_hash],
                );
            }
        }
        let cookie =
            format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Secure; Max-Age=0");
        (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
    })
    .await
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let conn = match pool.get() {
            Ok(conn) => conn,
            Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
        };
        match current_user(&conn, &headers) {
            Ok(user) => Json(UserResponse {
                user_id: user.user_id,
                email: user.email,
                email_verified: user.email_verified,
            })
            .into_response(),
            Err(status) => error(status, "authentication required"),
        }
    })
    .await
}

pub(crate) fn current_user(
    conn: &Connection,
    headers: &HeaderMap,
) -> Result<CurrentUser, StatusCode> {
    let token = session_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let token_hash = hash_token(&token);
    let (user_id, email, verified): (String, String, i64) = conn
        .query_row(
            "SELECT U.USER_ID, U.EMAIL, U.EMAIL_VERIFIED \
             FROM SESSIONS S JOIN USERS U ON U.USER_ID = S.USER_ID \
             WHERE S.TOKEN_HASH = ?1 AND S.EXPIRES_AT > datetime('now') \
               AND U.STATUS = 'ACTIVE'",
            params![token_hash],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(CurrentUser {
        user_id,
        email,
        email_verified: verified == 1,
    })
}

fn insert_session(conn: &Connection, user_id: &str) -> Result<String, rusqlite::Error> {
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_token(&token);
    let session_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO SESSIONS (SESSION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
         VALUES (?1, ?2, ?3, datetime('now', '+30 days'))",
        params![session_id, user_id, token_hash],
    )?;
    Ok(token)
}

/// SQLite reports unique-index conflicts as extended code 2067
/// (`SQLITE_CONSTRAINT_UNIQUE`).
fn is_unique_violation(error: &rusqlite::Error) -> bool {
    match error {
        rusqlite::Error::SqliteFailure(failure, _) => failure.extended_code == 2067,
        _ => false,
    }
}

fn normalize_email(email: &str) -> Result<String, &'static str> {
    let email = email.trim().to_lowercase();
    if email.len() > 320
        || email.split_once('@').is_none_or(|(local, domain)| {
            local.is_empty() || domain.is_empty() || !domain.contains('.')
        })
    {
        return Err("invalid email");
    }
    Ok(email)
}

fn validate_password(password: &str) -> Result<(), &'static str> {
    if !(8..=128).contains(&password.len()) {
        Err("password must be 8 to 128 bytes")
    } else {
        Ok(())
    }
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    let parsed = match PasswordHash::new(password_hash) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
}

fn with_session_cookie<T: Serialize>(status: StatusCode, token: String, body: Json<T>) -> Response {
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Secure; Max-Age={SESSION_MAX_AGE}"
    );
    (status, [(header::SET_COOKIE, cookie)], body).into_response()
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_normalized_and_validated() {
        assert_eq!(
            normalize_email(" User@Example.COM ").unwrap(),
            "user@example.com"
        );
        assert!(normalize_email("not-an-email").is_err());
    }

    #[test]
    fn passwords_are_hashed_and_verified() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }
}
