use axum::{
    http,
    response::{IntoResponse, Response},
};
pub type WebResult<T> = std::result::Result<T, WebError>;

#[derive(thiserror::Error, Debug)]
pub enum WebError {
    #[error("Internal Server Error: {0}")]
    Internal(#[from] anyhow::Error),
    #[error("Templating error: {0:#}")]
    Template(#[from] minijinja::Error),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Not found")]
    NotFound,
    // Potentially more error types in the future
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        // In development, we want to return the error message
        // In production, we want to return a generic error message
        let display = self.to_string();
        match self {
            WebError::Internal(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, display).into_response()
            }
            WebError::Template(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, display).into_response()
            }
            // Auth failures are always explained
            WebError::Auth(_) => (http::StatusCode::UNAUTHORIZED, self.to_string()).into_response(),
            WebError::NotFound => (http::StatusCode::NOT_FOUND, "Not Found").into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_status_codes() {
        let cases = vec![
            (
                WebError::NotFound,
                http::StatusCode::NOT_FOUND,
            ),
            (
                WebError::Auth("bad token".into()),
                http::StatusCode::UNAUTHORIZED,
            ),
            (
                WebError::Internal(anyhow::anyhow!("boom")),
                http::StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, expected_status) in cases {
            let response = error.into_response();
            assert_eq!(response.status(), expected_status);
        }
    }
}
