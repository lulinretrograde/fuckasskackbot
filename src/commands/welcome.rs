use poise::serenity_prelude as serenity;
use serenity::CreateEmbed;

use crate::commands::moderation::{err, ok};
use crate::{Context, Error};

/// Willkommenskanal setzen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "welcome-channel"
)]
pub async fn welcome_channel(
    ctx: Context<'_>,
    #[description = "Kanal für Willkommensnachrichten"] channel: Option<serenity::GuildChannel>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    // No args → show current setting
    if channel.is_none() {
        let configs = ctx.data().log_configs.lock().await;
        let current = configs.get(&guild_id).and_then(|c| c.welcome);
        drop(configs);

        let value = match current {
            Some(id) => format!("<#{}>", id),
            None => "Nicht konfiguriert".to_string(),
        };

        ctx.send(
            poise::CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Willkommenskanal")
                        .description(format!("Aktueller Kanal: {}", value))
                        .color(0x5865F2u32),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    let ch = channel.as_ref().unwrap();

    if ch.kind != serenity::ChannelType::Text {
        ctx.send(
            poise::CreateReply::default()
                .embed(err(
                    "Ungültiger Kanal",
                    "Bitte wähle einen Textkanal.",
                ))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let saved_config = {
        let mut configs = ctx.data().log_configs.lock().await;
        let config = configs.entry(guild_id).or_default();
        config.welcome = Some(ch.id);
        config.clone()
    };

    crate::db::save_log_config(&ctx.data().db, guild_id, &saved_config).await;

    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Willkommenskanal gesetzt",
                &format!("Willkommensnachrichten werden in <#{}> gesendet.", ch.id),
            ))
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
