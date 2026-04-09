use poise::serenity_prelude::CreateEmbed;
use crate::{Context, Error};

/// Alle Befehle und Infos auf der Website
#[poise::command(slash_command, guild_only)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("Unit8200: Hilfe")
                    .description("Alle Befehle, Logs und Infos findest du auf der Website:\n\n**https://bot.pawjobs.net**")
                    .color(0x5865F2u32),
            )
            .ephemeral(true),
    )
    .await?;
    Ok(())
}
