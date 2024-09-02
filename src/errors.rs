use std::fmt::Display;

use axum::{
    http,
    response::{IntoResponse, Response},
};
use image::error;
use tracing_subscriber::field::display;
pub type WebResult<T> = std::result::Result<T, WebError>;

#[derive(thiserror::Error, Debug)]
pub enum WebError {
    #[error("Internal Server Error: {0}")]
    InternalError(#[from] anyhow::Error),
    #[error("Templating error: {0}")]
    TemplateError(#[from] handlebars::RenderError),
    #[error("Not found")]
    NotFound,
    // Potentially more error types in the future
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        // In development, we want to return the error message
        // In production, we want to return a generic error message
        let display = |err: &dyn Display| {
            if cfg!(debug_assertions) {
                err.to_string()
            } else {
                "Internal Server Error".into()
            }
        };
        match self {
            WebError::InternalError(err) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, display(&err)).into_response()
            }
            WebError::TemplateError(err) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, display(&err)).into_response()
            }
            WebError::NotFound => (http::StatusCode::NOT_FOUND, "Not Found").into_response(),
        }
    }
}
