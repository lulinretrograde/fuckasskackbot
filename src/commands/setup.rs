use poise::serenity_prelude as serenity;
use serenity::CreateEmbed;

use crate::commands::moderation::err;
use crate::config::LogConfig;
use crate::{Context, Error};

/// Basisrolle setzen, die jedes Mitglied automatisch erhält. Ohne Angabe: aktuelle Einstellung.
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "baserole"
)]
pub async fn baserole(
    ctx: Context<'_>,
    #[description = "Rolle, die jedes Mitglied (außer gesperrten) haben muss"] role: Option<serenity::Role>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    if role.is_none() {
        let configs = ctx.data().log_configs.lock().await;
        let c = configs.get(&guild_id).cloned().unwrap_or_default();
        drop(configs);

        let fmt = match c.base_role {
            Some(id) => format!("<@&{}>", id),
            None => "Nicht konfiguriert".to_string(),
        };

        ctx.send(
            poise::CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Basisrollen-Konfiguration")
                        .color(0x5865F2u32)
                        .field("Aktuelle Basisrolle", fmt, false)
                        .footer(serenity::CreateEmbedFooter::new(
                            "Tipp: Nutze /baserole mit einer Rolle zum Setzen",
                        )),
                )
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let role = role.unwrap();

    // Save new config
    {
        let mut configs = ctx.data().log_configs.lock().await;
        let config = configs.entry(guild_id).or_insert_with(LogConfig::default);
        config.base_role = Some(role.id);
        let saved = config.clone();
        drop(configs);
        crate::db::save_log_config(&ctx.data().db, guild_id, &saved).await;
    }

    // Scan all current members and assign the role where missing (skip bots and mod-jailed users)
    let jailed: std::collections::HashSet<serenity::UserId> =
        crate::db::get_jailed_user_ids(&ctx.data().db, guild_id)
            .await
            .into_iter()
            .collect();

    let needs_role: Option<Vec<serenity::UserId>> = {
        ctx.guild().map(|guild| {
            guild
                .members
                .iter()
                .filter(|(id, m)| {
                    !m.user.bot && !jailed.contains(*id) && !m.roles.contains(&role.id)
                })
                .map(|(id, _)| *id)
                .collect()
        })
    };

    let needs_role = match needs_role {
        Some(v) => v,
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", "Guild nicht im Cache. Bitte erneut versuchen."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let total = needs_role.len();
    let mut assigned = 0u32;
    for user_id in &needs_role {
        if ctx
            .http()
            .add_member_role(guild_id, *user_id, role.id, Some("Basisrolle vergeben"))
            .await
            .is_ok()
        {
            assigned += 1;
        }
    }

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("Basisrolle gesetzt")
                    .color(0x57F287u32)
                    .field("Basisrolle", format!("<@&{}>", role.id), false)
                    .field(
                        "Sofortiger Sync",
                        format!("{}/{} Mitglieder erhalten die Rolle", assigned, total),
                        false,
                    )
                    .footer(serenity::CreateEmbedFooter::new(
                        "Neue Mitglieder erhalten die Rolle automatisch beim Beitritt",
                    )),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// Jail-System konfigurieren. Ohne Angaben wird die aktuelle Konfiguration angezeigt.
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "setup-jail"
)]
pub async fn setup_jail(
    ctx: Context<'_>,
    #[description = "Jail-Rolle, die gesperrten Nutzern zugewiesen wird"] jail_role: Option<serenity::Role>,
    #[description = "Kanal, in dem der gesperrte Nutzer sitzen soll"] jail_channel: Option<serenity::GuildChannel>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    if jail_role.is_none() && jail_channel.is_none() {
        let configs = ctx.data().log_configs.lock().await;
        let c = configs.get(&guild_id).cloned().unwrap_or_default();
        drop(configs);

        let fmt_role = match c.jail_role {
            Some(id) => format!("<@&{}>", id),
            None => "Nicht konfiguriert".to_string(),
        };
        let fmt_ch = match c.jail_channel {
            Some(id) => format!("<#{}>", id),
            None => "Nicht konfiguriert".to_string(),
        };

        ctx.send(
            poise::CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Jail-Konfiguration")
                        .color(0x5865F2u32)
                        .field("🔒 Jail-Rolle", fmt_role, false)
                        .field("📢 Jail-Kanal", fmt_ch, false)
                        .footer(serenity::CreateEmbedFooter::new(
                            "Tipp: Nutze /setup-jail mit Angaben zum Ändern",
                        )),
                )
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    if let Some(ch) = &jail_channel {
        if ch.kind != serenity::ChannelType::Text {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Ungültiger Kanal", "Bitte wähle einen Textkanal."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    }

    let mut configs = ctx.data().log_configs.lock().await;
    let config = configs.entry(guild_id).or_insert_with(LogConfig::default);
    let mut updated: Vec<(&str, String)> = Vec::new();

    if let Some(r) = &jail_role {
        config.jail_role = Some(r.id);
        updated.push(("🔒 Jail-Rolle", format!("<@&{}>", r.id)));
    }
    if let Some(ch) = &jail_channel {
        config.jail_channel = Some(ch.id);
        updated.push(("📢 Jail-Kanal", format!("<#{}>", ch.id)));
    }
    let saved_config = config.clone();
    drop(configs);

    crate::db::save_log_config(&ctx.data().db, guild_id, &saved_config).await;

    let mut embed = CreateEmbed::new()
        .title("Jail-System konfiguriert")
        .description("<:approve:1478760793880137981> **Folgende Einstellungen wurden gespeichert:**")
        .color(0x57F287u32);

    for (name, value) in updated {
        embed = embed.field(name, value, false);
    }

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// Log-Kanäle konfigurieren. Ohne Angaben wird die aktuelle Konfiguration angezeigt.
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "setup-logs"
)]
pub async fn setup_logs(
    ctx: Context<'_>,
    #[description = "Kanal für Voice-Logs (Beitritt, Verlassen, Wechsel, Stummschaltung)"]
    voice_logs: Option<serenity::GuildChannel>,
    #[description = "Kanal für Nachrichten-Logs (Bearbeitet, Gelöscht, Massenlöschung)"]
    message_logs: Option<serenity::GuildChannel>,
    #[description = "Kanal für Beitritts- und Abgangs-Logs (inkl. Sicherheitshinweise)"]
    join_leave_logs: Option<serenity::GuildChannel>,
    #[description = "Kanal für Server-Logs (Rollen, Kanäle, Server-Einstellungen, Bans)"]
    server_logs: Option<serenity::GuildChannel>,
    #[description = "Kanal für Mitglieder-Logs (Nickname, Rollen, Avatar)"]
    member_logs: Option<serenity::GuildChannel>,
    #[description = "Kanal für Moderations-Logs (Ban, Kick, Mute, Warn, Purge, Jail …)"]
    mod_log: Option<serenity::GuildChannel>,
    #[description = "Kanal für Bot-Logs (Level-Ups, Willkommensnachrichten, Raid-Erkennung)"]
    bot_log: Option<serenity::GuildChannel>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    // No args → show current config
    if voice_logs.is_none()
        && message_logs.is_none()
        && join_leave_logs.is_none()
        && server_logs.is_none()
        && member_logs.is_none()
        && mod_log.is_none()
        && bot_log.is_none()
    {
        let configs = ctx.data().log_configs.lock().await;
        let c = configs.get(&guild_id).cloned().unwrap_or_default();
        drop(configs);

        let fmt = |ch: Option<serenity::ChannelId>| match ch {
            Some(id) => format!("<#{}>", id),
            None => "Nicht konfiguriert".to_string(),
        };

        ctx.send(
            poise::CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Log-Konfiguration")
                        .color(0x5865F2u32)
                        .field("🔊 Voice-Logs", fmt(c.voice), false)
                        .field("💬 Nachrichten-Logs", fmt(c.messages), false)
                        .field("📥 Beitritts-/Abgangs-Logs", fmt(c.join_leave), false)
                        .field("🛡️ Server-Logs", fmt(c.server), false)
                        .field("👤 Mitglieder-Logs", fmt(c.members), false)
                        .field("⚖️ Mod-Log", fmt(c.mod_log), false)
                        .field("🤖 Bot-Log", fmt(c.bot_log), false)
                        .footer(serenity::CreateEmbedFooter::new(
                            "Tipp: Nutze /setup-logs mit Kanalangaben zum Ändern",
                        )),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    // Validate that all provided channels are text channels
    let all_channels = [
        voice_logs.as_ref(),
        message_logs.as_ref(),
        join_leave_logs.as_ref(),
        server_logs.as_ref(),
        member_logs.as_ref(),
        mod_log.as_ref(),
        bot_log.as_ref(),
    ];

    for ch in all_channels.iter().flatten() {
        if ch.kind != serenity::ChannelType::Text {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err(
                        "Ungültiger Kanal",
                        &format!(
                            "**#{}** ist kein Textkanal. Bitte wähle einen Textkanal.",
                            ch.name
                        ),
                    ))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    }

    // Update config
    let mut configs = ctx.data().log_configs.lock().await;
    let config = configs.entry(guild_id).or_insert_with(LogConfig::default);
    let mut updated: Vec<(&str, String)> = Vec::new();

    if let Some(ch) = &voice_logs {
        config.voice = Some(ch.id);
        updated.push(("🔊 Voice-Logs", format!("<#{}>", ch.id)));
    }
    if let Some(ch) = &message_logs {
        config.messages = Some(ch.id);
        updated.push(("💬 Nachrichten-Logs", format!("<#{}>", ch.id)));
    }
    if let Some(ch) = &join_leave_logs {
        config.join_leave = Some(ch.id);
        updated.push(("📥 Beitritts-/Abgangs-Logs", format!("<#{}>", ch.id)));
    }
    if let Some(ch) = &server_logs {
        config.server = Some(ch.id);
        updated.push(("🛡️ Server-Logs", format!("<#{}>", ch.id)));
    }
    if let Some(ch) = &member_logs {
        config.members = Some(ch.id);
        updated.push(("👤 Mitglieder-Logs", format!("<#{}>", ch.id)));
    }
    if let Some(ch) = &mod_log {
        config.mod_log = Some(ch.id);
        updated.push(("⚖️ Mod-Log", format!("<#{}>", ch.id)));
    }
    if let Some(ch) = &bot_log {
        config.bot_log = Some(ch.id);
        updated.push(("🤖 Bot-Log", format!("<#{}>", ch.id)));
    }
    let saved_config = config.clone();
    drop(configs);

    crate::db::save_log_config(&ctx.data().db, guild_id, &saved_config).await;

    let mut embed = CreateEmbed::new()
        .title("Log-Kanäle gesetzt")
        .description(
            "<:approve:1478760793880137981> **Folgende Log-Kanäle wurden konfiguriert:**\n\
             ⚠️ Stelle sicher, dass der `MESSAGE_CONTENT`-Intent im Developer Portal aktiviert ist, \
             damit Nachrichteninhalte in Lösch-Logs erscheinen.",
        )
        .color(0x57F287u32);

    for (name, value) in updated {
        embed = embed.field(name, value, false);
    }

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// Bot-Kanal festlegen. Alle nicht-moderativen Befehle sind dann nur dort erlaubt.
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "bot-channel"
)]
pub async fn bot_channel(
    ctx: Context<'_>,
    #[description = "Bot-Kanal (leer lassen um die Einschränkung aufzuheben)"]
    kanal: Option<serenity::GuildChannel>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().unwrap();
    let channel_id = kanal.as_ref().map(|c| c.id);
    crate::db::set_bot_channel(&ctx.data().db, guild_id, channel_id).await;

    let msg = match &kanal {
        Some(c) => format!("✅ Bot-Kanal auf <#{}> gesetzt. Alle nicht-moderativen Befehle sind jetzt nur dort verfügbar.", c.id),
        None    => "✅ Bot-Kanal-Einschränkung aufgehoben. Befehle sind wieder überall nutzbar.".to_string(),
    };
    ctx.send(poise::CreateReply::default().content(msg).ephemeral(true)).await?;
    Ok(())
}
