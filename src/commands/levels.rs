use std::collections::HashMap;

use poise::serenity_prelude as serenity;
use serenity::{ChannelType, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, CreateMessage};

use crate::commands::moderation::{err, info};
use crate::xp::{level_from_xp, progress_bar, total_xp_for_level, xp_progress};
use crate::{Context, Error};

// ── /level ────────────────────────────────────────────────────────────────────

/// Deinen Rang und Level anzeigen
#[poise::command(slash_command, guild_only)]
pub async fn level(
    ctx: Context<'_>,
    #[description = "Nutzer (Standard: du selbst)"] user: Option<serenity::User>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let target = user.as_ref().unwrap_or_else(|| ctx.author());

    if target.bot {
        ctx.send(
            poise::CreateReply::default()
                .embed(err("Ungültig", "Bots haben keinen Rang.")),
        )
        .await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();

    let total_xp = crate::db::get_xp(&ctx.data().db, guild_id, target.id).await;
    let rank_pos = crate::db::get_xp_rank(&ctx.data().db, guild_id, target.id).await;

    let level = level_from_xp(total_xp);
    let (current, needed) = xp_progress(total_xp);
    let bar = progress_bar(current, needed);
    let guild_icon = ctx.guild().and_then(|g| g.icon_url()).unwrap_or_default();

    let color: u32 = match level {
        0..=4 => 0x99AAB5,
        5..=9 => 0x57F287,
        10..=19 => 0x5865F2,
        20..=29 => 0xFEE75C,
        30..=49 => 0xED4245,
        _ => 0xEB459E,
    };

    let embed = CreateEmbed::new()
        .author(CreateEmbedAuthor::new(target.tag()).icon_url(target.face()))
        .thumbnail(guild_icon)
        .color(color)
        .field("🏆 Rang", format!("> #{}", rank_pos), true)
        .field("⭐ Level", format!("> {}", level), true)
        .field("📊 Gesamt-XP", format!("> {} XP", total_xp), true)
        .field(
            "✨ Fortschritt",
            format!("`{}` {}/{} XP", bar, current, needed),
            false,
        )
        .footer(CreateEmbedFooter::new(format!(
            "Noch {} XP bis Level {}",
            needed - current,
            level + 1
        )));

    ctx.send(
        poise::CreateReply::default()
            .embed(embed),
    )
    .await?;

    Ok(())
}

// ── /leaderboard ──────────────────────────────────────────────────────────────

/// Bestenliste des Servers anzeigen
#[poise::command(slash_command, guild_only)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();

    let entries = crate::db::get_guild_leaderboard(&ctx.data().db, guild_id, 10).await;

    if entries.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .embed(info(
                    "Keine Daten",
                    "Noch keine XP auf diesem Server. Schreib etwas!",
                )),
        )
        .await?;
        return Ok(());
    }

    let medals = ["🥇", "🥈", "🥉"];
    let mut lines = Vec::new();
    for (i, (user_id, xp)) in entries.iter().enumerate() {
        let level = level_from_xp(*xp);
        let prefix = medals.get(i).copied().unwrap_or("🔹");
        lines.push(format!(
            "{} **#{}** <@{}>: Level {} · {} XP",
            prefix,
            i + 1,
            user_id,
            level,
            xp
        ));
    }

    let guild_icon = ctx.guild().and_then(|g| g.icon_url()).unwrap_or_default();
    let guild_name = ctx.guild().map(|g| g.name.clone()).unwrap_or_default();

    let embed = CreateEmbed::new()
        .title("🏆 Bestenliste")
        .description(lines.join("\n"))
        .color(0xFEE75Cu32)
        .thumbnail(guild_icon)
        .footer(CreateEmbedFooter::new(format!("Top 10 auf {}", guild_name)));

    ctx.send(
        poise::CreateReply::default()
            .embed(embed),
    )
    .await?;

    Ok(())
}

// ── /scan-xp ──────────────────────────────────────────────────────────────────

/// Alle alten Nachrichten scannen und XP nachträglich vergeben (nur einmal ausführen!)
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "scan-xp"
)]
pub async fn scan_xp(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    let channels: Vec<serenity::ChannelId> = ctx
        .guild()
        .unwrap()
        .channels
        .values()
        .filter(|c| c.kind == ChannelType::Text)
        .map(|c| c.id)
        .collect();

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .description(format!(
                        "⏳ Scanne {} Textkanäle... Das kann einige Minuten dauern.",
                        channels.len()
                    ))
                    .color(0x5865F2u32),
            )
            .ephemeral(true),
    )
    .await?;

    let mut total_messages = 0u64;
    let mut user_counts: HashMap<serenity::UserId, u64> = HashMap::new();

    for channel_id in &channels {
        let mut before: Option<serenity::MessageId> = None;

        loop {
            let mut builder = serenity::GetMessages::new().limit(100);
            if let Some(b) = before {
                builder = builder.before(b);
            }

            let batch = match channel_id.messages(ctx.http(), builder).await {
                Ok(m) if !m.is_empty() => m,
                _ => break,
            };

            before = batch.last().map(|m| m.id);
            let len = batch.len();

            for msg in batch {
                if !msg.author.bot {
                    *user_counts.entry(msg.author.id).or_default() += 1;
                    total_messages += 1;
                }
            }

            if len < 100 {
                break;
            }
        }
    }

    // Grant 20 XP per historical message via DB transaction
    let counts: Vec<(serenity::UserId, u64)> = user_counts.iter().map(|(&u, &c)| (u, c)).collect();
    crate::db::bulk_add_xp(&ctx.data().db, guild_id, &counts).await;

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .description(format!(
                        "<:approve:1478760793880137981> **Scan abgeschlossen!**\n\
                        **{}** Nachrichten von **{}** Nutzern verarbeitet.\n\
                        Jede Nachricht wurde mit **20 XP** gewertet.",
                        total_messages,
                        user_counts.len()
                    ))
                    .color(0x57F287u32),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

// ── /reset-xp ─────────────────────────────────────────────────────────────────

/// Alle XP eines Nutzers auf diesem Server zurücksetzen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "reset-xp"
)]
pub async fn reset_xp(
    ctx: Context<'_>,
    #[description = "Nutzer, dessen XP zurückgesetzt werden soll"] user: serenity::User,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    let removed = crate::db::reset_user_xp(&ctx.data().db, guild_id, user.id).await;

    let msg = if removed {
        format!("XP von **{}** wurde zurückgesetzt.", user.tag())
    } else {
        format!("**{}** hatte keine XP.", user.tag())
    };

    ctx.send(
        poise::CreateReply::default()
            .embed(CreateEmbed::new().description(format!(
                "<:approve:1478760793880137981> {}",
                msg
            )).color(0x57F287u32))
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

// ── level-up embed (used by event handler) ────────────────────────────────────

pub fn level_up_embed(user_id: serenity::UserId, level: u64) -> CreateMessage {
    let coin_reward = level * 100;
    CreateMessage::new().embed(
        CreateEmbed::new()
            .description(format!(
                "🎉 <@{}> hat **Level {}** erreicht! **+{} Coins** als Belohnung!",
                user_id, level, coin_reward
            ))
            .color(match level {
                0..=4  => 0x99AAB5u32,
                5..=9  => 0x57F287,
                10..=19 => 0x5865F2,
                20..=29 => 0xFEE75C,
                30..=49 => 0xED4245,
                _      => 0xEB459E,
            })
            .footer(CreateEmbedFooter::new(if level < 50 {
                format!("Nächstes Level: {} XP", total_xp_for_level(level + 1))
            } else {
                "Level 50 erreicht! Nutze /prestige um weiterzumachen.".to_string()
            })),
    )
}

// ── /level-coins-migrate ──────────────────────────────────────────────────────

/// Einmalig: Rückwirkend Level-Coins für alle bestehenden Level auszahlen
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    guild_only,
    rename = "level-coins-migrate"
)]
pub async fn level_coins_migrate(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let users    = crate::db::get_guild_xp_users(&ctx.data().db, guild_id).await;

    let mut total_credited: i64 = 0;
    let mut users_credited: usize = 0;

    for (user_id, total_xp) in users {
        let current_level   = crate::xp::level_from_xp(total_xp) as i64;
        let already_credited = crate::db::get_credited_level(&ctx.data().db, guild_id, user_id).await as i64;

        if already_credited >= current_level { continue; }

        // sum coins from (already_credited+1) to current_level
        let coins: i64 = ((already_credited + 1)..=current_level)
            .map(|l| l * 100)
            .sum();

        if coins > 0 {
            crate::db::add_coins(&ctx.data().db, guild_id, user_id, coins).await;
            crate::db::set_credited_level(&ctx.data().db, guild_id, user_id, current_level as u64).await;
            total_credited += coins;
            users_credited += 1;
        }
    }

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("✅ Level-Coins Migration abgeschlossen")
                    .description(format!(
                        "**{}** Nutzer wurden rückwirkend gutgeschrieben.\n\
                         Insgesamt **{} Coins** ausgezahlt.",
                        users_credited, total_credited
                    ))
                    .color(0x57F287u32),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
