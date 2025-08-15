use crate::error::{env_error, BotResult};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

/// Default activity text for the bot
pub const DEFAULT_ACTIVITY: &str = "DOTA2";

/// Main configuration structure for the bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Discord bot token
    pub discord_token: String,
    /// Google Calendar API client ID
    pub google_client_id: String,
    /// Google Calendar API client secret
    pub google_client_secret: String,
    /// Google Calendar ID to monitor
    pub google_calendar_id: String,
    /// Discord channel ID to send calendar notifications
    pub calendar_channel_id: u64,
    /// Discord guild ID (server)
    pub guild_id: u64,
    /// Map of component names to their enabled status
    pub components: HashMap<String, bool>,
    /// Timezone for scheduling
    pub timezone: String,
    /// Bot activity status text
    pub activity: String,
    /// Redis connection URL
    pub redis_url: String,
    /// Daily notification time in 24h format (HH:MM)
    pub daily_notification_time: String,
    /// Weekly notification time in 24h format (HH:MM)
    pub weekly_notification_time: String,
    /// Bot locale
    pub bot_locale: String,
    /// Interval in seconds for checking new calendar events (default: 300)
    pub new_events_check_interval: u64,
    /// LlamaIndex API Key
    pub llama_api_key: String,
    /// When true, disables daily work schedule notifications
    pub disable_work_schedule_daily_notifications: bool,
    /// When true, disables weekly work schedule notifications
    pub disable_work_schedule_weekly_notifications: bool,
}

impl Config {
    /// Load configuration from environment and config file
    pub fn load() -> BotResult<Self> {
        // Load .env file if it exists
        dotenv().ok();

        // Required environment variables
        let discord_token = env::var("DISCORD_TOKEN").map_err(|_| env_error("DISCORD_TOKEN"))?;
        let google_client_id =
            env::var("GOOGLE_CLIENT_ID").map_err(|_| env_error("GOOGLE_CLIENT_ID"))?;
        let google_client_secret =
            env::var("GOOGLE_CLIENT_SECRET").map_err(|_| env_error("GOOGLE_CLIENT_SECRET"))?;
        let google_calendar_id =
            env::var("GOOGLE_CALENDAR_ID").map_err(|_| env_error("GOOGLE_CALENDAR_ID"))?;

        // Optional notification times with defaults
        let daily_notification_time =
            env::var("DAILY_NOTIFICATION_TIME").unwrap_or_else(|_| "06:00".to_string());
        let weekly_notification_time =
            env::var("WEEKLY_NOTIFICATION_TIME").unwrap_or_else(|_| "06:00".to_string());

        // New events check interval (default: 5 minutes/300 seconds)
        let new_events_check_interval = env::var("NEW_EVENTS_CHECK_INTERVAL")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300);

        // Parse numeric values
        let calendar_channel_id = env::var("CALENDAR_CHANNEL_ID")
            .map_err(|_| env_error("CALENDAR_CHANNEL_ID"))?
            .parse::<u64>()
            .map_err(|_| env_error("Invalid CALENDAR_CHANNEL_ID format"))?;

        let guild_id = env::var("GUILD_ID")
            .map_err(|_| env_error("GUILD_ID"))?
            .parse::<u64>()
            .map_err(|_| env_error("Invalid GUILD_ID format"))?;

        // Default timezone
        let timezone = env::var("TIMEZONE").unwrap_or_else(|_| String::from("UTC"));

        // Bot activity status
        let activity = env::var("BOT_ACTIVITY").unwrap_or_else(|_| String::from(DEFAULT_ACTIVITY));

        // Redis connection URL
        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| String::from("redis://127.0.0.1:6379"));

        // Bot locale
        let bot_locale = env::var("BOT_LOCALE").unwrap_or_else(|_| "en-US".to_string());

        // LlamaIndex API Key
        let llama_api_key = env::var("LLAMA_API_KEY").unwrap_or_default();

        // Work Schedule daily notifications toggle (default: enabled)
        let disable_work_schedule_daily_notifications =
            env::var("DISABLE_WORK_SCHEDULE_DAILY_NOTIFICATIONS")
                .ok()
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        let disable_work_schedule_weekly_notifications =
            env::var("DISABLE_WORK_SCHEDULE_WEEKLY_NOTIFICATIONS")
                .ok()
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        // Initialize default components
        let mut components = HashMap::new();
        components.insert("google_calendar".to_string(), true);

        // Load components configuration from file if it exists
        if let Ok(content) = fs::read_to_string("config/components.toml") {
            if let Ok(file_components) = toml::from_str::<HashMap<String, bool>>(&content) {
                // Merge with defaults
                for (key, value) in file_components {
                    components.insert(key, value);
                }
            }
        }

        Ok(Config {
            discord_token,
            google_client_id,
            google_client_secret,
            google_calendar_id,
            calendar_channel_id,
            guild_id,
            components,
            timezone,
            activity,
            redis_url,
            daily_notification_time,
            weekly_notification_time,
            bot_locale,
            new_events_check_interval,
            llama_api_key,
            disable_work_schedule_daily_notifications,
            disable_work_schedule_weekly_notifications,
        })
    }

    /// Check if a component is enabled
    #[allow(dead_code)]
    pub fn is_component_enabled(&self, name: &str) -> bool {
        *self.components.get(name).unwrap_or(&false)
    }

    /// Update component enabled status
    #[allow(dead_code)]
    pub fn set_component_enabled(&mut self, name: &str, enabled: bool) -> BotResult<()> {
        self.components.insert(name.to_string(), enabled);
        self.save_components()
    }

    /// Save component configuration to file
    #[allow(dead_code)]
    fn save_components(&self) -> BotResult<()> {
        // Create config directory if it doesn't exist
        if !Path::new("config").exists() {
            fs::create_dir("config")?;
        }

        let toml_str = toml::to_string(&self.components)?;
        fs::write("config/components.toml", toml_str)?;

        Ok(())
    }
}
