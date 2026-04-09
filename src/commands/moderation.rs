use poise::serenity_prelude as serenity;
use serenity::{ChannelId, CreateEmbed, CreateEmbedAuthor, CreateMessage, RoleId, Timestamp, UserId};

use crate::{Context, Error};

// ── embed helpers (used by setup/welcome) ────────────────────────────────────

pub fn ok(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .description(format!(
            "<:approve:1478760793880137981> **{}**\n{}",
            title, description
        ))
        .color(0x57F287u32)
}

pub fn err(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .description(format!(
            "<:deny:1478760843738091531> **{}**\n{}",
            title, description
        ))
        .color(0xED4245u32)
}

pub fn info(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .description(format!(
            "<:warning:1478852498683985982> **{}**\n{}",
            title, description
        ))
        .color(0x5865F2u32)
}

// ── mod log ──────────────────────────────────────────────────────────────────

async fn send_mod_log(ctx: Context<'_>, embed: CreateEmbed) {
    let guild_id = match ctx.guild_id() {
        Some(g) => g,
        None => return,
    };
    let log_ch = {
        let configs = ctx.data().log_configs.lock().await;
        configs.get(&guild_id).and_then(|c| c.mod_log)
    };
    if let Some(ch) = log_ch {
        if let Err(e) = ch
            .send_message(ctx.http(), CreateMessage::new().embed(embed))
            .await
        {
            tracing::warn!("Mod-Log konnte nicht gesendet werden: {}", e);
        }
    }
}

fn mod_log_embed(
    action: &str,
    color: u32,
    moderator: &serenity::User,
    target: &serenity::User,
    extra_fields: Vec<(&'static str, String, bool)>,
) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(action)
        .color(color)
        .field("👤 Ziel", format!("<@{}> ({})", target.id, target.tag()), true)
        .field("🛡️ Moderator", format!("<@{}>", moderator.id), true)
        .timestamp(Timestamp::now());
    for (name, value, inline) in extra_fields {
        embed = embed.field(name, value, inline);
    }
    embed
}

fn mod_log_embed_no_target(
    action: &str,
    color: u32,
    moderator: &serenity::User,
    channel_id: ChannelId,
    extra_fields: Vec<(&'static str, String, bool)>,
) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(action)
        .color(color)
        .field("🛡️ Moderator", format!("<@{}>", moderator.id), true)
        .field("📢 Kanal", format!("<#{}>", channel_id), true)
        .timestamp(Timestamp::now());
    for (name, value, inline) in extra_fields {
        embed = embed.field(name, value, inline);
    }
    embed
}

// ── shared embed builder for mod actions ─────────────────────────────────────

fn mod_embed(
    user: &serenity::User,
    guild_icon: &str,
    fields: Vec<(&str, String, bool)>,
) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .author(CreateEmbedAuthor::new(user.tag()).icon_url(user.face()))
        .thumbnail(guild_icon);
    for (name, value, inline) in fields {
        embed = embed.field(name, value, inline);
    }
    embed
}

fn guild_icon(ctx: Context<'_>) -> String {
    ctx.guild()
        .and_then(|g| g.icon_url())
        .unwrap_or_default()
}

// ── /ban ─────────────────────────────────────────────────────────────────────

/// Einen Nutzer dauerhaft vom Server bannen
#[poise::command(
    slash_command,
    required_permissions = "BAN_MEMBERS",
    guild_only
)]
pub async fn ban(
    ctx: Context<'_>,
    #[description = "Der Nutzer, der gebannt werden soll"] user: serenity::User,
    #[description = "Grund für den Bann"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let reason = reason.as_deref().unwrap_or("Kein Grund angegeben");
    let icon = guild_icon(ctx);

    match guild_id.ban_with_reason(ctx.http(), user.id, 7, reason).await {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde gebannt.", user.id))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", format!("> {}", reason), true),
                        ("**Dauer:**", "> Dauerhaft".to_string(), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("🔨 Bann", 0xED4245, ctx.author(), &user, vec![
                ("Grund", reason.to_string(), false),
                ("Dauer", "Dauerhaft".to_string(), true),
            ])).await;
        }
        Err(e) if e.to_string().contains("403") => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehlende Berechtigungen", "Ich habe keine Berechtigung, diesen Nutzer zu bannen."))
                    .ephemeral(true),
            )
            .await?;
        }
        Err(e) => {
            tracing::error!("Bann fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Bann fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /unban ────────────────────────────────────────────────────────────────────

/// Einen gebannten Nutzer anhand seiner ID entbannen
#[poise::command(
    slash_command,
    required_permissions = "BAN_MEMBERS",
    guild_only
)]
pub async fn unban(
    ctx: Context<'_>,
    #[description = "Die Nutzer-ID, die entbannt werden soll"] user_id: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let icon = guild_icon(ctx);

    let uid: u64 = match user_id.trim().parse() {
        Ok(id) => id,
        Err(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Ungültige ID", "Das sieht nicht wie eine gültige Nutzer-ID aus."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let target_id = UserId::new(uid);

    // Fetch user info for the embed
    let user = match ctx.http().get_user(target_id).await {
        Ok(u) => u,
        Err(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Nutzer nicht gefunden", "Dieser Nutzer existiert nicht."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    match guild_id.unban(ctx.http(), target_id).await {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde entbannt.", uid))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", "> Kein Grund angegeben".to_string(), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("🔓 Entbann", 0x57F287, ctx.author(), &user, vec![])).await;
        }
        Err(e) => {
            tracing::error!("Entbann fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Entbannen fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /kick ─────────────────────────────────────────────────────────────────────

/// Einen Nutzer vom Server kicken
#[poise::command(
    slash_command,
    required_permissions = "KICK_MEMBERS",
    guild_only
)]
pub async fn kick(
    ctx: Context<'_>,
    #[description = "Der Nutzer, der gekickt werden soll"] user: serenity::User,
    #[description = "Grund für den Kick"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let reason = reason.as_deref().unwrap_or("Kein Grund angegeben");
    let icon = guild_icon(ctx);

    match guild_id.kick_with_reason(ctx.http(), user.id, reason).await {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde gekickt.", user.id))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", format!("> {}", reason), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("👢 Kick", 0xFEE75C, ctx.author(), &user, vec![
                ("Grund", reason.to_string(), false),
            ])).await;
        }
        Err(e) if e.to_string().contains("403") => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehlende Berechtigungen", "Ich habe keine Berechtigung, diesen Nutzer zu kicken."))
                    .ephemeral(true),
            )
            .await?;
        }
        Err(e) => {
            tracing::error!("Kick fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Kick fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /mute (timeout) ──────────────────────────────────────────────────────────

/// Einen Nutzer für eine bestimmte Zeit stummschalten (Timeout)
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn mute(
    ctx: Context<'_>,
    #[description = "Der Nutzer, der stummgeschaltet werden soll"] user: serenity::User,
    #[description = "Dauer in Minuten (max. 40320 = 28 Tage)"]
    #[min = 1]
    #[max = 40320]
    minuten: u32,
    #[description = "Grund für die Stummschaltung"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let reason = reason.as_deref().unwrap_or("Kein Grund angegeben");
    let icon = guild_icon(ctx);

    let until = (chrono::Utc::now() + chrono::Duration::minutes(i64::from(minuten))).to_rfc3339();

    let (display, einheit) = if minuten >= 60 {
        (minuten / 60, if minuten / 60 == 1 { "Stunde" } else { "Stunden" })
    } else {
        (minuten, if minuten == 1 { "Minute" } else { "Minuten" })
    };
    let duration_text = format!("{} {}", display, einheit);

    match guild_id
        .edit_member(ctx.http(), user.id, serenity::EditMember::new().disable_communication_until(until).audit_log_reason(reason))
        .await
    {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde stummgeschaltet.", user.id))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", format!("> {}", reason), true),
                        ("**Dauer:**", format!("> {}", duration_text), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("🔇 Stummschaltung", 0xFEE75C, ctx.author(), &user, vec![
                ("Grund", reason.to_string(), true),
                ("Dauer", duration_text.clone(), true),
            ])).await;
        }
        Err(e) if e.to_string().contains("403") => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehlende Berechtigungen", "Ich habe keine Berechtigung, diesen Nutzer stummzuschalten."))
                    .ephemeral(true),
            )
            .await?;
        }
        Err(e) => {
            tracing::error!("Mute fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Stummschalten fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /unmute ───────────────────────────────────────────────────────────────────

/// Timeout eines Nutzers vorzeitig aufheben
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn unmute(
    ctx: Context<'_>,
    #[description = "Der Nutzer, dessen Stummschaltung aufgehoben werden soll"] user: serenity::User,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let icon = guild_icon(ctx);

    match guild_id
        .edit_member(ctx.http(), user.id, serenity::EditMember::new().enable_communication())
        .await
    {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde entstummt.", user.id))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", "> Kein Grund angegeben".to_string(), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("🔊 Entstummung", 0x57F287, ctx.author(), &user, vec![])).await;
        }
        Err(e) => {
            tracing::error!("Unmute fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Stummschaltung aufheben fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /warn ─────────────────────────────────────────────────────────────────────

/// Einem Nutzer eine Verwarnung ausstellen
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn warn(
    ctx: Context<'_>,
    #[description = "Der Nutzer, der verwarnt werden soll"] user: serenity::User,
    #[description = "Grund für die Verwarnung"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let reason = reason.unwrap_or_else(|| "Kein Grund angegeben".to_string());
    let guild_id = ctx.guild_id().unwrap();
    let icon = guild_icon(ctx);

    crate::db::add_warning(&ctx.data().db, guild_id, user.id, ctx.author().id, &reason).await;
    let count = crate::db::get_warnings(&ctx.data().db, guild_id, user.id).await.len();

    // DM the warned user
    if let Ok(dm) = user.create_dm_channel(ctx.http()).await {
        let _ = dm
            .send_message(
                ctx.http(),
                CreateMessage::new().embed(
                    mod_embed(&user, &icon, vec![
                        ("**Grund:**", format!("> {}", reason), true),
                        ("**Verwarnungen gesamt:**", format!("> {}", count), true),
                    ])
                ),
            )
            .await;
    }

    ctx.send(
        poise::CreateReply::default()
            .content(format!("<@{}> wurde verwarnt.", user.id))
            .embed(mod_embed(&user, &icon, vec![
                ("**Grund:**", format!("> {}", reason), true),
                ("**Verwarnungen gesamt:**", format!("> {}", count), true),
            ])),
    )
    .await?;
    send_mod_log(ctx, mod_log_embed("⚠️ Verwarnung", 0xFEE75C, ctx.author(), &user, vec![
        ("Grund", reason.clone(), true),
        ("Verwarnungen gesamt", count.to_string(), true),
    ])).await;

    Ok(())
}

// ── /warnings ────────────────────────────────────────────────────────────────

/// Verwarnungen eines Nutzers anzeigen
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn warnings(
    ctx: Context<'_>,
    #[description = "Der Nutzer, dessen Verwarnungen angezeigt werden sollen"] user: serenity::User,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let list = crate::db::get_warnings(&ctx.data().db, guild_id, user.id).await;

    if list.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .embed(info("Keine Verwarnungen", &format!("**{}** hat keine Verwarnungen.", user.tag())))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let lines: Vec<String> = list
        .iter()
        .enumerate()
        .map(|(i, (moderator, reason))| format!("**{}**. <@{}>: {}", i + 1, moderator, reason))
        .collect();

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title(format!("Verwarnungen von {}", user.tag()))
                    .description(lines.join("\n"))
                    .color(0xFEE75Cu32),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

// ── /clearwarnings ────────────────────────────────────────────────────────────

/// Alle Verwarnungen eines Nutzers löschen
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn clearwarnings(
    ctx: Context<'_>,
    #[description = "Der Nutzer, dessen Verwarnungen gelöscht werden sollen"] user: serenity::User,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let entfernt = crate::db::clear_warnings(&ctx.data().db, guild_id, user.id).await;

    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Verwarnungen gelöscht",
                &format!("{} Verwarnung(en) von **{}** wurden gelöscht.", entfernt, user.tag()),
            ))
            .ephemeral(true),
    )
    .await?;
    send_mod_log(ctx, mod_log_embed("🗑️ Verwarnungen gelöscht", 0x5865F2, ctx.author(), &user, vec![
        ("Anzahl", entfernt.to_string(), true),
    ])).await;

    Ok(())
}

// ── /purge ────────────────────────────────────────────────────────────────────

/// Nachrichten in diesem Kanal massenhaft löschen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_MESSAGES",
    guild_only
)]
pub async fn purge(
    ctx: Context<'_>,
    #[description = "Anzahl der zu löschenden Nachrichten (1–100)"]
    #[min = 1]
    #[max = 100]
    anzahl: u8,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let channel_id = ctx.channel_id();

    let messages = channel_id
        .messages(ctx.http(), serenity::GetMessages::new().limit(anzahl))
        .await?;

    let count = messages.len();
    let ids: Vec<serenity::MessageId> = messages.into_iter().map(|m| m.id).collect();

    match channel_id.delete_messages(ctx.http(), &ids).await {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(ok("Bereinigt", &format!("**{}** Nachricht(en) wurden gelöscht.", count)))
                    .ephemeral(true),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed_no_target("🧹 Bereinigung", 0x5865F2, ctx.author(), channel_id, vec![
                ("Gelöscht", count.to_string(), true),
            ])).await;
        }
        Err(e) => {
            tracing::error!("Bereinigung fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Bereinigung fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /jail ─────────────────────────────────────────────────────────────────────

/// Einen Nutzer in den Knast sperren (entfernt alle Rollen, gibt Jail-Rolle)
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn jail(
    ctx: Context<'_>,
    #[description = "Der Nutzer, der gesperrt werden soll"] user: serenity::User,
    #[description = "Grund für die Sperrung"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let reason = reason.as_deref().unwrap_or("Kein Grund angegeben");
    let icon = guild_icon(ctx);

    let (jail_role, jail_channel) = {
        let configs = ctx.data().log_configs.lock().await;
        let c = configs.get(&guild_id);
        (c.and_then(|c| c.jail_role), c.and_then(|c| c.jail_channel))
    };

    let jail_role = match jail_role {
        Some(r) => r,
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Nicht konfiguriert", "Kein Jail-System eingerichtet. Nutze `/setup-jail`."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let member = match guild_id.member(ctx.http(), user.id).await {
        Ok(m) => m,
        Err(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Nicht gefunden", "Dieses Mitglied ist nicht auf dem Server."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let original_roles: Vec<RoleId> = member
        .roles
        .iter()
        .filter(|&&r| r.get() != guild_id.get())
        .cloned()
        .collect();

    crate::db::jail_user(&ctx.data().db, guild_id, user.id, &original_roles).await;

    match guild_id
        .edit_member(ctx.http(), user.id, serenity::EditMember::new().roles(vec![jail_role]))
        .await
    {
        Ok(_) => {
            let jail_ch_mention = jail_channel
                .map(|id| format!(" Begib dich in <#{}>.", id))
                .unwrap_or_default();

            if let Ok(dm) = user.create_dm_channel(ctx.http()).await {
                let _ = dm
                    .send_message(
                        ctx.http(),
                        CreateMessage::new().embed(
                            mod_embed(&user, &icon, vec![
                                ("**Grund:**", format!("> {}", reason), true),
                            ])
                            .description(format!(
                                "Du wurdest gesperrt.{}",
                                jail_ch_mention
                            )),
                        ),
                    )
                    .await;
            }

            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde in den Knast gesperrt.", user.id))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", format!("> {}", reason), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("🔒 Eingesperrt", 0xED4245, ctx.author(), &user, vec![
                ("Grund", reason.to_string(), false),
            ])).await;
        }
        Err(e) if e.to_string().contains("403") => {
            crate::db::unjail_user(&ctx.data().db, guild_id, user.id).await;
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehlende Berechtigungen", "Ich habe keine Berechtigung, die Rollen dieses Nutzers zu ändern."))
                    .ephemeral(true),
            )
            .await?;
        }
        Err(e) => {
            crate::db::unjail_user(&ctx.data().db, guild_id, user.id).await;
            tracing::error!("Jail fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Jail fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

// ── /unjail ───────────────────────────────────────────────────────────────────

/// Einen gesperrten Nutzer freilassen und seine Rollen wiederherstellen
#[poise::command(
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    guild_only
)]
pub async fn unjail(
    ctx: Context<'_>,
    #[description = "Der Nutzer, der freigelassen werden soll"] user: serenity::User,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let icon = guild_icon(ctx);

    let jail_role = {
        let configs = ctx.data().log_configs.lock().await;
        configs.get(&guild_id).and_then(|c| c.jail_role)
    };

    let jail_role = match jail_role {
        Some(r) => r,
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Nicht konfiguriert", "Kein Jail-System eingerichtet. Nutze `/setup-jail`."))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let mut restore_roles = crate::db::unjail_user(&ctx.data().db, guild_id, user.id).await;
    restore_roles.retain(|&r| r != jail_role);

    match guild_id
        .edit_member(ctx.http(), user.id, serenity::EditMember::new().roles(restore_roles))
        .await
    {
        Ok(_) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}> wurde freigelassen.", user.id))
                    .embed(mod_embed(&user, &icon, vec![
                        ("**Grund:**", "> Kein Grund angegeben".to_string(), true),
                    ])),
            )
            .await?;
            send_mod_log(ctx, mod_log_embed("🔓 Freigelassen", 0x57F287, ctx.author(), &user, vec![])).await;
        }
        Err(e) => {
            tracing::error!("Unjail fehlgeschlagen: {e}");
            ctx.send(
                poise::CreateReply::default()
                    .embed(err("Fehler", &format!("Freilassen fehlgeschlagen: {e}")))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}
