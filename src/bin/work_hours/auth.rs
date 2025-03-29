use axum::http::StatusCode;
use axum::{
    http::request::Parts,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;
use axum::http::header;
use axum::response::Redirect;

/// User credentials structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Name (username)
    pub name: Option<String>,
    /// Role (admin, user, etc.)
    pub role: String,
    /// Expiration time (as UTC timestamp)
    pub exp: usize,
    /// Issued at (as UTC timestamp)
    pub iat: usize,
}

/// Authentication configuration
#[derive(Clone)]
pub struct AuthConfig {
    /// JWT secret for signing/verifying tokens
    pub jwt_secret: String,
    /// Token expiration time in minutes
    pub token_expiration_minutes: i64,
    /// Admin username
    pub admin_username: String,
    /// Admin password
    pub admin_password: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| "super_secret_key".to_string()),
            token_expiration_minutes: 60 * 24, // 24 hours
            admin_username: std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string()),
            admin_password: std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "password".to_string()),
        }
    }
}

/// Authentication error
#[derive(Debug)]
pub enum AuthError {
    /// Token is missing
    MissingToken,
    /// Token is invalid
    InvalidToken,
    /// Token is expired
    TokenExpired,
    /// User not authorized for this action
    Unauthorized,
    /// Some other error
    Other(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::MissingToken | AuthError::InvalidToken | AuthError::TokenExpired => {
                // For authentication errors, redirect to login
                Redirect::to("/login").into_response()
            },
            AuthError::Unauthorized => {
                // For authorization errors, return forbidden
                (StatusCode::FORBIDDEN, "Not authorized").into_response()
            },
            AuthError::Other(err) => {
                // For other errors, log and return internal server error
                error!("Auth error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            },
        }
    }
}

/// JWT extractor for authentication
#[derive(Debug, Clone)]
pub struct JwtAuth {
    pub claims: Claims,
}

/// A simpler function to extract JWT token from request
pub fn extract_token(parts: &Parts) -> Result<String, AuthError> {
    // First check for token in cookies
    let cookie_header = parts.headers.get(header::COOKIE);
    let mut token = None;
    
    if let Some(cookie) = cookie_header {
        let cookie_str = cookie.to_str().map_err(|_| AuthError::InvalidToken)?;
        for cookie_pair in cookie_str.split(';') {
            let mut parts = cookie_pair.trim().split('=');
            if let (Some("auth_token"), Some(value)) = (parts.next(), parts.next()) {
                token = Some(value.to_string());
                break;
            }
        }
    }
    
    // If no token in cookie, check Authorization header
    if token.is_none() {
        let auth_header = parts
            .headers
            .get("Authorization")
            .ok_or(AuthError::MissingToken)?;

        let auth_str = auth_header.to_str().map_err(|_| AuthError::InvalidToken)?;

        if !auth_str.starts_with("Bearer ") {
            return Err(AuthError::InvalidToken);
        }

        token = Some(auth_str.trim_start_matches("Bearer ").trim().to_string());
    }
    
    token.ok_or(AuthError::MissingToken)
}

/// Auth service for token operations
pub struct AuthService {
    config: Arc<AuthConfig>,
}

impl AuthService {
    /// Create a new auth service
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Get the auth config
    pub fn config(&self) -> Arc<AuthConfig> {
        self.config.clone()
    }

    /// Authenticate a user
    pub fn authenticate(&self, username: &str, password: &str) -> Result<String, AuthError> {
        // Check credentials against configured admin user
        if username == self.config.admin_username && password == self.config.admin_password {
            self.generate_token(username, Some(username.to_string()), "admin")
                .map_err(AuthError::Other)
        } else {
            Err(AuthError::Unauthorized)
        }
    }

    /// Generate a new JWT token
    pub fn generate_token(&self, user_id: &str, name: Option<String>, role: &str) -> Result<String, String> {
        let now = Utc::now();
        let exp = now + Duration::minutes(self.config.token_expiration_minutes);

        let claims = Claims {
            sub: user_id.to_string(),
            name,
            role: role.to_string(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )
        .map_err(|e| format!("Failed to generate token: {}", e))
    }

    /// Validate a JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map(|token_data| token_data.claims)
        .map_err(|e| {
            error!("JWT validation error: {:?}", e);
            AuthError::InvalidToken
        })
    }
}
