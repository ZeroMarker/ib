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
use oracle::Connection;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const SESSION_COOKIE: &str = "ib_session";
const SESSION_MAX_AGE: i64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Arc<Mutex<Connection>>,
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

pub async fn serve(conn: Connection, address: &str) {
    let address: SocketAddr = address
        .parse()
        .unwrap_or_else(|_| panic!("invalid server address: {address}"));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {address}: {e}"));
    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };
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

async fn health() -> Response {
    Json(MessageResponse { message: "ok" }).into_response()
}

async fn register(State(state): State<AppState>, Json(request): Json<AuthRequest>) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Err(message) = validate_password(&request.password) {
        return error(StatusCode::BAD_REQUEST, message);
    }

    let password_hash = match hash_password(&request.password) {
        Ok(hash) => hash,
        Err(_) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create account",
            )
        }
    };
    let user_id = Uuid::new_v4().to_string();
    let conn = match state.db.lock() {
        Ok(conn) => conn,
        Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
    };

    let existing: i64 = match conn.query_row_as(
        "SELECT COUNT(*) FROM USERS WHERE EMAIL = :1",
        &[&email.as_str()],
    ) {
        Ok(count) => count,
        Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
    };
    if existing > 0 {
        return error(StatusCode::CONFLICT, "email is already registered");
    }

    if conn
        .execute(
            "INSERT INTO USERS (USER_ID, EMAIL, PASSWORD_HASH) VALUES (:1, :2, :3)",
            &[&user_id.as_str(), &email.as_str(), &password_hash.as_str()],
        )
        .is_err()
    {
        let _ = conn.rollback();
        return error(StatusCode::CONFLICT, "email is already registered");
    }

    // TODO(resend): create a verification token and send it with Resend using RESEND_API_KEY.
    // Registration remains usable until this adapter is implemented.
    let token = match insert_session(&conn, &user_id) {
        Ok(token) => token,
        Err(_) => {
            let _ = conn.rollback();
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create session",
            );
        }
    };
    if conn.commit().is_err() {
        let _ = conn.rollback();
        return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable");
    }

    let response = UserResponse {
        user_id,
        email,
        email_verified: false,
    };
    with_session_cookie(StatusCode::CREATED, token, Json(response))
}

async fn login(State(state): State<AppState>, Json(request): Json<AuthRequest>) -> Response {
    let email = match normalize_email(&request.email) {
        Ok(email) => email,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    let conn = match state.db.lock() {
        Ok(conn) => conn,
        Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"),
    };
    let (user_id, password_hash, verified): (String, String, i64) = match conn.query_row_as(
        "SELECT USER_ID, PASSWORD_HASH, EMAIL_VERIFIED FROM USERS \
         WHERE EMAIL = :1 AND STATUS = 'ACTIVE'",
        &[&email.as_str()],
    ) {
        Ok(row) => row,
        Err(_) => return error(StatusCode::UNAUTHORIZED, "invalid email or password"),
    };
    if !verify_password(&request.password, &password_hash) {
        return error(StatusCode::UNAUTHORIZED, "invalid email or password");
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
    if conn.commit().is_err() {
        let _ = conn.rollback();
        return error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable");
    }

    let response = UserResponse {
        user_id,
        email,
        email_verified: verified == 1,
    };
    with_session_cookie(StatusCode::OK, token, Json(response))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        if let Ok(conn) = state.db.lock() {
            let token_hash = hash_token(&token);
            let _ = conn.execute(
                "DELETE FROM SESSIONS WHERE TOKEN_HASH = :1",
                &[&token_hash.as_str()],
            );
            let _ = conn.commit();
        }
    }
    let cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Secure; Max-Age=0");
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let conn = match state.db.lock() {
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
}

pub(crate) fn current_user(
    conn: &Connection,
    headers: &HeaderMap,
) -> Result<CurrentUser, StatusCode> {
    let token = session_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let token_hash = hash_token(&token);
    let (user_id, email, verified): (String, String, i64) = conn
        .query_row_as(
            "SELECT U.USER_ID, U.EMAIL, U.EMAIL_VERIFIED \
             FROM SESSIONS S JOIN USERS U ON U.USER_ID = S.USER_ID \
             WHERE S.TOKEN_HASH = :1 AND S.EXPIRES_AT > SYSTIMESTAMP \
               AND U.STATUS = 'ACTIVE'",
            &[&token_hash.as_str()],
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(CurrentUser {
        user_id,
        email,
        email_verified: verified == 1,
    })
}

fn insert_session(conn: &Connection, user_id: &str) -> Result<String, oracle::Error> {
    let token = Uuid::new_v4().to_string();
    let token_hash = hash_token(&token);
    conn.execute(
        "INSERT INTO SESSIONS (SESSION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
         VALUES (:1, :2, :3, SYSTIMESTAMP + INTERVAL '30' DAY)",
        &[
            &Uuid::new_v4().to_string().as_str(),
            &user_id,
            &token_hash.as_str(),
        ],
    )?;
    Ok(token)
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
