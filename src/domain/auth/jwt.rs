use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthError;
use crate::config::Config;
use crate::domain::users::models::{User, UserRole};
use crate::error::AppError;

pub const ISSUER: &str = "citra-petcare";

/// Access-token claims. Kept small: the API trusts these for the token's
/// 15-minute lifetime instead of hitting the users table on every request.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// User id.
    pub sub: Uuid,
    pub email: String,
    pub role: UserRole,
    pub iss: String,
    /// Issued at (unix seconds).
    pub iat: i64,
    /// Expiry (unix seconds).
    pub exp: i64,
    /// Unique token id.
    pub jti: Uuid,
}

pub fn encode_access_token(user: &User, config: &Config) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user.id,
        email: user.email.clone(),
        role: user.role,
        iss: ISSUER.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(config.access_token_ttl_secs)).timestamp(),
        jti: Uuid::now_v7(),
    };
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("failed to sign access token: {e}")))
}

pub fn decode_access_token(token: &str, config: &Config) -> Result<Claims, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    // deliberately collapse all decode failures (bad signature, expired, wrong
    // issuer, garbage) into one opaque 401 error
    .map_err(|_| AuthError::InvalidToken)
}
