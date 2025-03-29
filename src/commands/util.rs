use crate::commands::{create_success_embed, CommandResult, Context};
use rust_i18n::t;

/// Simple ping command to check if the bot is responsive
#[poise::command(slash_command, prefix_command)]
pub async fn ping(ctx: Context<'_>) -> CommandResult {
    ctx.send(poise::CreateReply::default().embed(create_success_embed(
        &t!("ping_command"),
        &t!("ping_response"),
    )))
    .await?;
    Ok(())
}
