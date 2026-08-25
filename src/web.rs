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
