use miette::{Diagnostic, Result};
use thiserror::Error;

/// Main error type for the application
#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("Discord API error: {0}")]
    #[diagnostic(code(mussubot::discord_api))]
    DiscordApi(#[from] serenity::Error),
    
    #[error("Poise framework error: {0}")]
    #[diagnostic(code(mussubot::poise))]
    Poise(#[from] Box<dyn std::error::Error + Send + Sync>),
    
    #[error("Environment error: {0}")]
    #[diagnostic(code(mussubot::environment))]
    Environment(String),
    
    #[error("Configuration error: {0}")]
    #[diagnostic(code(mussubot::config))]
    Config(String),
    
    #[error("Google Calendar API error: {0}")]
    #[diagnostic(code(mussubot::google_calendar))]
    GoogleCalendar(String),
    
    #[error("Component error: {0}")]
    #[diagnostic(code(mussubot::component))]
    Component(String),
    
    #[error(transparent)]
    #[diagnostic(code(mussubot::io))]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    #[diagnostic(code(mussubot::serialization))]
    Serialization(String),
    
    #[error("Other error: {0}")]
    #[diagnostic(code(mussubot::other))]
    Other(String),
}

// Implement From for TOML serialization errors
impl From<toml::ser::Error> for Error {
    fn from(err: toml::ser::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

// Implement From for TOML deserialization errors
impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

/// Type alias for Result with our Error type
pub type BotResult<T> = Result<T, Error>;

/// Helper to create environment errors
pub fn env_error(var: &str) -> Error {
    Error::Environment(format!("Missing environment variable: {}", var))
}

/// Helper to create configuration errors
#[allow(dead_code)]
pub fn config_error(message: &str) -> Error {
    Error::Config(message.to_string())
}

/// Helper to create component errors
#[allow(dead_code)]
pub fn component_error(message: &str) -> Error {
    Error::Component(message.to_string())
}

/// Helper to create Google Calendar errors
pub fn google_calendar_error(message: &str) -> Error {
    Error::GoogleCalendar(message.to_string())
}

/// Helper to create other errors
#[allow(dead_code)]
pub fn other_error(message: &str) -> Error {
    Error::Other(message.to_string())
} 