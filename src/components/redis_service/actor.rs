use crate::config::Config;
use crate::error::{BotResult, google_calendar_error};
use crate::components::google_calendar::models::CalendarEvent;
use redis::{Client as RedisClient, AsyncCommands, aio::Connection};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;
use serde_json::Value;

// Redis key constants
pub mod keys {
    pub const GOOGLE_CALENDAR_EVENTS: &str = "google_calendar_events";
    pub const GOOGLE_CALENDAR_TOKEN: &str = "google_calendar_token";
}

/// The Redis actor that processes messages
pub struct RedisActor {
    config: Arc<RwLock<Config>>,
    client: RedisClient,
    command_rx: mpsc::Receiver<RedisCommand>,
}

/// Commands that can be sent to the Redis actor
pub enum RedisCommand {
    SaveEvents(Vec<CalendarEvent>, mpsc::Sender<BotResult<()>>),
    GetEvents(mpsc::Sender<BotResult<Vec<CalendarEvent>>>),
    GetToken(mpsc::Sender<BotResult<Option<Value>>>),
    SaveToken(Value, mpsc::Sender<BotResult<()>>),
    Shutdown,
}

/// Handle for communicating with the Redis actor
#[derive(Clone)]
pub struct RedisActorHandle {
    command_tx: mpsc::Sender<RedisCommand>,
}

impl RedisActorHandle {
    /// Create a new empty handle for initialization purposes
    pub fn empty() -> Self {
        let (command_tx, _) = mpsc::channel(32);
        Self { command_tx }
    }

    /// Save calendar events to Redis
    pub async fn save_events(&self, events: Vec<CalendarEvent>) -> BotResult<()> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx.send(RedisCommand::SaveEvents(events, response_tx)).await
            .map_err(|e| google_calendar_error(&format!("Actor mailbox error: {}", e)))?;
        
        response_rx.recv().await
            .ok_or_else(|| google_calendar_error("Response channel closed"))?
    }

    /// Get calendar events from Redis
    pub async fn get_events(&self) -> BotResult<Vec<CalendarEvent>> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx.send(RedisCommand::GetEvents(response_tx)).await
            .map_err(|e| google_calendar_error(&format!("Actor mailbox error: {}", e)))?;
        
        response_rx.recv().await
            .ok_or_else(|| google_calendar_error("Response channel closed"))?
    }
    
    /// Get token from Redis
    pub async fn get_token(&self) -> BotResult<Option<Value>> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx.send(RedisCommand::GetToken(response_tx)).await
            .map_err(|e| google_calendar_error(&format!("Actor mailbox error: {}", e)))?;
        
        response_rx.recv().await
            .ok_or_else(|| google_calendar_error("Response channel closed"))?
    }
    
    /// Save token to Redis
    pub async fn save_token(&self, token: Value) -> BotResult<()> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx.send(RedisCommand::SaveToken(token, response_tx)).await
            .map_err(|e| google_calendar_error(&format!("Actor mailbox error: {}", e)))?;
        
        response_rx.recv().await
            .ok_or_else(|| google_calendar_error("Response channel closed"))?
    }
    
    /// Shutdown the actor
    pub async fn shutdown(&self) -> BotResult<()> {
        let _ = self.command_tx.send(RedisCommand::Shutdown).await;
        Ok(())
    }
}

impl RedisActor {
    /// Create a new actor and return its handle
    pub fn new(config: Arc<RwLock<Config>>) -> (Self, RedisActorHandle) {
        let (command_tx, command_rx) = mpsc::channel(32);
        
        // Get the default Redis URL - we'll connect to Redis properly in the async methods
        let redis_url = "redis://127.0.0.1:6379".to_string();
        let redis = RedisClient::open(redis_url).expect("Failed to create Redis client");
        
        let actor = Self {
            config,
            client: redis,
            command_rx,
        };
        
        let handle = RedisActorHandle { command_tx };
        
        (actor, handle)
    }
    
    /// Start the actor's processing loop
    pub async fn run(&mut self) {
        info!("Redis actor started");
        
        // Process commands
        while let Some(cmd) = self.command_rx.recv().await {
            match cmd {
                RedisCommand::SaveEvents(events, response_tx) => {
                    let result = self.save_events_to_redis(events).await;
                    let _ = response_tx.send(result).await;
                },
                RedisCommand::GetEvents(response_tx) => {
                    let result = self.get_events_from_redis().await;
                    let _ = response_tx.send(result).await;
                },
                RedisCommand::GetToken(response_tx) => {
                    let result = self.get_token_from_redis().await;
                    let _ = response_tx.send(result).await;
                },
                RedisCommand::SaveToken(token, response_tx) => {
                    let result = self.save_token_to_redis(token).await;
                    let _ = response_tx.send(result).await;
                },
                RedisCommand::Shutdown => {
                    info!("Redis actor shutting down");
                    break;
                }
            }
        }
        
        info!("Redis actor shut down");
    }

    /// Get a redis connection
    async fn get_redis_connection(&self) -> BotResult<Connection> {
        // Get Redis URL from config
        let redis_url = {
            let config_guard = self.config.read().await;
            config_guard.redis_url.clone()
        };
        
        // Reconnect with the proper URL if needed
        let redis = if redis_url != "redis://127.0.0.1:6379" {
            RedisClient::open(redis_url)
                .map_err(|e| google_calendar_error(&format!("Failed to create Redis client: {}", e)))?
        } else {
            self.client.clone()
        };
        
        let result: BotResult<Connection> = redis.get_async_connection().await
            .map_err(|e| google_calendar_error(&format!("Failed to connect to Redis: {}", e)));
        result
    }

    /// Save events to Redis
    async fn save_events_to_redis(&self, events: Vec<CalendarEvent>) -> BotResult<()> {
        // Get Redis connection
        let mut redis_conn = self.get_redis_connection().await?;
        
        // Convert events to JSON
        let events_json = serde_json::to_string(&events)
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to serialize events: {}", e)) })?;
        
        // Save to Redis
        () = redis_conn.set(keys::GOOGLE_CALENDAR_EVENTS, events_json).await
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to save events to Redis: {}", e)) })?;
        
        Ok(())
    }

    /// Get events from Redis
    async fn get_events_from_redis(&self) -> BotResult<Vec<CalendarEvent>> {
        // Get Redis connection
        let mut redis_conn = self.get_redis_connection().await?;
        
        // Check if events exist in Redis
        let exists: bool = redis_conn.exists(keys::GOOGLE_CALENDAR_EVENTS).await
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Redis error: {}", e)) })?;
            
        if !exists {
            return Ok(Vec::new());
        }
        
        // Get events from Redis
        let events_json: String = redis_conn.get(keys::GOOGLE_CALENDAR_EVENTS).await
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to read events from Redis: {}", e)) })?;
        
        // Deserialize events
        let events: Vec<CalendarEvent> = serde_json::from_str(&events_json)
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to deserialize events: {}", e)) })?;
        
        Ok(events)
    }

    /// Get token from Redis
    async fn get_token_from_redis(&self) -> BotResult<Option<Value>> {
        // Get Redis connection
        let mut redis_conn = self.get_redis_connection().await?;
        
        // Check if token exists in Redis
        let exists: bool = redis_conn.exists(keys::GOOGLE_CALENDAR_TOKEN).await
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Redis error: {}", e)) })?;
            
        if !exists {
            return Ok(None);
        }
        
        // Get token from Redis
        let token_json: String = redis_conn.get(keys::GOOGLE_CALENDAR_TOKEN).await
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to read token from Redis: {}", e)) })?;
        
        // Deserialize token
        let token: Value = serde_json::from_str(&token_json)
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to deserialize token: {}", e)) })?;
        
        Ok(Some(token))
    }

    /// Save token to Redis
    async fn save_token_to_redis(&self, token: Value) -> BotResult<()> {
        // Get Redis connection
        let mut redis_conn = self.get_redis_connection().await?;
        
        // Convert token to JSON string
        let token_json = token.to_string();
        
        // Save to Redis
        () = redis_conn.set(keys::GOOGLE_CALENDAR_TOKEN, token_json).await
            .map_err(|e| -> crate::error::Error { google_calendar_error(&format!("Failed to save token to Redis: {}", e)) })?;
        
        Ok(())
    }
} 