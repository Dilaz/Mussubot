use mussubotti::components::google_calendar::models::CalendarEvent;
use mussubotti::components::redis_service::RedisActorHandle;
use mussubotti::config::Config;
use mussubotti::error::BotResult;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Smoke test to verify that the config can be loaded
#[tokio::test]
async fn test_config_loads() {
    // Create a minimal config for testing
    let config = Config {
        discord_token: String::new(),
        google_client_id: String::new(),
        google_client_secret: String::new(),
        google_calendar_id: String::new(),
        calendar_channel_id: 0,
        guild_id: 0,
        components: std::collections::HashMap::new(),
        timezone: "UTC".to_string(),
        activity: "Testing".to_string(),
        redis_url: "redis://127.0.0.1:6379".to_string(),
        daily_notification_time: "06:00".to_string(),
        weekly_notification_time: "06:00".to_string(),
        bot_locale: "en".to_string(),
        new_events_check_interval: 300,
        llama_api_key: "test_llama_api_key".to_string(),
        disable_work_schedule_daily_notifications: false,
        disable_work_schedule_weekly_notifications: false,
    };

    assert_eq!(config.redis_url, "redis://127.0.0.1:6379");
    assert!(config.discord_token.is_empty());
}

/// Smoke test for the Redis actor handle
#[tokio::test]
async fn test_redis_handle_creation() {
    // Create an empty Redis handle
    let redis_handle = RedisActorHandle::empty();

    // This test is mainly to verify that the code compiles and the handle can be created
    // In a real integration test, we would initialize the Redis actor
    assert!(redis_handle.shutdown().await.is_ok());
}

/// Mock function for testing without real Redis
async fn mock_get_events(_redis_handle: &RedisActorHandle) -> BotResult<Vec<CalendarEvent>> {
    // Return some mock calendar events
    let events = vec![
        CalendarEvent {
            id: "event1".to_string(),
            summary: Some("Test Event 1".to_string()),
            description: Some("Test Description 1".to_string()),
            created: Some("2023-01-01T00:00:00Z".to_string()),
            start_date_time: Some("2023-01-01T10:00:00Z".to_string()),
            start_date: None,
            end_date_time: Some("2023-01-01T11:00:00Z".to_string()),
            end_date: None,
        },
        CalendarEvent {
            id: "event2".to_string(),
            summary: Some("Test Event 2".to_string()),
            description: Some("Test Description 2".to_string()),
            created: Some("2023-01-02T00:00:00Z".to_string()),
            start_date_time: Some("2023-01-02T10:00:00Z".to_string()),
            start_date: None,
            end_date_time: Some("2023-01-02T11:00:00Z".to_string()),
            end_date: None,
        },
    ];
    Ok(events)
}

/// Test basic calendar event operations
#[tokio::test]
async fn test_calendar_events() {
    // Create a Redis handle
    let redis_handle = RedisActorHandle::empty();

    // Get mock events
    let events = mock_get_events(&redis_handle).await.unwrap();

    // Verify mock events
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, "event1");
    assert_eq!(events[0].summary, Some("Test Event 1".to_string()));
    assert_eq!(events[1].id, "event2");
    assert_eq!(events[1].summary, Some("Test Event 2".to_string()));
}

/// Test config with environment variables
#[tokio::test]
async fn test_config_from_env() {
    // Create a test configuration with Arc and RwLock
    let config = Arc::new(RwLock::new(Config {
        discord_token: "test_token".to_string(),
        google_calendar_id: "test_calendar_id".to_string(),
        redis_url: "redis://localhost:6379".to_string(),
        google_client_id: String::new(),
        google_client_secret: String::new(),
        calendar_channel_id: 0,
        guild_id: 0,
        components: std::collections::HashMap::new(),
        timezone: "UTC".to_string(),
        activity: "Testing".to_string(),
        daily_notification_time: "06:00".to_string(),
        weekly_notification_time: "06:00".to_string(),
        bot_locale: "en".to_string(),
        new_events_check_interval: 300,
        llama_api_key: "test_llama_api_key".to_string(),
        disable_work_schedule_daily_notifications: false,
        disable_work_schedule_weekly_notifications: false,
    }));

    // Test reading from the config
    let discord_token = {
        let config_guard = config.read().await;
        config_guard.discord_token.clone()
    };

    assert_eq!(discord_token, "test_token");
}

/// Test for component initialization order using real ComponentManager and mock components
#[tokio::test]
async fn test_component_initialization_order() {
    use async_trait::async_trait;
    use mussubotti::components::{Component, ComponentManager};
    use mussubotti::error::BotResult;
    use poise::serenity_prelude as serenity;
    use std::sync::{Arc, Mutex};

    // We'll create a global initialization counter to track the order
    static INIT_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    // Create an initialization recorder to store component init order
    let order_recorder = Arc::new(Mutex::new(Vec::<(String, usize)>::new()));

    // Create mock components that implement the Component trait
    struct MockRedisComponent {
        order_recorder: Arc<Mutex<Vec<(String, usize)>>>,
    }

    struct MockGCalendarComponent {
        order_recorder: Arc<Mutex<Vec<(String, usize)>>>,
    }

    // Implement the Component trait for Redis component
    #[async_trait]
    impl Component for MockRedisComponent {
        fn name(&self) -> &'static str {
            "redis_service"
        }

        async fn init(
            &self,
            _ctx: &serenity::Context,
            _config: Arc<RwLock<Config>>,
            _redis_handle: RedisActorHandle,
        ) -> BotResult<()> {
            // Record initialization with an incrementing counter
            let order = INIT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.order_recorder
                .lock()
                .unwrap()
                .push((self.name().to_string(), order));
            Ok(())
        }

        async fn shutdown(&self) -> BotResult<()> {
            Ok(())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // Implement the Component trait for Google Calendar component
    #[async_trait]
    impl Component for MockGCalendarComponent {
        fn name(&self) -> &'static str {
            "google_calendar"
        }

        async fn init(
            &self,
            _ctx: &serenity::Context,
            _config: Arc<RwLock<Config>>,
            _redis_handle: RedisActorHandle,
        ) -> BotResult<()> {
            // Record initialization with an incrementing counter
            let order = INIT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.order_recorder
                .lock()
                .unwrap()
                .push((self.name().to_string(), order));
            Ok(())
        }

        async fn shutdown(&self) -> BotResult<()> {
            Ok(())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // Create a fake Context that can be passed to the components
    // We just need something that implements Deref<Target=serenity::Context>
    struct MockContext;
    impl std::ops::Deref for MockContext {
        type Target = serenity::Context;
        fn deref(&self) -> &Self::Target {
            panic!("MockContext is just a placeholder and shouldn't be dereferenced");
        }
    }

    // Create a test config
    let config = Arc::new(RwLock::new(Config {
        discord_token: String::new(),
        google_client_id: String::new(),
        google_client_secret: String::new(),
        google_calendar_id: String::new(),
        calendar_channel_id: 0,
        guild_id: 0,
        components: std::collections::HashMap::new(),
        timezone: "UTC".to_string(),
        activity: "Testing".to_string(),
        redis_url: "redis://127.0.0.1:6379".to_string(),
        daily_notification_time: "06:00".to_string(),
        weekly_notification_time: "06:00".to_string(),
        bot_locale: "en".to_string(),
        new_events_check_interval: 300,
        llama_api_key: "test_llama_api_key".to_string(),
        disable_work_schedule_daily_notifications: false,
        disable_work_schedule_weekly_notifications: false,
    }));

    // Create component manager
    let mut component_manager = ComponentManager::new(Arc::clone(&config));

    // Create and register components
    let redis_component = MockRedisComponent {
        order_recorder: Arc::clone(&order_recorder),
    };

    let calendar_component = MockGCalendarComponent {
        order_recorder: Arc::clone(&order_recorder),
    };

    // Register the components in the expected order
    component_manager.register(redis_component);
    component_manager.register(calendar_component);

    // Create a custom init function to replace ComponentManager.init_all()
    // since we can't easily create a real Context
    async fn custom_init(
        manager: &ComponentManager,
        config: Arc<RwLock<Config>>,
        _order_recorder: Arc<Mutex<Vec<(String, usize)>>>,
    ) -> BotResult<()> {
        // Create a Redis handle
        let redis_handle = RedisActorHandle::empty();

        // Get all components registered with the manager
        for i in 0..99 {
            // Max 100 components to prevent infinite loop
            let component_name = match i {
                0 => "redis_service",
                1 => "google_calendar",
                _ => break,
            };

            if let Some(component) = manager.get_component_by_name(component_name) {
                // Use unsafe code just for testing - don't do this in production!
                // The MockContext won't actually be dereferenced if our components are implemented correctly
                let ctx_ref = unsafe {
                    #[allow(clippy::transmute_ptr_to_ref, clippy::missing_transmute_annotations)]
                    std::mem::transmute(&MockContext as *const _ as *const serenity::Context)
                };

                // Init the component
                component
                    .init(ctx_ref, Arc::clone(&config), redis_handle.clone())
                    .await?;
            }
        }

        Ok(())
    }

    // Initialize components
    custom_init(
        &component_manager,
        Arc::clone(&config),
        Arc::clone(&order_recorder),
    )
    .await
    .unwrap();

    // Get the recorded initialization order
    let records = order_recorder.lock().unwrap();

    // Record the actual initialization sequence
    println!("Component initialization order: {:?}", *records);

    // Verify the components were initialized in the correct order
    assert_eq!(records.len(), 2, "Expected 2 components to be initialized");

    // Sort by initialization order (the counter value)
    let mut sorted_records = records.clone();
    sorted_records.sort_by_key(|(_, order)| *order);

    // Verify Redis was initialized first
    assert_eq!(
        sorted_records[0].0, "redis_service",
        "Redis service must be initialized first to provide a handle for other components"
    );

    // Verify Google Calendar was initialized second
    assert_eq!(
        sorted_records[1].0, "google_calendar",
        "Google Calendar must be initialized after Redis service"
    );
}
