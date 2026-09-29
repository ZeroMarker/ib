//! Shared HTTP plumbing: one error type, one JSON error body, one blocking
//! bridge.
//!
//! Handlers previously threaded `Box<Response>` through validation helpers so
//! that a data-layer failure could carry a status code. That forced every
//! helper signature to mention `Response` (and tripped clippy's
//! `result_large_err`). `ApiError` is a small `Copy`-ish enum instead, so
//! handlers can use `?` and let axum render the response.

use crate::auth::AppState;
use crate::db::Pool;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// A failure with an HTTP status attached. The message is the
/// client-facing text, so nothing sensitive belongs in a variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApiError {
    /// 400 with a specific, user-actionable message.
    Invalid(String),
    /// 401 missing or expired session.
    Unauthenticated(&'static str),
    /// 403 authenticated but not allowed.
    Forbidden(&'static str),
    /// 410 the referenced token is no longer usable.
    Gone(&'static str),
    /// 409 state conflict (duplicate email, non-cancellable order, ...).
    Conflict(&'static str),
    /// 503 a dependency is down or not configured.
    Unavailable(&'static str),
    /// 500 unexpected; the message must be generic.
    Internal(&'static str),
}

impl ApiError {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::Unauthenticated(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Gone(_) => StatusCode::GONE,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Invalid(message) => message,
            Self::Unauthenticated(message)
            | Self::Forbidden(message)
            | Self::Gone(message)
            | Self::Conflict(message)
            | Self::Unavailable(message)
            | Self::Internal(message) => message,
        }
    }

    /// Every 5xx we produce means "we broke", so log it before answering.
    pub(crate) fn log_if_server_error(&self) {
        if self.status().is_server_error() {
            eprintln!(
                "ib: request failed with {}: {}",
                self.status(),
                self.message()
            );
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.log_if_server_error();
        (
            self.status(),
            Json(ErrorResponse {
                error: self.message().to_owned(),
            }),
        )
            .into_response()
    }
}

impl From<ApiError> for Response {
    fn from(error: ApiError) -> Self {
        error.into_response()
    }
}

/// A `Result` that renders its error through `IntoResponse`, for handlers that
/// can use `?` instead of matching on every step.
pub(crate) type ApiResult<T> = Result<T, ApiError>;

/// Run blocking SQLite work on Tokio's blocking pool so queries and Argon2
/// hashing never occupy a reactor worker. Panics inside the task surface as a
/// generic 500 rather than killing the connection.
pub(crate) async fn run_db<F>(state: AppState, task: F) -> Response
where
    F: FnOnce(&Pool) -> ApiResult<Response> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || task(&state.db)).await {
        Ok(Ok(response)) => response,
        Ok(Err(api_error)) => api_error.into_response(),
        Err(_) => ApiError::Internal("internal server error").into_response(),
    }
}
