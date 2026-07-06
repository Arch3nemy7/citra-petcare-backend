pub mod dto;
pub mod extractor;
pub mod handlers;
pub mod jwt;
pub mod password;
pub mod repo;
pub mod service;

pub use extractor::AuthUser;

/// Auth domain errors. Every user-facing variant maps to 401 so responses
/// never reveal whether the email exists, the password was wrong, or a token
/// was merely expired.
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
    #[error("internal auth error: {0}")]
    Internal(String),
}
