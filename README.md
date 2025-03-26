# Mussubot

A modular Discord bot that integrates with Google Calendar to send notifications about upcoming events.

## Features

- 🔌 **Modular plugin system**: Components can be enabled or disabled as needed
- 📅 **Google Calendar integration**: Sends notifications about calendar events
- 🌅 **Daily event summary**: Every morning, the bot sends a message with today's events
- 📊 **Weekly event summary**: Every Monday, the bot sends a summary of the upcoming week's events
- 🔔 **New event notifications**: When events are added to the calendar, the bot sends a notification
- 💾 **Configuration via environment variables**: All settings are loaded from `.env` file
- 🎮 **Custom status**: The bot displays a custom "playing" status that can be configured
- 🐳 **Docker support**: Run the bot with Redis using Docker Compose

## Getting Started

### Prerequisites

- Rust and Cargo (latest stable version)
- Discord bot token
- Google Calendar API credentials
- Redis (for Docker setup)

### Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/mussubot.git
   cd mussubot
   ```

2. Copy the `.env.example` file to `.env` and fill in your credentials:
   ```bash
   cp .env.example .env
   ```

3. Edit the `.env` file with your Discord bot token, Google Calendar API credentials, and other settings.

4. Build and run the bot:
   ```bash
   cargo run
   ```

### Docker Setup

You can also run the bot using Docker Compose:

1. Make sure you have Docker and Docker Compose installed
2. Set up your `.env` file with the appropriate configuration
3. Run the bot with:
   ```bash
   docker-compose up -d
   ```

This will start both the bot and Redis service as defined in the `docker-compose.yaml` file.

### Discord Bot Setup

1. Go to the [Discord Developer Portal](https://discord.com/developers/applications)
2. Create a new application and set up a bot
3. Enable the necessary intents (Message Content, Server Members, etc.)
4. Invite the bot to your server with appropriate permissions

### Google Calendar Setup

1. Create a project in the [Google Cloud Console](https://console.cloud.google.com/)
2. Enable the Google Calendar API
3. Create OAuth 2.0 credentials
4. Find your Google Calendar ID (it's in the calendar settings)

## Configuration

Configure the bot by editing the `.env` file:

```
# Discord Bot Token
DISCORD_TOKEN=your_discord_bot_token_here

# Google Calendar API
GOOGLE_CLIENT_ID=your_google_client_id_here
GOOGLE_CLIENT_SECRET=your_google_client_secret_here
GOOGLE_CALENDAR_ID=your_google_calendar_id_here

# Discord channel ID for calendar notifications
CALENDAR_CHANNEL_ID=1234567890123456789

# Discord guild ID (server)
GUILD_ID=1234567890123456789

# Timezone (default: UTC)
TIMEZONE=Europe/Helsinki

# Bot activity status (default: "Leikkii lankakerällä")
BOT_ACTIVITY=Leikkii lankakerällä

# Redis connection URL
# Use redis:6379 when running with docker-compose
# Use 127.0.0.1:6379 when running locally
REDIS_URL=redis://redis:6379
```

## Logging

The bot uses the `tracing` crate for logging. You can control the log level by setting the `RUST_LOG` environment variable:

```bash
# Set general log level to debug, but keep serenity and poise at info level
RUST_LOG=debug,serenity=info,poise=info cargo run
```

## Available Commands

- `/ping` - Check if the bot is responsive
- `/dummy [param]` - A dummy command that can be customized (placeholder for future implementations)
- `/this_week [timezone]` - Get a list of this week's calendar events with optional timezone parameter

## Docker Compose Configuration

The project includes a `docker-compose.yaml` file to set up the bot and its Redis dependency:

```yaml
services:
  redis:
    image: redis:alpine
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data
    restart: unless-stopped

volumes:
  redis-data:
```

## Adding New Components

To add a new component to the bot:

1. Create a new file in the `src/components/` directory
2. Implement the `Component` trait
3. Add the component to the `ComponentManager` in `main.rs`

Example:

```rust
// src/components/my_component.rs
use crate::components::Component;
use crate::config::Config;
use crate::error::BotResult;
use async_trait::async_trait;
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MyComponent;

impl MyComponent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Component for MyComponent {
    fn name(&self) -> &'static str {
        "my_component"
    }
    
    async fn init(&self, ctx: &serenity::Context, config: Arc<RwLock<Config>>) -> BotResult<()> {
        tracing::info!("Initializing my component");
        // Initialization logic here
        Ok(())
    }
    
    async fn shutdown(&self) -> BotResult<()> {
        tracing::info!("Shutting down my component");
        // Cleanup logic here
        Ok(())
    }
}
```

Then register it in `main.rs`:

```rust
// In main.rs
use crate::components::my_component::MyComponent;

// Inside the main function
component_manager.register(MyComponent::new());
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [Serenity](https://github.com/serenity-rs/serenity) - Discord API library for Rust
- [Poise](https://github.com/serenity-rs/poise) - Framework for Serenity 