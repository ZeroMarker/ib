use crate::db::{Connection, Pool};
use crate::http::{run_db, ApiError, ApiResult};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Json, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rand_core::OsRng;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

const SESSION_COOKIE: &str = "ib_session";
const SESSION_MAX_AGE: i64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Pool>,
}

pub(crate) struct CurrentUser {
    pub user_id: String,
    pub email: String,
    pub email_verified: bool,
}

#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct ResendRequest {
    email: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    user_id: String,
    email: String,
    email_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    message: &'static str,
}

/// Resolve on SIGINT (Ctrl-C) or SIGTERM (systemd), whichever arrives first.
pub(crate) async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("ib: cannot install Ctrl-C handler: {error}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                // Without the handler the default action already terminates
                // the process, so just wait forever and let the signal land.
                eprintln!("ib: cannot install SIGTERM handler: {error}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {}
        _ = terminate => {}
    }
    println!("received shutdown signal, draining connections");
}

/// Precomputed Argon2 hash used to equalize login timing for unknown emails,
/// so response latency does not reveal whether an account exists.
fn dummy_password_hash() -> &'static String {
    static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        hash_password("ib-timing-equalizer").expect("dummy hash generation failed")
    });
    &DUMMY_HASH
}

pub(crate) async fn health(State(state): State<AppState>) -> Response {
    run_db(state, |pool| {
        let alive = match pool.get() {
            Ok(conn) => conn
                .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .is_ok_and(|value| value == 1),
            Err(_) => false,
        };
        if alive {
            Ok(Json(MessageResponse { message: "ok" }).into_response())
        } else {
            Err(ApiError::Unavailable("database unavailable"))
        }
    })
    .await
}

pub(crate) async fn register(
    State(state): State<AppState>,
    Json(request): Json<AuthRequest>,
) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return ApiError::Invalid(message.to_owned()).into_response(),
    };
    if let Err(message) = validate_password(&request.password) {
        return ApiError::Invalid(message.to_owned()).into_response();
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
            Ok(Err(api_error)) => return api_error.into_response(),
            Err(_) => {
                return ApiError::Internal("internal server error").into_response();
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

fn register_in_db(pool: &Pool, email: &str, password: &str) -> ApiResult<RegisterOutcome> {
    // Argon2 hashing costs tens of milliseconds; this runs in spawn_blocking.
    let password_hash =
        hash_password(password).map_err(|_| ApiError::Internal("could not create account"))?;
    let user_id = Uuid::new_v4().to_string();
    let conn = pool
        .get()
        .map_err(|_| ApiError::Unavailable("database unavailable"))?;

    if conn.execute_batch("BEGIN IMMEDIATE").is_err() {
        return Err(ApiError::Unavailable("database unavailable"));
    }
    // A single INSERT lets the unique EMAIL index settle concurrent
    // registrations; no check-then-insert race window remains.
    if let Err(insert_error) = conn.execute(
        "INSERT INTO USERS (USER_ID, EMAIL, PASSWORD_HASH) VALUES (?1, ?2, ?3)",
        params![user_id.as_str(), email, password_hash.as_str()],
    ) {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(if is_unique_violation(&insert_error) {
            ApiError::Conflict("email is already registered")
        } else {
            ApiError::Unavailable("database unavailable")
        });
    }

    ensure_verification_table(&conn);
    let verify_token = match store_verification_token(&conn, &user_id) {
        Ok(token) => token,
        Err(_) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(ApiError::Internal("could not create verification email"));
        }
    };
    if conn.execute_batch("COMMIT").is_err() {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(ApiError::Unavailable("database unavailable"));
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

pub(crate) async fn verify(
    State(state): State<AppState>,
    Json(request): Json<VerifyRequest>,
) -> Response {
    let token = request.token.trim().to_owned();
    if token.is_empty() {
        return ApiError::Invalid("verification token is required".into()).into_response();
    }
    run_db(state, move |pool| {
        let conn = pool
            .get()
            .map_err(|_| ApiError::Unavailable("database unavailable"))?;
        ensure_verification_table(&conn);
        let token_hash = hash_token(&token);
        let (user_id, expired): (String, bool) = conn
            .query_row(
                "SELECT USER_ID, EXPIRES_AT <= datetime('now') FROM EMAIL_VERIFICATIONS \
                 WHERE TOKEN_HASH = ?1",
                params![token_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| ApiError::Invalid("invalid verification token".into()))?;
        if expired {
            let _ = conn.execute(
                "DELETE FROM EMAIL_VERIFICATIONS WHERE TOKEN_HASH = ?1",
                params![token_hash],
            );
            return Err(ApiError::Gone("verification token has expired"));
        }
        conn.execute(
            "UPDATE USERS SET EMAIL_VERIFIED = 1 WHERE USER_ID = ?1",
            params![user_id.as_str()],
        )
        .map_err(|_| ApiError::Unavailable("database unavailable"))?;
        // Single use: burn every outstanding token, not just this one.
        let _ = conn.execute(
            "DELETE FROM EMAIL_VERIFICATIONS WHERE USER_ID = ?1",
            params![user_id.as_str()],
        );
        let (user_id, email) = conn
            .query_row(
                "SELECT USER_ID, EMAIL FROM USERS WHERE USER_ID = ?1",
                params![user_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(|_| ApiError::Unavailable("database unavailable"))?;
        Ok(Json(UserResponse {
            user_id,
            email,
            email_verified: true,
        })
        .into_response())
    })
    .await
}

pub(crate) async fn resend_verification(
    State(state): State<AppState>,
    Json(request): Json<ResendRequest>,
) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return ApiError::Invalid(message.to_owned()).into_response(),
    };
    let minted = {
        let state = state.clone();
        match tokio::task::spawn_blocking(move || mint_verification_token(&state.db, &email)).await
        {
            Ok(minted) => minted,
            Err(_) => return ApiError::Internal("internal server error").into_response(),
        }
    };
    // Never reveal whether an address is registered. Unknown emails, already
    // verified accounts, an unconfigured mailer and a provider failure must
    // all look identical to the caller, otherwise the endpoint becomes an
    // account-enumeration oracle (a 503 for "known but unconfigured" versus
    // 200 for "unknown" is exactly such an oracle).
    let (email, token) = match minted {
        Some(pair) => pair,
        None => return Json(MessageResponse { message: "ok" }).into_response(),
    };
    if !crate::email::is_configured() {
        eprintln!("resend skipped for {email}: verification email is not configured");
        return Json(MessageResponse { message: "ok" }).into_response();
    }
    if let Err(send_error) = crate::email::send_verification(&email, &token).await {
        eprintln!("resend verification email failed for {email}: {send_error}");
    }
    Json(MessageResponse { message: "ok" }).into_response()
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

pub(crate) async fn login(
    State(state): State<AppState>,
    Json(request): Json<AuthRequest>,
) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return ApiError::Invalid(message.to_owned()).into_response(),
    };

    run_db(state, move |pool| {
        let conn = pool
            .get()
            .map_err(|_| ApiError::Unavailable("database unavailable"))?;
        // Login is what grows SESSIONS, so it is also where expired rows get
        // reclaimed; see migration 004 for the supporting indexes.
        purge_expired(&conn);
        let row: (String, String, i64) = conn
            .query_row(
                "SELECT USER_ID, PASSWORD_HASH, EMAIL_VERIFIED FROM USERS \
                 WHERE EMAIL = ?1 AND STATUS = 'ACTIVE'",
                params![email.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_or_else(
                |_| {
                    // Burn an Argon2 verification so unknown-email responses
                    // take the same time as wrong-password ones.
                    verify_password(&request.password, dummy_password_hash());
                    Err(ApiError::Unauthenticated("invalid email or password"))
                },
                |row| {
                    if !verify_password(&request.password, &row.1) {
                        return Err(ApiError::Unauthenticated("invalid email or password"));
                    }
                    Ok(row)
                },
            )?;
        let (user_id, _password_hash, verified) = row;
        // Verification is enforced only when the server can actually send
        // verification emails; otherwise local/dev logins stay usable and
        // the account remains unverified.
        if verified != 1 && crate::email::is_configured() {
            return Err(ApiError::Forbidden(
                "email not verified; check your inbox for the verification email",
            ));
        }

        let token = insert_session(&conn, &user_id)
            .map_err(|_| ApiError::Internal("could not create session"))?;

        let response = UserResponse {
            user_id,
            email,
            email_verified: verified == 1,
        };
        Ok(with_session_cookie(StatusCode::OK, token, Json(response)))
    })
    .await
}

/// Delete rows that can no longer authenticate anyone. Failures are ignored
/// on purpose: a stale row is harmless, and a cleanup error must never stop
/// someone from logging in.
pub(crate) fn purge_expired(conn: &Connection) {
    ensure_verification_table(conn);
    for sql in [
        "DELETE FROM SESSIONS WHERE EXPIRES_AT <= datetime('now')",
        "DELETE FROM EMAIL_VERIFICATIONS WHERE EXPIRES_AT <= datetime('now')",
    ] {
        if let Err(error) = conn.execute(sql, []) {
            eprintln!("ib: expired-row sweep skipped ({error}): {sql}");
        }
    }
}

pub(crate) async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
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
        Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response())
    })
    .await
}

pub(crate) async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let conn = pool
            .get()
            .map_err(|_| ApiError::Unavailable("database unavailable"))?;
        let user = current_user(&conn, &headers)
            .map_err(|_| ApiError::Unauthenticated("authentication required"))?;
        Ok(Json(UserResponse {
            user_id: user.user_id,
            email: user.email,
            email_verified: user.email_verified,
        })
        .into_response())
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
