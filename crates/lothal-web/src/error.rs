use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

/// Web-layer error type that renders as an HTML error page.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found")]
    NotFound,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("{0}")]
    Internal(String),
}

impl From<anyhow::Error> for WebError {
    fn from(err: anyhow::Error) -> Self {
        WebError::Internal(err.to_string())
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            WebError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            WebError::NotFound => (StatusCode::NOT_FOUND, "Not found"),
            WebError::BadRequest(_) => (StatusCode::BAD_REQUEST, "Bad request"),
            WebError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error"),
        };

        tracing::error!(%status, error = %self, "request failed");

        let body = format!(
            r#"<!DOCTYPE html>
<html lang="en" class="dark">
<head><meta charset="utf-8"><title>{status} — Lothal</title>
<style>
  body {{ background: #0f1117; color: #e8eaed; font-family: system-ui, sans-serif;
         display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
  .err {{ text-align: center; }}
  h1 {{ font-size: 4rem; margin: 0; color: #f76c6c; }}
  p {{ color: #8b8fa3; font-size: 1.1rem; }}
  a {{ color: #4f9cf7; text-decoration: none; }}
</style></head>
<body><div class="err">
  <h1>{code}</h1>
  <p>{message}</p>
  <p><a href="/">Back to Pulse</a></p>
</div></body></html>"#,
            status = status,
            code = status.as_u16(),
            message = message,
        );

        (status, Html(body)).into_response()
    }
}
