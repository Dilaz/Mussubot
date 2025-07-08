use crate::components::redis_service::RedisActorHandle;
use crate::config::Config;
use crate::error::{google_calendar_error, BotResult};
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

// Constants
const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

#[derive(Clone)]
pub struct TokenManager {
    config: Arc<RwLock<Config>>,
    client: Client,
    redis_handle: RedisActorHandle,
}

impl TokenManager {
    pub fn new(config: Arc<RwLock<Config>>, redis_handle: RedisActorHandle) -> Self {
        Self {
            config,
            client: Client::new(),
            redis_handle,
        }
    }

    /// Get OAuth token, either from Redis or by requesting a new one
    pub async fn get_token(&self) -> BotResult<Value> {
        // Try to get token from Redis
        let token_result = self.redis_handle.get_token().await?;

        if let Some(token) = token_result {
            // Check if token is expired or expires soon
            if let Some(expiry) = token.get("expires_at").and_then(|v| v.as_i64()) {
                let now = Utc::now().timestamp();
                let buffer_seconds = 300; // 5 minutes buffer
                if expiry > now + buffer_seconds {
                    // Token is still valid for a reasonable time
                    return Ok(token);
                }
                // Token is expired or will expire soon, refresh it
                return self.refresh_token(&token).await;
            }
        }

        // No token in Redis or no expiry, return error - manual setup required
        Err(google_calendar_error(
            "No valid token found. Please set up token manually.",
        ))
    }

    /// Refresh an expired token
    async fn refresh_token(&self, token: &Value) -> BotResult<Value> {
        let refresh_token = token
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| google_calendar_error("No refresh token in token data"))?;

        let client_id = {
            let config_read = self.config.read().await;
            config_read.google_client_id.clone()
        };

        let client_secret = {
            let config_read = self.config.read().await;
            config_read.google_client_secret.clone()
        };

        let params = [
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token.to_string()),
            ("grant_type", "refresh_token".to_string()),
        ];

        let response = self
            .client
            .post(GOOGLE_OAUTH_TOKEN_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| google_calendar_error(&format!("Failed to refresh token: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(google_calendar_error(&format!(
                "Failed to refresh token: HTTP {status} - {error_body}"
            )));
        }

        let new_token: Value = response
            .json()
            .await
            .map_err(|e| google_calendar_error(&format!("Failed to parse token response: {e}")))?;

        // Check for required fields
        if new_token.get("access_token").is_none() {
            return Err(google_calendar_error(
                "Token response missing 'access_token' field",
            ));
        }

        // Combine new access token with existing refresh token
        let mut token_data = serde_json::Map::new();
        token_data.insert(
            "access_token".to_string(),
            new_token.get("access_token").cloned().unwrap(),
        );
        token_data.insert("refresh_token".to_string(), json!(refresh_token));

        // Calculate expiry
        let expires_in = new_token
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(3600);
        let expires_at = Utc::now().timestamp() + expires_in;
        token_data.insert("expires_at".to_string(), json!(expires_at));

        // Save token to Redis using the Redis actor
        let token_json = json!(token_data);
        self.redis_handle.save_token(token_json.clone()).await?;

        // Return the refreshed token
        Ok(token_json)
    }

    /// Manually set token in Redis (to be called from an admin command)
    #[allow(dead_code)]
    pub async fn set_token(&self, token_json: Value) -> BotResult<()> {
        // Save token using Redis actor
        self.redis_handle.save_token(token_json).await
    }
}
