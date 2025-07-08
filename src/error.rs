use miette::{Diagnostic, Result};
use std::ops::Deref;
use thiserror::Error;

/// Main error type for the application (boxed internally to reduce size)
#[derive(Debug)]
pub struct Error(Box<ErrorImpl>);

/// Internal error implementation
#[derive(Debug, Error, Diagnostic)]
pub enum ErrorImpl {
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

    #[error("Work Schedule error: {0}")]
    #[diagnostic(code(mussubot::work_schedule))]
    WorkSchedule(String),

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

// Implement Deref to provide access to the inner error
impl Deref for Error {
    type Target = ErrorImpl;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Implement Display for Error
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// Implement std::error::Error for Error
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

// Implement miette::Diagnostic for Error
impl miette::Diagnostic for Error {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.0.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.0.source_code()
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.0.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        self.0.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        self.0.diagnostic_source()
    }
}

// Implement From traits for the public Error type
impl From<serenity::Error> for Error {
    fn from(err: serenity::Error) -> Self {
        Error(Box::new(ErrorImpl::DiscordApi(err)))
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error(Box::new(ErrorImpl::Io(err)))
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for Error {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error(Box::new(ErrorImpl::Poise(err)))
    }
}

// Implement From for TOML serialization errors
impl From<toml::ser::Error> for Error {
    fn from(err: toml::ser::Error) -> Self {
        Error(Box::new(ErrorImpl::Serialization(err.to_string())))
    }
}

// Implement From for TOML deserialization errors
impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error(Box::new(ErrorImpl::Serialization(err.to_string())))
    }
}

// Implement From for Redis errors
impl From<redis::RedisError> for Error {
    fn from(err: redis::RedisError) -> Self {
        Error(Box::new(ErrorImpl::Other(format!("Redis error: {err}"))))
    }
}

// Implement From for reqwest errors
impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error(Box::new(ErrorImpl::Other(format!(
            "HTTP client error: {err}"
        ))))
    }
}

/// Type alias for Result with our Error type
pub type BotResult<T> = Result<T, Error>;

/// Helper to create environment errors
pub fn env_error(var: &str) -> Error {
    Error(Box::new(ErrorImpl::Environment(format!(
        "Missing environment variable: {var}"
    ))))
}

/// Helper to create configuration errors
#[allow(dead_code)]
pub fn config_error(message: &str) -> Error {
    Error(Box::new(ErrorImpl::Config(message.to_string())))
}

/// Helper to create component errors
#[allow(dead_code)]
pub fn component_error(message: &str) -> Error {
    Error(Box::new(ErrorImpl::Component(message.to_string())))
}

/// Helper to create Google Calendar errors
pub fn google_calendar_error(message: &str) -> Error {
    Error(Box::new(ErrorImpl::GoogleCalendar(message.to_string())))
}

/// Helper to create Work Schedule errors
pub fn work_schedule_error(message: &str) -> Error {
    Error(Box::new(ErrorImpl::WorkSchedule(message.to_string())))
}

/// Helper to create other errors
#[allow(dead_code)]
pub fn other_error(message: &str) -> Error {
    Error(Box::new(ErrorImpl::Other(message.to_string())))
}
