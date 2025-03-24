use crate::error::{env_error, BotResult};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use toml;

/// Default activity text for the bot
pub const DEFAULT_ACTIVITY: &str = "Leikkii lankakerällä";

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
}

impl Config {
    /// Load configuration from environment and config file
    pub fn load() -> BotResult<Self> {
        // Load .env file if it exists
        dotenv().ok();
        
        // Required environment variables
        let discord_token = env::var("DISCORD_TOKEN").map_err(|_| env_error("DISCORD_TOKEN"))?;
        let google_client_id = env::var("GOOGLE_CLIENT_ID").map_err(|_| env_error("GOOGLE_CLIENT_ID"))?;
        let google_client_secret = env::var("GOOGLE_CLIENT_SECRET").map_err(|_| env_error("GOOGLE_CLIENT_SECRET"))?;
        let google_calendar_id = env::var("GOOGLE_CALENDAR_ID").map_err(|_| env_error("GOOGLE_CALENDAR_ID"))?;
        
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