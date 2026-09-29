//! Library surface for the `ib` simulation trading platform.
//!
//! The binary in `main.rs` is a thin CLI over these modules. Exposing them as
//! a library lets integration tests in `tests/` drive the real router against
//! a real SQLite database instead of duplicating the wiring.
//!
//! Module map:
//! - [`db`]      -- schema migrations and all SQL
//! - [`models`]  -- domain structs, `Decimal` serialized as a string
//! - [`auth`]    -- registration, login, sessions, email verification
//! - [`trading`] -- the `/api/trading` simulation endpoints
//! - [`http`]    -- shared `ApiError` and the blocking bridge
//! - [`web`]     -- embedded static assets
//! - [`email`]   -- Resend adapter

pub mod auth;
pub mod db;
pub mod email;
pub mod http;
pub mod models;
pub mod trading;
pub mod web;

use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;

/// Build the full application router for a given pool.
///
/// `auth::serve` uses this; tests call it directly to drive the same routes
/// without binding a socket.
pub fn app_router(pool: db::Pool) -> Router {
    let state = auth::AppState { db: Arc::new(pool) };
    Router::new()
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
        .route("/api/health", get(auth::health))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/verify", post(auth::verify))
        .route(
            "/api/auth/resend-verification",
            post(auth::resend_verification),
        )
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .merge(trading::router())
        .with_state(state)
}

use axum::routing::{get, post};

/// Run the HTTP server until a shutdown signal arrives.
pub async fn serve(pool: db::Pool, address: &str) {
    let address: SocketAddr = address
        .parse()
        .unwrap_or_else(|_| panic!("invalid server address: {address}"));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {address}: {e}"));
    let app = app_router(pool);

    println!("simulation auth API listening on http://{address}");
    // Without an explicit shutdown signal, `systemctl stop` closes the
    // listener and drops in-flight requests -- including ledger writes that
    // are mid-transaction inside `spawn_blocking`. Drain first, then exit.
    axum::serve(listener, app)
        .with_graceful_shutdown(auth::shutdown_signal())
        .await
        .unwrap_or_else(|e| panic!("HTTP server failed: {e}"));
    println!("shutdown complete");
}
