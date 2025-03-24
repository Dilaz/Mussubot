use serde_json::{json, Value};
use chrono::Utc;
use reqwest::Client;
use tokio::sync::RwLock;
use std::sync::Arc;
use redis::{Client as RedisClient, AsyncCommands};
use crate::config::Config;
use crate::error::{BotResult, google_calendar_error};

#[derive(Clone)]
pub struct TokenManager {
    config: Arc<RwLock<Config>>,
    redis_key: String,
    client: Client,
    redis: RedisClient,
}

impl TokenManager {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        // Set up Redis client with default localhost connection
        let redis = RedisClient::open("redis://127.0.0.1/").expect("Failed to create Redis client");
        
        Self {
            config,
            redis_key: "google_calendar_token".to_string(),
            client: Client::new(),
            redis,
        }
    }

    /// Get OAuth token, either from Redis or by requesting a new one
    pub async fn get_token(&self) -> BotResult<Value> {
        // Try to get token from Redis
        let mut redis_conn = self.redis.get_async_connection().await
            .map_err(|e| google_calendar_error(&format!("Failed to connect to Redis: {}", e)))?;
        
        // Check if token exists in Redis
        let token_exists: bool = redis_conn.exists(&self.redis_key).await
            .map_err(|e| google_calendar_error(&format!("Redis error: {}", e)))?;
            
        if token_exists {
            // Get token from Redis
            let token_str: String = redis_conn.get(&self.redis_key).await
                .map_err(|e| google_calendar_error(&format!("Failed to read token from Redis: {}", e)))?;
            
            let token: Value = serde_json::from_str(&token_str)
                .map_err(|e| google_calendar_error(&format!("Failed to parse token JSON: {}", e)))?;
            
            // Check if token is expired
            if let Some(expiry) = token.get("expires_at").and_then(|v| v.as_i64()) {
                let now = Utc::now().timestamp();
                if expiry > now {
                    return Ok(token);
                }
                // Token is expired, refresh it
                return self.refresh_token(&token).await;
            }
        }
        
        // No token in Redis or no expiry, return error - manual setup required
        Err(google_calendar_error("No valid token found. Please set up token manually in Redis."))
    }
    
    /// Refresh an expired token
    async fn refresh_token(&self, token: &Value) -> BotResult<Value> {
        let refresh_token = token.get("refresh_token")
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
        
        let response = self.client.post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| google_calendar_error(&format!("Failed to refresh token: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(google_calendar_error(&format!(
                "Failed to refresh token: HTTP {} - {}",
                status,
                error_body
            )));
        }
        
        let new_token: Value = response.json().await
            .map_err(|e| google_calendar_error(&format!("Failed to parse token response: {}", e)))?;
        
        // Check for required fields
        if !new_token.get("access_token").is_some() {
            return Err(google_calendar_error("Token response missing 'access_token' field"));
        }
        
        // Combine new access token with existing refresh token
        let mut token_data = serde_json::Map::new();
        token_data.insert("access_token".to_string(), new_token.get("access_token").cloned().unwrap());
        token_data.insert("refresh_token".to_string(), json!(refresh_token));
        
        // Calculate expiry
        let expires_in = new_token.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
        let expires_at = Utc::now().timestamp() + expires_in;
        token_data.insert("expires_at".to_string(), json!(expires_at));
        
        // Save token to Redis
        let token_json = json!(token_data);
        let mut redis_conn = self.redis.get_async_connection().await
            .map_err(|e| google_calendar_error(&format!("Failed to connect to Redis: {}", e)))?;
        
        redis_conn.set(&self.redis_key, token_json.to_string()).await
            .map_err(|e| google_calendar_error(&format!("Failed to save token to Redis: {}", e)))?;
        
        Ok(token_json)
    }
    
    /// Manually set token in Redis (to be called from an admin command)
    pub async fn set_token(&self, token_json: Value) -> BotResult<()> {
        let mut redis_conn = self.redis.get_async_connection().await
            .map_err(|e| google_calendar_error(&format!("Failed to connect to Redis: {}", e)))?;
        
        redis_conn.set(&self.redis_key, token_json.to_string()).await
            .map_err(|e| google_calendar_error(&format!("Failed to save token to Redis: {}", e)))?;
        
        Ok(())
    }
} 