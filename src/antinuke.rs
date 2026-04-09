use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serenity::{
    ChannelType, CreateEmbed, CreateMessage, EditMember, GuildId,
    PermissionOverwrite, PermissionOverwriteType, Permissions, Timestamp, UserId,
};

use crate::config::{ActionKind, LockdownState, LogConfigs};
use crate::db::AntiNukeConfig;
use crate::AppData;

// ── punishment helper ─────────────────────────────────────────────────────────

pub async fn punish(
    ctx: &serenity::Context,
    guild_id: GuildId,
    actor_id: UserId,
    punishment: &str,
    reason: &str,
) {
    match punishment {
        "ban" => {
            if let Err(e) = guild_id.ban_with_reason(&ctx.http, actor_id, 0, reason).await {
                tracing::warn!("AntiNuke ban fehlgeschlagen für {}: {}", actor_id, e);
            }
        }
        "kick" => {
            if let Err(e) = guild_id.kick_with_reason(&ctx.http, actor_id, reason).await {
                tracing::warn!("AntiNuke kick fehlgeschlagen für {}: {}", actor_id, e);
            }
        }
        "strip" => {
            if let Err(e) = guild_id
                .edit_member(&ctx.http, actor_id, EditMember::new().roles(std::iter::empty::<serenity::RoleId>()))
                .await
            {
                tracing::warn!("AntiNuke strip fehlgeschlagen für {}: {}", actor_id, e);
            }
        }
        other => {
            tracing::warn!("AntiNuke: Unbekannte Bestrafung '{}'", other);
        }
    }
}

// ── record_action ─────────────────────────────────────────────────────────────

pub async fn record_action(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    actor_id: UserId,
    kind: ActionKind,
) {
    let config = match crate::db::get_antinuke_config(&data.db, guild_id).await {
        Some(c) => c,
        None => return,
    };
    if config.enabled == 0 {
        return;
    }

    // Skip guild owner
    let owner_id = ctx.cache.guild(guild_id).map(|g| g.owner_id);
    if owner_id == Some(actor_id) {
        return;
    }

    // Skip whitelisted users
    let whitelist = crate::db::get_antinuke_whitelist(&data.db, guild_id).await;
    if whitelist.contains(&actor_id) {
        return;
    }

    let threshold = threshold_for_kind(&config, kind);
    let window = Duration::from_secs(config.window_secs as u64);

    // Update counter and get current count
    let count = {
        let mut counters = data.nuke_counters.lock().await;
        let key = (guild_id, actor_id, kind);
        let deque = counters.entry(key).or_default();
        let cutoff = Instant::now().checked_sub(window).unwrap_or_else(Instant::now);
        deque.retain(|&t| t > cutoff);
        deque.push_back(Instant::now());
        deque.len()
    };

    if count < threshold as usize {
        return;
    }

    // Clear all counters for this guild+user
    {
        let mut counters = data.nuke_counters.lock().await;
        counters.retain(|(g, u, _), _| !(*g == guild_id && *u == actor_id));
    }

    let reason = format!(
        "AntiNuke: {:?} – {} Aktionen in {}s (Schwelle: {})",
        kind, count, config.window_secs, threshold
    );

    punish(ctx, guild_id, actor_id, &config.punishment, &reason).await;

    // DM guild owner
    if let Some(oid) = owner_id {
        if let Ok(dm) = oid.create_dm_channel(&ctx.http).await {
            let embed = CreateEmbed::new()
                .title("🛡️ AntiNuke ausgelöst")
                .color(0xED4245u32)
                .field("Server", guild_id.to_string(), true)
                .field("Täter", format!("<@{}>", actor_id), true)
                .field("Aktion", format!("{:?}", kind), true)
                .field("Anzahl", count.to_string(), true)
                .field("Bestrafung", &config.punishment, true)
                .field("Grund", &reason, false)
                .timestamp(Timestamp::now());
            let _ = dm
                .send_message(&ctx.http, CreateMessage::new().embed(embed))
                .await;
        }
    }

    // Log to bot_log
    let bot_log_embed = CreateEmbed::new()
        .title("🛡️ AntiNuke ausgelöst")
        .color(0xED4245u32)
        .field("Täter", format!("<@{}>", actor_id), true)
        .field("Aktion", format!("{:?}", kind), true)
        .field("Anzahl", count.to_string(), true)
        .field("Bestrafung", &config.punishment, true)
        .field("Grund", &reason, false)
        .timestamp(Timestamp::now());
    crate::events::send_bot_log(ctx, data, guild_id, bot_log_embed).await;
}

fn threshold_for_kind(cfg: &AntiNukeConfig, kind: ActionKind) -> i64 {
    match kind {
        ActionKind::ChannelDelete => cfg.chan_del_max,
        ActionKind::ChannelCreate => cfg.chan_cre_max,
        ActionKind::RoleDelete    => cfg.role_del_max,
        ActionKind::RoleCreate    => cfg.role_cre_max,
        ActionKind::Ban           => cfg.ban_max,
        ActionKind::WebhookCreate => cfg.webhook_max,
    }
}

// ── record_join ───────────────────────────────────────────────────────────────

pub async fn record_join(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    member: &serenity::Member,
) {
    let config = match crate::db::get_antinuke_config(&data.db, guild_id).await {
        Some(c) => c,
        None => return,
    };
    if config.enabled == 0 {
        return;
    }

    // Minimum account age check
    if config.min_account_age_days > 0 {
        let created_ts = member.user.id.created_at().unix_timestamp();
        let age_days = (chrono::Utc::now().timestamp() - created_ts) / 86400;
        if age_days < config.min_account_age_days {
            if let Err(e) = guild_id
                .kick_with_reason(
                    &ctx.http,
                    member.user.id,
                    &format!(
                        "AntiNuke: Konto zu jung ({} Tage, Minimum: {} Tage)",
                        age_days, config.min_account_age_days
                    ),
                )
                .await
            {
                tracing::warn!(
                    "AntiNuke: Kick fehlgeschlagen für junges Konto {}: {}",
                    member.user.id,
                    e
                );
            }
            return;
        }
    }

    let raid_window = Duration::from_secs(config.raid_window as u64);
    let user_id = member.user.id;

    // Update raid counter and collect recent joiners
    let (count, recent_joiners) = {
        let mut counters = data.raid_counters.lock().await;
        let deque = counters.entry(guild_id).or_default();
        let cutoff = Instant::now()
            .checked_sub(raid_window)
            .unwrap_or_else(Instant::now);
        deque.retain(|(t, _)| *t > cutoff);
        deque.push_back((Instant::now(), user_id));
        let recent: Vec<UserId> = deque.iter().map(|(_, uid)| *uid).collect();
        (deque.len(), recent)
    };

    if count >= config.raid_joins as usize {
        // Check if already in lockdown
        let already = {
            let ls = data.lockdown_state.lock().await;
            ls.contains_key(&guild_id)
        };
        if !already {
            enter_lockdown(ctx, data, guild_id, recent_joiners, &config).await;
        }
    }
}

// ── enter_lockdown ────────────────────────────────────────────────────────────

pub async fn enter_lockdown(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    recent_joiners: Vec<UserId>,
    config: &AntiNukeConfig,
) {
    let unlock_duration = Duration::from_secs(config.lockdown_mins as u64 * 60);
    let unlock_time = Instant::now() + unlock_duration;

    {
        let mut ls = data.lockdown_state.lock().await;
        ls.insert(guild_id, unlock_time);
    }

    // Collect text/news channels from cache
    let channels: Vec<serenity::ChannelId> = ctx
        .cache
        .guild(guild_id)
        .map(|g| {
            g.channels
                .values()
                .filter(|c| {
                    matches!(c.kind, ChannelType::Text | ChannelType::News)
                })
                .map(|c| c.id)
                .collect()
        })
        .unwrap_or_default();

    let everyone_id = serenity::RoleId::new(guild_id.get());
    let overwrite = PermissionOverwrite {
        allow: Permissions::empty(),
        deny: Permissions::SEND_MESSAGES | Permissions::ADD_REACTIONS,
        kind: PermissionOverwriteType::Role(everyone_id),
    };

    for ch_id in &channels {
        if let Err(e) = ch_id.create_permission(&ctx.http, overwrite.clone()).await {
            tracing::warn!("AntiNuke lockdown: Kanalsperre fehlgeschlagen für {}: {}", ch_id, e);
        }
    }

    // Punish recent joiners
    for uid in &recent_joiners {
        punish(ctx, guild_id, *uid, &config.punishment, "AntiNuke: Raid erkannt").await;
    }

    // Alert embed
    let embed = CreateEmbed::new()
        .title("🔒 Server-Lockdown aktiviert")
        .color(0xED4245u32)
        .description(format!(
            "**Raid erkannt** – {} Beitritte in {} Sekunden.\nServer wird für **{} Minuten** gesperrt.",
            recent_joiners.len(),
            config.raid_window,
            config.lockdown_mins
        ))
        .field(
            "Betroffene Nutzer",
            recent_joiners
                .iter()
                .map(|u| format!("<@{}>", u))
                .collect::<Vec<_>>()
                .join(", "),
            false,
        )
        .timestamp(Timestamp::now());
    crate::events::send_bot_log(ctx, data, guild_id, embed).await;

    // Spawn background task to auto-unlock
    let ctx_bg = ctx.clone();
    let ls_bg = data.lockdown_state.clone();
    let lc_bg = data.log_configs.clone();
    tokio::spawn(async move {
        tokio::time::sleep(unlock_duration).await;
        exit_lockdown(ctx_bg, ls_bg, lc_bg, guild_id).await;
    });
}

// ── exit_lockdown ─────────────────────────────────────────────────────────────

pub async fn exit_lockdown(
    ctx: serenity::Context,
    lockdown_state: LockdownState,
    log_configs: LogConfigs,
    guild_id: GuildId,
) {
    {
        let mut ls = lockdown_state.lock().await;
        ls.remove(&guild_id);
    }

    let channels: Vec<serenity::ChannelId> = ctx
        .cache
        .guild(guild_id)
        .map(|g| {
            g.channels
                .values()
                .filter(|c| matches!(c.kind, ChannelType::Text | ChannelType::News))
                .map(|c| c.id)
                .collect()
        })
        .unwrap_or_default();

    let everyone_id = serenity::RoleId::new(guild_id.get());

    for ch_id in &channels {
        if let Err(e) = ch_id
            .delete_permission(&ctx.http, PermissionOverwriteType::Role(everyone_id))
            .await
        {
            tracing::warn!(
                "AntiNuke exit_lockdown: Freigabe fehlgeschlagen für {}: {}",
                ch_id,
                e
            );
        }
    }

    // Send ended embed to bot_log
    let log_ch = {
        let c = log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.bot_log)
    };
    if let Some(ch) = log_ch {
        let embed = CreateEmbed::new()
            .title("🔓 Server-Lockdown beendet")
            .color(0x57F287u32)
            .description("Der Lockdown wurde aufgehoben. Alle Kanäle sind wieder zugänglich.")
            .timestamp(Timestamp::now());
        if let Err(e) = ch
            .send_message(&ctx.http, CreateMessage::new().embed(embed))
            .await
        {
            tracing::warn!("AntiNuke: Bot-Log konnte nicht gesendet werden: {}", e);
        }
    }
}

