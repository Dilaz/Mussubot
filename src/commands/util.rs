use crate::commands::{CommandResult, Context};

/// Simple ping command to check if the bot is responsive
#[poise::command(slash_command, prefix_command)]
pub async fn ping(ctx: Context<'_>) -> CommandResult {
    ctx.say("Pong!").await?;
    Ok(())
}

/// Dummy command placeholder that can be filled in later
#[poise::command(slash_command, prefix_command)]
pub async fn dummy(
    ctx: Context<'_>,
    #[description = "Optional parameter"] param: Option<String>,
) -> CommandResult {
    let response = if let Some(value) = param {
        format!("Dummy command received: {}", value)
    } else {
        "Dummy command executed!".to_string()
    };
    
    ctx.say(response).await?;
    Ok(())
} 