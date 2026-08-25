use axum::{
    http::header,
    response::{IntoResponse, Response},
};

pub async fn index() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../frontend/dist/index.html"),
    )
        .into_response()
}

pub async fn app_js() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../frontend/dist/assets/app.js"),
    )
        .into_response()
}

pub async fn styles() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../frontend/dist/assets/index.css"),
    )
        .into_response()
}

pub async fn manifest() -> Response {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        include_str!("../frontend/dist/manifest.webmanifest"),
    )
        .into_response()
}

pub async fn service_worker() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../frontend/dist/sw.js"),
    )
        .into_response()
}

pub async fn icon_192() -> Response {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../frontend/dist/icons/icon-192.png").as_slice(),
    )
        .into_response()
}

pub async fn icon_512() -> Response {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../frontend/dist/icons/icon-512.png").as_slice(),
    )
        .into_response()
}
