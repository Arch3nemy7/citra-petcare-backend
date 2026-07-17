pub mod dto;
pub mod extractor;
pub mod handlers;
pub mod jwt;
pub mod password;
pub mod repo;
pub mod service;

pub use extractor::{AuthUser, require_auth};

/// Auth domain errors. Every variant maps to 401 so responses never reveal
/// whether the email exists, the password was wrong, or a token was merely
/// expired. Infrastructure failures (hashing, signing) are `AppError::Internal`,
/// never an auth variant, so the 401 mapping stays total.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid email or password")]
    InvalidCredentials,
    #[error("missing or malformed Authorization header")]
    MissingToken,
    #[error("invalid or expired access token")]
    InvalidToken,
    #[error("refresh token is invalid, expired, or revoked")]
    InvalidRefreshToken,
}
