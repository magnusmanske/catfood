use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use wikimisc::mysql_async::{self, FromRowError};
use wikimisc::toolforge;

/// Messages shown to users are fixed strings, so input is never echoed back.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(&'static str),
    #[error("{0}")]
    NotFound(&'static str),
    #[error("Unknown wiki")]
    UnknownWiki,
    #[error("database error: {0}")]
    Database(toolforge::Error),
    #[error("unexpected database row: {0}")]
    Row(#[from] FromRowError),
    #[error("invalid timestamp in database: {0}")]
    Timestamp(#[from] chrono::ParseError),
}

impl From<toolforge::Error> for AppError {
    fn from(error: toolforge::Error) -> Self {
        match error {
            toolforge::Error::UnknownDatabase(_) => Self::UnknownWiki,
            error => Self::Database(error),
        }
    }
}

impl From<mysql_async::Error> for AppError {
    fn from(error: mysql_async::Error) -> Self {
        Self::Database(error.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) | Self::UnknownWiki => StatusCode::NOT_FOUND,
            Self::Database(_) => {
                tracing::error!("{self}");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Database unavailable or query too slow, please try again later.",
                )
                    .into_response();
            }
            Self::Row(_) | Self::Timestamp(_) => {
                tracing::error!("{self}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error.").into_response();
            }
        };
        (status, self.to_string()).into_response()
    }
}
