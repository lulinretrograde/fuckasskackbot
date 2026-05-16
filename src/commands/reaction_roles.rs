use poise::serenity_prelude as serenity;
use serenity::CreateEmbed;

use crate::{Context, Error};

fn emoji_str(emoji: &serenity::ReactionType) -> String {
    match emoji {
        serenity::ReactionType::Unicode(s) => s.clone(),
        serenity::ReactionType::Custom { id, name, .. } => {
            format!("<:{}:{}>", name.as_deref().unwrap_or("e"), id)
        }
        _ => emoji.to_string(),
    }
}

// ── /reaktionsrolle ───────────────────────────────────────────────────────────

/// Reaktionsrollen verwalten
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_ROLES",
    guild_only,
    subcommands("hinzufuegen", "entfernen", "liste"),
    rename = "reaktionsrolle"
)]
pub async fn reaktionsrolle(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Reaktionsrolle zu einer Nachricht hinzufügen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_ROLES",
    guild_only,
    rename = "hinzufügen"
)]
pub async fn hinzufuegen(
    ctx: Context<'_>,
    #[description = "Kanal mit der Ziel-Nachricht"] kanal: serenity::GuildChannel,
    #[description = "Nachrichten-ID"] nachricht_id: String,
    #[description = "Emoji (Unicode oder :name:)"] emoji: String,
    #[description = "Rolle, die vergeben werden soll"] rolle: serenity::Role,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    let message_id: u64 = match nachricht_id.trim().parse() {
        Ok(id) => id,
        Err(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(crate::commands::moderation::err("Ungültige ID", "Das sieht nicht wie eine gültige Nachrichten-ID aus."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let msg_id = serenity::MessageId::new(message_id);

    // Verify message exists
    if kanal.id.message(ctx.http(), msg_id).await.is_err() {
        ctx.send(
            poise::CreateReply::default()
                .embed(crate::commands::moderation::err("Nicht gefunden", "Nachricht nicht gefunden. Prüfe Kanal und ID."))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let emoji_clean = emoji.trim().to_string();
    let added = crate::db::add_reaction_role(
        &ctx.data().db,
        guild_id,
        kanal.id,
        msg_id,
        &emoji_clean,
        rolle.id,
    ).await;

    if added {
        // Add the reaction to the message so users can see it
        let reaction = parse_reaction_type(&emoji_clean);
        let _ = kanal.id.create_reaction(ctx.http(), msg_id, reaction).await;

        ctx.send(
            poise::CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("✅ Reaktionsrolle hinzugefügt")
                        .color(0x57F287u32)
                        .field("Emoji",   &emoji_clean,              true)
                        .field("Rolle",   format!("<@&{}>", rolle.id), true)
                        .field("Kanal",   format!("<#{}>", kanal.id),  true),
                )
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            poise::CreateReply::default()
                .embed(crate::commands::moderation::err(
                    "Bereits vorhanden",
                    "Für dieses Emoji auf dieser Nachricht ist bereits eine Reaktionsrolle eingerichtet.",
                ))
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}

/// Reaktionsrolle entfernen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_ROLES",
    guild_only,
    rename = "entfernen"
)]
pub async fn entfernen(
    ctx: Context<'_>,
    #[description = "ID der Reaktionsrolle (aus /reaktionsrolle liste)"] id: i64,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let deleted  = crate::db::remove_reaction_role(&ctx.data().db, id, guild_id).await;

    if deleted {
        ctx.send(
            poise::CreateReply::default()
                .embed(crate::commands::moderation::ok("Entfernt", &format!("Reaktionsrolle `{}` wurde entfernt.", id)))
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            poise::CreateReply::default()
                .embed(crate::commands::moderation::err("Nicht gefunden", "Reaktionsrolle nicht gefunden."))
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}

/// Alle Reaktionsrollen auf diesem Server anzeigen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_ROLES",
    guild_only,
    rename = "liste"
)]
pub async fn liste(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let rows     = crate::db::get_reaction_roles_for_guild(&ctx.data().db, guild_id).await;

    if rows.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .embed(crate::commands::moderation::info("Keine Reaktionsrollen", "Noch keine Reaktionsrollen eingerichtet."))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let lines: Vec<String> = rows
        .iter()
        .map(|r| format!(
            "`[ID {}]` {} → <@&{}> in <#{}>",
            r.id, r.emoji, r.role_id, r.channel_id
        ))
        .collect();

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("🎭 Reaktionsrollen")
                    .description(lines.join("\n"))
                    .color(0x5865F2u32),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// Convert a string emoji (Unicode or custom) into a ReactionType.
pub fn parse_reaction_type(s: &str) -> serenity::ReactionType {
    // Custom emoji: <:name:id> or <a:name:id>
    if s.starts_with('<') && s.ends_with('>') {
        let inner = &s[1..s.len()-1];
        let animated = inner.starts_with('a');
        let parts: Vec<&str> = inner.trim_start_matches('a').trim_start_matches(':').splitn(2, ':').collect();
        if parts.len() == 2 {
            if let Ok(id) = parts[1].parse::<u64>() {
                return serenity::ReactionType::Custom {
                    animated,
                    id: serenity::EmojiId::new(id),
                    name: Some(parts[0].to_string()),
                };
            }
        }
    }
    serenity::ReactionType::Unicode(s.to_string())
}
