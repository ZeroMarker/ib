//! Static asset handlers for the embedded Vite build.
//!
//! `dist/` is compiled into the binary, so every asset is a compile-time
//! constant. These handlers exist purely to attach the right content type
//! and the no-store cache policy: the payload itself never changes at
//! runtime, but the `?v=` query string that busts the service-worker cache
//! differs per build.

use axum::{
    http::header,
    response::{IntoResponse, Response},
};

/// The page shell and its assets must never be cached by the browser or the
/// service worker: a stale `index.html` pins a user to an old app bundle.
const CACHE_CONTROL: &str = "no-cache, no-store, must-revalidate";

/// Serve an embedded text asset with an explicit content type.
fn text_asset(content_type: &'static str, body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, CACHE_CONTROL),
        ],
        body,
    )
        .into_response()
}

/// Serve an embedded binary asset with an explicit content type.
fn binary_asset(content_type: &'static str, body: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, CACHE_CONTROL),
        ],
        body,
    )
        .into_response()
}

pub async fn index() -> Response {
    text_asset(
        "text/html; charset=utf-8",
        include_str!("../frontend/dist/index.html"),
    )
}

pub async fn app_js() -> Response {
    text_asset(
        "application/javascript; charset=utf-8",
        include_str!("../frontend/dist/assets/app.js"),
    )
}

pub async fn styles() -> Response {
    text_asset(
        "text/css; charset=utf-8",
        include_str!("../frontend/dist/assets/index.css"),
    )
}

pub async fn manifest() -> Response {
    text_asset(
        "application/manifest+json",
        include_str!("../frontend/dist/manifest.webmanifest"),
    )
}

pub async fn service_worker() -> Response {
    // A service worker must be served from the origin root to control the
    // whole scope, and browsers bypass the HTTP cache for it by default.
    text_asset(
        "application/javascript; charset=utf-8",
        include_str!("../frontend/dist/sw.js"),
    )
}

pub async fn icon_192() -> Response {
    binary_asset(
        "image/png",
        include_bytes!("../frontend/dist/icons/icon-192.png"),
    )
}

pub async fn icon_512() -> Response {
    binary_asset(
        "image/png",
        include_bytes!("../frontend/dist/icons/icon-512.png"),
    )
}

pub async fn icon_svg() -> Response {
    text_asset(
        "image/svg+xml",
        include_str!("../frontend/dist/icons/icon.svg"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The browser and the service worker must both be told never to cache,
    /// so assert the header on the real compiled-in payload.
    #[tokio::test]
    async fn static_assets_are_served_no_store() {
        for response in [
            index().await,
            app_js().await,
            styles().await,
            manifest().await,
            service_worker().await,
            icon_192().await,
            icon_512().await,
            icon_svg().await,
        ] {
            let cache = response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok());
            assert_eq!(cache, Some(CACHE_CONTROL));
            assert!(response.headers().contains_key(header::CONTENT_TYPE));
        }
    }

    #[tokio::test]
    async fn binary_icons_keep_a_non_utf8_content_type() {
        // `text_asset` accepts &str, so routing PNG bytes through it would
        // reject the 0x89 prefix as invalid UTF-8. The &[u8] path must be
        // used instead, and must survive the response round-trip.
        let response = binary_asset("image/png", &[0x89, 0x50, 0x4E, 0x47, 0x0D]);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("image/png")
        );
    }
}
