use crate::config::Config;
use crate::error::{BotResult, google_calendar_error};
use super::models::CalendarEvent;
use super::token::TokenManager;
use crate::components::redis_service::RedisActorHandle;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;
use url::Url;
use reqwest::Client;

/// The Google Calendar actor that processes messages
pub struct GoogleCalendarActor {
    config: Arc<RwLock<Config>>,
    token_manager: TokenManager,
    client: Client,
    command_rx: mpsc::Receiver<GoogleCalendarCommand>,
    redis_handle: RedisActorHandle,
}

/// Commands that can be sent to the Google Calendar actor
pub enum GoogleCalendarCommand {
    GetUpcomingEvents(mpsc::Sender<BotResult<Vec<CalendarEvent>>>),
    CheckNewEvents(mpsc::Sender<BotResult<Vec<CalendarEvent>>>),
    Shutdown,
}

/// Handle for communicating with the Google Calendar actor
#[derive(Clone)]
pub struct GoogleCalendarActorHandle {
    command_tx: mpsc::Sender<GoogleCalendarCommand>,
}

impl GoogleCalendarActorHandle {
    /// Get upcoming events from the calendar
    pub async fn get_upcoming_events(&self) -> BotResult<Vec<CalendarEvent>> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx.send(GoogleCalendarCommand::GetUpcomingEvents(response_tx)).await
            .map_err(|e| google_calendar_error(&format!("Actor mailbox error: {}", e)))?;
        
        response_rx.recv().await
            .ok_or_else(|| google_calendar_error("Response channel closed"))?
    }

    /// Check for new events since last check
    pub async fn check_new_events(&self) -> BotResult<Vec<CalendarEvent>> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx.send(GoogleCalendarCommand::CheckNewEvents(response_tx)).await
            .map_err(|e| google_calendar_error(&format!("Actor mailbox error: {}", e)))?;
        
        response_rx.recv().await
            .ok_or_else(|| google_calendar_error("Response channel closed"))?
    }
    
    /// Shutdown the actor
    pub async fn shutdown(&self) -> BotResult<()> {
        let _ = self.command_tx.send(GoogleCalendarCommand::Shutdown).await;
        Ok(())
    }
}

impl GoogleCalendarActor {
    /// Create a new actor and return its handle
    pub fn new(config: Arc<RwLock<Config>>, redis_handle: RedisActorHandle) -> (Self, GoogleCalendarActorHandle) {
        let (command_tx, command_rx) = mpsc::channel(32);
        
        let actor = Self {
            config: Arc::clone(&config),
            token_manager: TokenManager::new(Arc::clone(&config), redis_handle.clone()),
            client: Client::new(),
            command_rx,
            redis_handle,
        };
        
        let handle = GoogleCalendarActorHandle { command_tx };
        
        (actor, handle)
    }
    
    /// Start the actor's processing loop
    pub async fn run(&mut self) {
        info!("Google Calendar actor started");
        
        // Process commands
        while let Some(cmd) = self.command_rx.recv().await {
            match cmd {
                GoogleCalendarCommand::GetUpcomingEvents(response_tx) => {
                    let result = Self::get_upcoming_events(
                        Arc::clone(&self.config),
                        self.token_manager.clone(),
                        self.client.clone(),
                    ).await;
                    
                    // Save events to Redis if successful
                    if let Ok(events) = &result {
                        let _ = self.redis_handle.save_events(events.clone()).await;
                    }
                    
                    let _ = response_tx.send(result).await;
                },
                GoogleCalendarCommand::CheckNewEvents(response_tx) => {
                    let result = self.check_new_events().await;
                    let _ = response_tx.send(result).await;
                },
                GoogleCalendarCommand::Shutdown => {
                    info!("Google Calendar actor shutting down");
                    break;
                }
            }
        }
        
        info!("Google Calendar actor shut down");
    }

    /// Get upcoming events from the calendar
    pub async fn get_upcoming_events(
        config: Arc<RwLock<Config>>,
        token_manager: TokenManager,
        client: Client,
    ) -> BotResult<Vec<CalendarEvent>> {
        // Get calendar ID from config
        let calendar_id = {
            let config_read = config.read().await;
            config_read.google_calendar_id.clone()
        };
        
        // Get authentication token
        let token = token_manager.get_token().await?;
        let access_token = token.get("access_token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| google_calendar_error("No access token available"))?;
        
        // Calculate time range (from now to 4 weeks in the future)
        let now = Utc::now();
        let time_min = now.to_rfc3339();
        let time_max = (now + chrono::Duration::days(28)).to_rfc3339();
        
        // Build URL with query parameters
        let url_str = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events",
            calendar_id
        );
        
        let mut url = Url::parse(&url_str)
            .map_err(|e| google_calendar_error(&format!("Failed to parse URL: {}", e)))?;
        
        let mut query_params = HashMap::new();
        query_params.insert("timeMin", time_min);
        query_params.insert("timeMax", time_max);
        query_params.insert("singleEvents", "true".to_string());
        query_params.insert("orderBy", "startTime".to_string());
        
        for (key, value) in query_params {
            url.query_pairs_mut().append_pair(key, &value);
        }
        
        // Make API request
        let response = client.get(url)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| google_calendar_error(&format!("Failed to fetch events: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(google_calendar_error(&format!(
                "Failed to fetch events: HTTP {} - {}",
                status,
                error_body
            )));
        }
        
        let response_data: serde_json::Value = response.json().await
            .map_err(|e| google_calendar_error(&format!("Failed to parse events response: {}", e)))?;
        
        // Parse events from response
        let events = response_data.get("items")
            .and_then(|i| i.as_array())
            .ok_or_else(|| google_calendar_error("No items in response"))?;
        
        // Convert to CalendarEvent objects
        let calendar_events = events.iter().map(|event| {
            let id = event.get("id").and_then(|id| id.as_str()).unwrap_or("").to_string();
            let summary = event.get("summary").and_then(|s| s.as_str()).map(|s| s.to_string());
            let description = event.get("description").and_then(|s| s.as_str()).map(|s| s.to_string());
            let created = event.get("created").and_then(|s| s.as_str()).map(|s| s.to_string());
            
            let start_date_time = event.get("start")
                .and_then(|start| start.as_object())
                .and_then(|start| start.get("dateTime"))
                .and_then(|dt| dt.as_str())
                .map(|s| s.to_string());
            
            let start_date = event.get("start")
                .and_then(|start| start.as_object())
                .and_then(|start| start.get("date"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            
            let end_date_time = event.get("end")
                .and_then(|end| end.as_object())
                .and_then(|end| end.get("dateTime"))
                .and_then(|dt| dt.as_str())
                .map(|s| s.to_string());
            
            let end_date = event.get("end")
                .and_then(|end| end.as_object())
                .and_then(|end| end.get("date"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            
            CalendarEvent {
                id,
                summary,
                description,
                created,
                start_date_time,
                start_date,
                end_date_time,
                end_date,
            }
        }).collect();
        
        Ok(calendar_events)
    }

    /// Check for new events since last check
    async fn check_new_events(&self) -> BotResult<Vec<CalendarEvent>> {
        // Get current events from Google Calendar
        let current_events = Self::get_upcoming_events(
            Arc::clone(&self.config),
            self.token_manager.clone(),
            self.client.clone(),
        ).await?;

        // Get last known events from Redis
        let last_known_events = self.redis_handle.get_events().await?;
        let mut new_events = Vec::new();

        // Find new events by comparing with last known events
        for event in &current_events {
            if !last_known_events.iter().any(|e| e.id == event.id) {
                new_events.push(event.clone());
            }
        }

        // Update last known events in Redis
        if !current_events.is_empty() {
            let _ = self.redis_handle.save_events(current_events).await;
        }

        Ok(new_events)
    }
} 