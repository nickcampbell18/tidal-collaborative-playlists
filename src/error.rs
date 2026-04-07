use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use thiserror::Error;

use crate::views::layout;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("oauth state mismatch")]
    OAuthStateMismatch,

    #[error("missing oauth cookies")]
    MissingOAuthCookies,

    #[error("tidal api error: {0}")]
    TidalApi(String),

    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("{self}");

        let status = match &self {
            AppError::OAuthStateMismatch | AppError::MissingOAuthCookies => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = layout(
            "Error",
            html! {
                div class="card" {
                    h1 { "Something went wrong" }
                    p { (self.to_string()) }
                    a href="/" { "Go home" }
                }
            },
        );

        (status, body).into_response()
    }
}
