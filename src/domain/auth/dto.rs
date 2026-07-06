use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;
use validator::Validate;

use super::service::TokenPair;
use crate::domain::users::dto::UserResponse;

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    #[validate(email)]
    #[schema(example = "citra@citrapetcare.id")]
    pub email: String,
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshRequest {
    /// The opaque 64-character refresh token from a previous login/refresh.
    #[validate(length(equal = 64))]
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogoutRequest {
    /// The refresh token to revoke.
    #[validate(length(equal = 64))]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    /// Short-lived JWT for the `Authorization: Bearer` header.
    pub access_token: String,
    /// Rotating refresh token — shown exactly once, store it securely.
    pub refresh_token: String,
    #[schema(example = "Bearer")]
    pub token_type: String,
    /// Access-token lifetime in seconds.
    pub expires_in: i64,
    pub user: UserResponse,
}

impl From<TokenPair> for TokenResponse {
    fn from(pair: TokenPair) -> Self {
        Self {
            access_token: pair.access_token,
            refresh_token: pair.refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: pair.expires_in,
            user: pair.user.into(),
        }
    }
}
