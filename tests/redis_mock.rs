use mussubotti::components::google_calendar::models::CalendarEvent;
use mussubotti::error::{google_calendar_error, BotResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Mock implementation of Redis for testing
#[derive(Debug, Clone, Default)]
pub struct MockRedis {
    data: Arc<Mutex<HashMap<String, String>>>,
}

impl MockRedis {
    /// Create a new mock Redis instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Save events to the mock Redis
    pub async fn save_events(&self, events: Vec<CalendarEvent>) -> BotResult<()> {
        let events_json = serde_json::to_string(&events)
            .map_err(|e| google_calendar_error(&format!("Failed to serialize events: {e}")))?;
        let mut data = self.data.lock().await;
        data.insert("google_calendar_events".to_string(), events_json);
        Ok(())
    }

    /// Get events from the mock Redis
    pub async fn get_events(&self) -> BotResult<Vec<CalendarEvent>> {
        let data = self.data.lock().await;

        if let Some(events_json) = data.get("google_calendar_events") {
            let events: Vec<CalendarEvent> = serde_json::from_str(events_json).map_err(|e| {
                google_calendar_error(&format!("Failed to deserialize events: {e}"))
            })?;
            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    /// Save a token to the mock Redis
    pub async fn save_token(&self, token: serde_json::Value) -> BotResult<()> {
        let token_json = token.to_string();
        let mut data = self.data.lock().await;
        data.insert("google_calendar_token".to_string(), token_json);
        Ok(())
    }

    /// Get a token from the mock Redis
    pub async fn get_token(&self) -> BotResult<Option<serde_json::Value>> {
        let data = self.data.lock().await;

        if let Some(token_json) = data.get("google_calendar_token") {
            let token: serde_json::Value = serde_json::from_str(token_json).map_err(|e| {
                google_calendar_error(&format!("Failed to deserialize token: {e}"))
            })?;
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }
}

/// Basic test for the Redis mock
#[tokio::test]
async fn test_redis_mock() {
    // Create a new mock Redis
    let mock_redis = MockRedis::new();

    // Create some test events
    let events = vec![CalendarEvent {
        id: "event1".to_string(),
        summary: Some("Test Event 1".to_string()),
        description: Some("Test Description 1".to_string()),
        created: Some("2023-01-01T00:00:00Z".to_string()),
        start_date_time: Some("2023-01-01T10:00:00Z".to_string()),
        start_date: None,
        end_date_time: Some("2023-01-01T11:00:00Z".to_string()),
        end_date: None,
    }];

    // Save events to Redis
    mock_redis.save_events(events.clone()).await.unwrap();

    // Get events back
    let retrieved_events = mock_redis.get_events().await.unwrap();

    // Verify events match
    assert_eq!(retrieved_events.len(), 1);
    assert_eq!(retrieved_events[0].id, "event1");

    // Test token storage
    let token = serde_json::json!({
        "access_token": "test_token",
        "refresh_token": "test_refresh",
        "expires_in": 3600
    });

    // Save token
    mock_redis.save_token(token.clone()).await.unwrap();

    // Get token back
    let retrieved_token = mock_redis.get_token().await.unwrap();

    // Verify token
    assert!(retrieved_token.is_some());
    if let Some(token_value) = retrieved_token {
        assert_eq!(token_value["access_token"], "test_token");
    }
}
