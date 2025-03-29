use mussubotti::components::google_calendar::models::CalendarEvent;
use mussubotti::error::BotResult;
use mussubotti::config::Config;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Mock implementation of Google Calendar actor handle for testing
#[derive(Debug, Clone, Default)]
pub struct MockGoogleCalendarHandle {
    events: Vec<CalendarEvent>,
}

impl MockGoogleCalendarHandle {
    /// Create a new mock handle with predefined events
    pub fn new() -> Self {
        let events = vec![
            CalendarEvent {
                id: "event1".to_string(),
                summary: Some("Test Event 1".to_string()),
                description: Some("Test Description 1".to_string()),
                created: Some("2023-01-01T00:00:00Z".to_string()),
                start_date_time: Some("2023-01-01T10:00:00Z".to_string()),
                end_date_time: Some("2023-01-01T11:00:00Z".to_string()),
                ..Default::default()
            },
            CalendarEvent {
                id: "event2".to_string(),
                summary: Some("Test Event 2".to_string()),
                description: Some("Test Description 2".to_string()),
                created: Some("2023-01-02T00:00:00Z".to_string()),
                start_date_time: Some("2023-01-02T10:00:00Z".to_string()),
                end_date_time: Some("2023-01-02T11:00:00Z".to_string()),
                ..Default::default()
            },
        ];
        
        Self { events }
    }
    
    /// Get upcoming events from the mock
    pub async fn get_upcoming_events(&self) -> BotResult<Vec<CalendarEvent>> {
        Ok(self.events.clone())
    }
    
    /// Simulate checking for new events
    pub async fn check_new_events(&self) -> BotResult<Vec<CalendarEvent>> {
        // In a real implementation, this would compare with previous events
        // Here we just return a subset of the events as "new"
        Ok(vec![self.events[0].clone()])
    }
    
    /// Shutdown the mock
    #[allow(dead_code)]
    pub async fn shutdown(&self) -> BotResult<()> {
        Ok(())
    }
}

/// Test that demonstrates how to use the mock
#[tokio::test]
async fn test_google_calendar_mock() {
    // Create the mock
    let mock_handle = MockGoogleCalendarHandle::new();
    
    // Get events from the mock
    let events = mock_handle.get_upcoming_events().await.unwrap();
    
    // Verify events
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, "event1");
    assert_eq!(events[1].id, "event2");
    
    // Check new events
    let new_events = mock_handle.check_new_events().await.unwrap();
    assert_eq!(new_events.len(), 1);
    assert_eq!(new_events[0].id, "event1");
}

/// Test the full configuration and calendar service
#[tokio::test]
async fn test_calendar_with_config() {
    // Create a test configuration
    let config = Arc::new(RwLock::new(Config {
        discord_token: "test_token".to_string(),
        google_client_id: "test_client_id".to_string(),
        google_client_secret: "test_client_secret".to_string(),
        google_calendar_id: "test_calendar_id".to_string(),
        calendar_channel_id: 123456789,
        guild_id: 987654321,
        components: std::collections::HashMap::new(),
        timezone: "UTC".to_string(),
        activity: "Testing".to_string(),
        redis_url: "redis://127.0.0.1:6379".to_string(),
        daily_notification_time: "06:00".to_string(),
        weekly_notification_time: "06:00".to_string(),
        bot_locale: "en-US".to_string(),
    }));
    
    // Create a mock calendar handle
    let mock_handle = MockGoogleCalendarHandle::new();
    
    // Test reading calendar ID from config
    let calendar_id = {
        let config_guard = config.read().await;
        config_guard.google_calendar_id.clone()
    };
    
    assert_eq!(calendar_id, "test_calendar_id");
    
    // Test getting events
    let events = mock_handle.get_upcoming_events().await.unwrap();
    assert!(!events.is_empty());
} 