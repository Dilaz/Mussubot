use poise::serenity_prelude as serenity;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::error::BotResult;

/// Trait for component schedulers that handle periodic notifications
pub trait Scheduler: Send + 'static {
    /// The type of handle used by this scheduler
    type Handle: Clone + Send + Sync + 'static;

    /// Start the scheduler with the necessary context
    fn start(
        ctx: Arc<serenity::Context>,
        config: Arc<RwLock<Config>>,
        handle: Self::Handle,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>>;

    /// Stop the scheduler gracefully
    fn stop(&self) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>>;
}
