use std::collections::HashSet;
use std::time::Instant;

use poise::serenity_prelude as serenity;
use serenity::{
    ChannelId, ChannelType, CreateEmbed, CreateEmbedFooter, CreateMessage, GuildId, Timestamp,
};

use crate::config::ActionKind;

use crate::config::{CachedMessage, MESSAGE_CACHE_LIMIT, RAID_JOINS, RAID_WINDOW_SECS};
#[allow(unused_imports)]
use crate::config::InviteCache;
use crate::xp::{level_from_xp, XP_COOLDOWN_SECS};
use crate::{AppData, Error};

// ── helpers ───────────────────────────────────────────────────────────────────

async fn send_log(ctx: &serenity::Context, channel_id: ChannelId, embed: CreateEmbed) {
    if let Err(e) = channel_id
        .send_message(ctx, CreateMessage::new().embed(embed))
        .await
    {
        tracing::warn!("Log konnte nicht gesendet werden (Kanal {}): {}", channel_id, e);
    }
}

pub async fn send_bot_log(ctx: &serenity::Context, data: &AppData, guild_id: GuildId, embed: CreateEmbed) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.bot_log)
    };
    if let Some(ch) = log_ch {
        send_log(ctx, ch, embed).await;
    }
}

fn ch_name(ctx: &serenity::Context, guild_id: GuildId, channel_id: ChannelId) -> String {
    ctx.cache
        .guild(guild_id)
        .and_then(|g| g.channels.get(&channel_id).map(|c| c.name.clone()))
        .unwrap_or_else(|| channel_id.to_string())
}

fn channel_kind_de(kind: ChannelType) -> &'static str {
    match kind {
        ChannelType::Text => "Textkanal",
        ChannelType::Voice => "Sprachkanal",
        ChannelType::Category => "Kategorie",
        ChannelType::News => "Ankündigungskanal",
        ChannelType::Stage => "Bühne",
        ChannelType::Forum => "Forum",
        _ => "Kanal",
    }
}

// ── main dispatch ─────────────────────────────────────────────────────────────

pub async fn handle(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, AppData, Error>,
    data: &AppData,
) -> Result<(), Error> {
    match event {
        // Cache incoming messages for delete logs + XP
        serenity::FullEvent::Message { new_message } => {
            if !new_message.author.bot {
                let has_content = !new_message.content.is_empty();
                let has_attachments = !new_message.attachments.is_empty();

                // Persist to DB (for delete/edit logs — survives restarts, stores attachments)
                if let Some(guild_id) = new_message.guild_id {
                    if has_content || has_attachments {
                        let att_names: Vec<String> = new_message.attachments.iter()
                            .map(|a| a.filename.clone()).collect();
                        crate::db::store_message(
                            &data.db,
                            new_message.id,
                            guild_id,
                            new_message.channel_id,
                            new_message.author.id,
                            &new_message.author.tag(),
                            &new_message.content,
                            &att_names,
                        ).await;
                    }
                }

                if has_content {
                    // In-memory cache (fast path for edit lookups)
                    {
                        let mut cache = data.message_cache.lock().await;
                        let (map, queue) = &mut *cache;
                        if map.len() >= MESSAGE_CACHE_LIMIT {
                            if let Some(old_id) = queue.pop_front() {
                                map.remove(&old_id);
                            }
                        }
                        map.insert(
                            new_message.id,
                            CachedMessage {
                                author_id: new_message.author.id,
                                author_tag: new_message.author.tag(),
                                content: new_message.content.clone(),
                                channel_id: new_message.channel_id,
                            },
                        );
                        queue.push_back(new_message.id);
                    }

                    // XP (min 5 chars, 60s cooldown)
                    let long_enough = new_message.content.chars().count() >= 5;
                    if long_enough {
                        if let Some(guild_id) = new_message.guild_id {
                            let user_id = new_message.author.id;
                            let key = (guild_id, user_id);

                            let can_earn = {
                                let cooldowns = data.xp_cooldowns.lock().await;
                                cooldowns
                                    .get(&key)
                                    .map(|t| t.elapsed().as_secs() >= XP_COOLDOWN_SECS)
                                    .unwrap_or(true)
                            };

                            if can_earn {
                                data.xp_cooldowns.lock().await.insert(key, Instant::now());

                                let has_booster = crate::db::has_active_shop_item(&data.db, guild_id, user_id, "xp_booster").await;
                                let base_xp = 15 + (new_message.id.get() % 11) as u64;
                                let xp_gain = if has_booster { base_xp * 2 } else { base_xp };

                                let old_xp    = crate::db::get_xp(&data.db, guild_id, user_id).await;
                                let new_xp    = crate::db::add_xp(&data.db, guild_id, user_id, xp_gain).await;
                                let old_level = level_from_xp(old_xp);
                                let new_level = level_from_xp(new_xp);

                                if new_level > old_level && old_level < 50 {
                                    let reward = (new_level * 100) as i64;
                                    crate::db::add_coins(&data.db, guild_id, user_id, reward).await;
                                    crate::db::set_credited_level(&data.db, guild_id, user_id, new_level).await;

                                    let bot_ch = crate::db::get_bot_channel(&data.db, guild_id).await;
                                    let announce_ch = bot_ch.unwrap_or(new_message.channel_id);
                                    let _ = announce_ch.send_message(
                                        ctx,
                                        crate::commands::levels::level_up_embed(user_id, new_level),
                                    ).await;

                                    send_bot_log(ctx, data, guild_id, CreateEmbed::new()
                                        .title("⭐ Level-Up")
                                        .color(0x57F287u32)
                                        .field("Nutzer", format!("<@{}>", user_id), true)
                                        .field("Level", new_level.to_string(), true)
                                        .field("Gesamt-XP", new_xp.to_string(), true)
                                        .field("Coins-Bonus", format!("+{} Coins", reward), true)
                                        .timestamp(Timestamp::now())
                                    ).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── voice ─────────────────────────────────────────────────────────────
        serenity::FullEvent::VoiceStateUpdate { old, new } => {
            voice_log(ctx, data, old, new).await;
            voice_xp_track(data, old, new).await;
        }

        // ── messages ──────────────────────────────────────────────────────────
        serenity::FullEvent::MessageDelete {
            channel_id,
            deleted_message_id,
            guild_id,
        } => {
            message_delete_log(ctx, data, *channel_id, *deleted_message_id, *guild_id).await;
        }
        serenity::FullEvent::MessageDeleteBulk {
            channel_id,
            multiple_deleted_messages_ids,
            guild_id,
        } => {
            message_bulk_delete_log(ctx, data, *channel_id, multiple_deleted_messages_ids, *guild_id)
                .await;
        }
        serenity::FullEvent::MessageUpdate {
            old_if_available,
            new,
            event,
        } => {
            message_edit_log(ctx, data, old_if_available, new, event).await;
        }

        // ── join / leave ──────────────────────────────────────────────────────
        serenity::FullEvent::GuildMemberAddition { new_member } => {
            if new_member.user.bot {
                bot_join_log(ctx, data, new_member).await;
            } else {
                member_join_log(ctx, data, new_member).await;
                welcome_message(ctx, data, new_member).await;
                invite_join(ctx, data, new_member).await;
                assign_base_role_on_join(ctx, data, new_member).await;
                crate::antinuke::record_join(ctx, data, new_member.guild_id, new_member).await;
            }
        }
        serenity::FullEvent::GuildMemberRemoval {
            guild_id,
            user,
            member_data_if_available,
        } => {
            member_leave_log(ctx, data, *guild_id, user, member_data_if_available).await;
        }

        // ── server ────────────────────────────────────────────────────────────
        serenity::FullEvent::GuildRoleCreate { new } => {
            role_create_log(ctx, data, new).await;
            antinuke_event(ctx, data, new.guild_id, ActionKind::RoleCreate).await;
        }
        serenity::FullEvent::GuildRoleUpdate {
            old_data_if_available,
            new,
        } => {
            role_update_log(ctx, data, old_data_if_available, new).await;
        }
        serenity::FullEvent::GuildRoleDelete {
            guild_id,
            removed_role_id,
            removed_role_data_if_available,
        } => {
            role_delete_log(ctx, data, *guild_id, *removed_role_id, removed_role_data_if_available)
                .await;
            antinuke_event(ctx, data, *guild_id, ActionKind::RoleDelete).await;
        }
        serenity::FullEvent::ChannelCreate { channel } => {
            channel_create_log(ctx, data, channel).await;
            antinuke_event(ctx, data, channel.guild_id, ActionKind::ChannelCreate).await;
        }
        serenity::FullEvent::ChannelUpdate { old, new } => {
            channel_update_log(ctx, data, old, new).await;
        }
        serenity::FullEvent::ChannelDelete { channel, .. } => {
            channel_delete_log(ctx, data, channel).await;
            antinuke_event(ctx, data, channel.guild_id, ActionKind::ChannelDelete).await;
        }
        serenity::FullEvent::GuildUpdate {
            old_data_if_available,
            new_data,
        } => {
            guild_update_log(ctx, data, old_data_if_available, new_data).await;
        }
        serenity::FullEvent::GuildBanAddition {
            guild_id,
            banned_user,
        } => {
            ban_log(ctx, data, *guild_id, banned_user, true).await;
            antinuke_event(ctx, data, *guild_id, ActionKind::Ban).await;
        }
        serenity::FullEvent::WebhookUpdate { guild_id, .. } => {
            antinuke_event(ctx, data, *guild_id, ActionKind::WebhookCreate).await;
        }
        serenity::FullEvent::GuildBanRemoval {
            guild_id,
            unbanned_user,
        } => {
            ban_log(ctx, data, *guild_id, unbanned_user, false).await;
        }
        serenity::FullEvent::GuildEmojisUpdate {
            guild_id,
            current_state,
        } => {
            emoji_update_log(ctx, data, *guild_id, current_state.len()).await;
        }

        // ── members ───────────────────────────────────────────────────────────
        serenity::FullEvent::GuildMemberUpdate {
            old_if_available,
            new,
            event,
        } => {
            member_update_log(ctx, data, old_if_available, new.as_ref(), event).await;
        }

        // ── reactions ────────────────────────────────────────────────────────
        serenity::FullEvent::ReactionAdd { add_reaction } => {
            reaction_log(ctx, data, add_reaction, true).await;
        }
        serenity::FullEvent::ReactionRemove { removed_reaction } => {
            reaction_log(ctx, data, removed_reaction, false).await;
        }

        // ── threads ───────────────────────────────────────────────────────────
        serenity::FullEvent::ThreadCreate { thread } => {
            thread_create_log(ctx, data, thread).await;
        }
        serenity::FullEvent::ThreadDelete { thread, .. } => {
            thread_delete_log(ctx, data, thread).await;
        }

        // ── pins ──────────────────────────────────────────────────────────────
        serenity::FullEvent::ChannelPinsUpdate { pin } => {
            channel_pins_log(ctx, data, pin).await;
        }

        // ── stage ─────────────────────────────────────────────────────────────
        serenity::FullEvent::StageInstanceCreate { stage_instance } => {
            stage_instance_log(ctx, data, stage_instance, "gestartet").await;
        }
        serenity::FullEvent::StageInstanceDelete { stage_instance } => {
            stage_instance_log(ctx, data, stage_instance, "beendet").await;
        }

        // ── scheduled events ──────────────────────────────────────────────────
        serenity::FullEvent::GuildScheduledEventCreate { event } => {
            scheduled_event_log(ctx, data, event, "erstellt").await;
        }
        serenity::FullEvent::GuildScheduledEventUpdate { event } => {
            scheduled_event_log(ctx, data, event, "aktualisiert").await;
        }
        serenity::FullEvent::GuildScheduledEventDelete { event } => {
            scheduled_event_log(ctx, data, event, "gelöscht").await;
        }

        // Enforce USE_APPLICATION_COMMANDS for @everyone on startup and new guilds
        // Also cache invites for the invite tracker
        serenity::FullEvent::GuildCreate { guild, .. } => {
            enforce_app_commands(ctx, guild).await;
            cache_invites(ctx, data, guild.id).await;
            apply_base_role_on_startup(ctx, data, guild).await;
            reschedule_giveaways(ctx, data).await;
        }

        serenity::FullEvent::InteractionCreate { interaction } => {
            if let serenity::Interaction::Component(comp) = interaction {
                let id = comp.data.custom_id.as_str();
                if id == "giveaway_join" {
                    handle_giveaway_join(ctx, data, comp).await;
                } else if id.starts_with("remind_fish_") || id.starts_with("remind_arbeit_") {
                    handle_remind_button(ctx, comp).await;
                } else if id.starts_with("loot_claim_") {
                    crate::commands::shop::handle_loot_claim(
                        ctx, &data.db, comp,
                    ).await;
                }
            }
        }

        _ => {}
    }

    Ok(())
}

// ── enforce USE_APPLICATION_COMMANDS ─────────────────────────────────────────

async fn enforce_app_commands(ctx: &serenity::Context, guild: &serenity::Guild) {
    let everyone_id = serenity::RoleId::new(guild.id.get());

    let current_perms = match guild.roles.get(&everyone_id) {
        Some(role) => role.permissions,
        None => return,
    };

    if current_perms.contains(serenity::Permissions::USE_APPLICATION_COMMANDS) {
        return; // Already enabled
    }

    let new_perms = current_perms | serenity::Permissions::USE_APPLICATION_COMMANDS;

    match guild
        .id
        .edit_role(
            &ctx.http,
            everyone_id,
            serenity::EditRole::new().permissions(new_perms),
        )
        .await
    {
        Ok(_) => tracing::info!(
            "USE_APPLICATION_COMMANDS für @everyone in '{}' aktiviert",
            guild.name
        ),
        Err(e) => tracing::warn!(
            "Konnte USE_APPLICATION_COMMANDS in '{}' nicht setzen: {}",
            guild.name,
            e
        ),
    }
}

// ── voice logs ────────────────────────────────────────────────────────────────

async fn voice_log(
    ctx: &serenity::Context,
    data: &AppData,
    old: &Option<serenity::VoiceState>,
    new: &serenity::VoiceState,
) {
    let guild_id = match new.guild_id {
        Some(g) => g,
        None => return,
    };

    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.voice)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let (user_mention, avatar_url) = match &new.member {
        Some(m) => (format!("<@{}>", m.user.id), m.user.face()),
        None => (
            format!("<@{}>", new.user_id),
            "https://cdn.discordapp.com/embed/avatars/0.png".to_string(),
        ),
    };

    let old_ch = old.as_ref().and_then(|v| v.channel_id);
    let new_ch = new.channel_id;

    let embed = if old_ch != new_ch {
        match (old_ch, new_ch) {
            (None, Some(ch)) => {
                let name = ch_name(ctx, guild_id, ch);
                CreateEmbed::new()
                    .title("🔊 Voice beigetreten")
                    .description(format!("{} ist **{}** beigetreten", user_mention, name))
                    .color(0x57F287u32)
                    .thumbnail(avatar_url)
                    .footer(CreateEmbedFooter::new(format!("User ID: {}", new.user_id)))
                    .timestamp(Timestamp::now())
            }
            (Some(old_id), None) => {
                let name = ch_name(ctx, guild_id, old_id);
                CreateEmbed::new()
                    .title("🔇 Voice verlassen")
                    .description(format!("{} hat **{}** verlassen", user_mention, name))
                    .color(0xED4245u32)
                    .thumbnail(avatar_url)
                    .footer(CreateEmbedFooter::new(format!("User ID: {}", new.user_id)))
                    .timestamp(Timestamp::now())
            }
            (Some(old_id), Some(new_id)) => {
                let from = ch_name(ctx, guild_id, old_id);
                let to = ch_name(ctx, guild_id, new_id);
                CreateEmbed::new()
                    .title("🔀 Kanal gewechselt")
                    .description(format!(
                        "{} wechselte von **{}** zu **{}**",
                        user_mention, from, to
                    ))
                    .color(0x5865F2u32)
                    .thumbnail(avatar_url)
                    .footer(CreateEmbedFooter::new(format!("User ID: {}", new.user_id)))
                    .timestamp(Timestamp::now())
            }
            _ => return,
        }
    } else {
        let old_v = match old.as_ref() {
            Some(v) => v,
            None => return,
        };

        let (title, desc) = if old_v.self_mute != new.self_mute {
            if new.self_mute {
                ("🔇 Mikrofon stummgeschaltet", format!("{} hat sein Mikrofon stummgeschaltet", user_mention))
            } else {
                ("🎤 Mikrofon entstummt", format!("{} hat sein Mikrofon entstummt", user_mention))
            }
        } else if old_v.self_deaf != new.self_deaf {
            if new.self_deaf {
                ("🎧 Kopfhörer taubgestellt", format!("{} hat sich taubgestellt", user_mention))
            } else {
                ("🎧 Kopfhörer entstummt", format!("{} hat die Taubstellung aufgehoben", user_mention))
            }
        } else if old_v.mute != new.mute {
            if new.mute {
                ("🔇 Server-Stummschaltung", format!("{} wurde vom Server stummgeschaltet", user_mention))
            } else {
                ("🎤 Server-Stummschaltung aufgehoben", format!("{} wurde vom Server entstummt", user_mention))
            }
        } else if old_v.deaf != new.deaf {
            if new.deaf {
                ("🔕 Server-Taubstellung", format!("{} wurde vom Server taubgestellt", user_mention))
            } else {
                ("🔔 Server-Taubstellung aufgehoben", format!("Taubstellung von {} wurde aufgehoben", user_mention))
            }
        } else if old_v.self_stream != new.self_stream {
            if new.self_stream.unwrap_or(false) {
                ("📺 Stream gestartet", format!("{} hat einen Stream gestartet", user_mention))
            } else {
                ("📺 Stream beendet", format!("{} hat seinen Stream beendet", user_mention))
            }
        } else {
            return;
        };

        CreateEmbed::new()
            .title(title)
            .description(desc)
            .color(0xFEE75Cu32)
            .thumbnail(avatar_url)
            .footer(CreateEmbedFooter::new(format!("User ID: {}", new.user_id)))
            .timestamp(Timestamp::now())
    };

    send_log(ctx, log_ch, embed).await;
}

// ── message logs ──────────────────────────────────────────────────────────────

async fn message_delete_log(
    ctx: &serenity::Context,
    data: &AppData,
    channel_id: ChannelId,
    message_id: serenity::MessageId,
    guild_id: Option<GuildId>,
) {
    let guild_id = match guild_id {
        Some(g) => g,
        None => return,
    };

    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let cached = {
        let cache = data.message_cache.lock().await;
        cache.0.get(&message_id).cloned()
    };
    let stored = if cached.is_none() {
        crate::db::get_message(&data.db, message_id).await
    } else {
        None
    };

    let mut embed = CreateEmbed::new()
        .title("🗑️ Nachricht gelöscht")
        .color(0xED4245u32)
        .field("Kanal", format!("<#{}>", channel_id), true)
        .footer(CreateEmbedFooter::new(format!("Nachrichten-ID: {}", message_id)))
        .timestamp(Timestamp::now());

    if let Some(msg) = cached {
        embed = embed
            .field("Autor", format!("<@{}> ({})", msg.author_id, msg.author_tag), true)
            .field("Inhalt", if msg.content.is_empty() { "_kein Textinhalt_".to_string() } else { truncate(&msg.content, 1020) }, false);
    } else if let Some(msg) = stored {
        embed = embed.field("Autor", format!("<@{}> ({})", msg.user_id, msg.user_tag), true);
        if !msg.content.is_empty() {
            embed = embed.field("Inhalt", truncate(&msg.content, 1020), false);
        }
        if !msg.attachment_names.is_empty() {
            embed = embed.field("Anhänge", msg.attachment_names.join(", "), false);
        }
        if msg.content.is_empty() && msg.attachment_names.is_empty() {
            embed = embed.field("Inhalt", "_kein Textinhalt_", false);
        }
    } else {
        embed = embed.field("Inhalt", "_nicht im Cache_", false);
    }

    send_log(ctx, log_ch, embed).await;
}

async fn message_bulk_delete_log(
    ctx: &serenity::Context,
    data: &AppData,
    channel_id: ChannelId,
    ids: &[serenity::MessageId],
    guild_id: Option<GuildId>,
) {
    let guild_id = match guild_id {
        Some(g) => g,
        None => return,
    };

    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let embed = CreateEmbed::new()
        .title("🗑️ Massenlöschung")
        .description(format!(
            "**{}** Nachrichten wurden in <#{}> gelöscht.",
            ids.len(),
            channel_id
        ))
        .color(0xED4245u32)
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

async fn message_edit_log(
    ctx: &serenity::Context,
    data: &AppData,
    old: &Option<serenity::Message>,
    new: &Option<serenity::Message>,
    event: &serenity::MessageUpdateEvent,
) {
    let guild_id = match event.guild_id {
        Some(g) => g,
        None => return,
    };

    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let new_content = match event.content.as_deref().or(new.as_ref().map(|m| m.content.as_str())) {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => return, // embed-only update, not interesting
    };

    let old_content = if let Some(old_msg) = old {
        old_msg.content.clone()
    } else {
        let from_cache = {
            let cache = data.message_cache.lock().await;
            cache.0.get(&event.id).map(|m| m.content.clone())
        };
        if let Some(c) = from_cache {
            c
        } else {
            crate::db::get_message(&data.db, event.id).await
                .map(|m| m.content)
                .unwrap_or_else(|| "_nicht im Cache_".to_string())
        }
    };

    // Skip if content didn't actually change
    if old_content == new_content {
        return;
    }

    let author_text = match event.author.as_ref() {
        Some(u) => format!("<@{}> ({})", u.id, u.tag()),
        None => "Unbekannt".to_string(),
    };

    let jump_url = new
        .as_ref()
        .map(|m| m.link())
        .unwrap_or_default();

    let mut embed = CreateEmbed::new()
        .title("✏️ Nachricht bearbeitet")
        .color(0xFEE75Cu32)
        .field("Kanal", format!("<#{}>", event.channel_id), true)
        .field("Autor", author_text, true)
        .field("Vorher", truncate(&old_content, 1020), false)
        .field("Nachher", truncate(&new_content, 1020), false)
        .footer(CreateEmbedFooter::new(format!("Nachrichten-ID: {}", event.id)))
        .timestamp(Timestamp::now());

    if !jump_url.is_empty() {
        embed = embed.description(format!("[Zur Nachricht]({})", jump_url));
    }

    send_log(ctx, log_ch, embed).await;

    // Update DB + memory cache with new content so future edits show correct old content
    crate::db::update_message_content(&data.db, event.id, &new_content).await;
    {
        let mut cache = data.message_cache.lock().await;
        if let Some(msg) = cache.0.get_mut(&event.id) {
            msg.content = new_content;
        }
    }
}

// ── join / leave logs ─────────────────────────────────────────────────────────

async fn member_join_log(
    ctx: &serenity::Context,
    data: &AppData,
    member: &serenity::Member,
) {
    let guild_id = member.guild_id;

    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.join_leave)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let user = &member.user;
    let created_ts = user.id.created_at().unix_timestamp();
    let now_ts = chrono::Utc::now().timestamp();
    let age_days = (now_ts - created_ts) / 86400;

    // Raid detection
    let is_raid = {
        let mut tracker = data.join_tracker.lock().await;
        let joins = tracker.entry(guild_id).or_default();
        let cutoff = Instant::now()
            .checked_sub(std::time::Duration::from_secs(RAID_WINDOW_SECS))
            .unwrap_or(Instant::now());
        joins.retain(|t| *t > cutoff);
        joins.push_back(Instant::now());
        joins.len() >= RAID_JOINS
    };

    let member_count = ctx
        .cache
        .guild(guild_id)
        .map(|g| g.member_count)
        .unwrap_or(0);

    // Collect warnings
    let mut warnings: Vec<String> = Vec::new();

    if age_days < 1 {
        warnings.push("🚨 **Konto weniger als 24 Stunden alt** — sehr verdächtig".to_string());
    } else if age_days < 7 {
        warnings.push(format!("⚠️ **Konto erst {} Tage alt** — verdächtig", age_days));
    } else if age_days < 30 {
        warnings.push(format!("⚠️ **Konto erst {} Tage alt** — neues Konto", age_days));
    }

    if user.avatar.is_none() {
        warnings.push("⚠️ **Kein Profilbild** — Standard-Avatar".to_string());
    }

    if is_raid {
        warnings.push(format!(
            "🚨 **Möglicher Raid** — {} Beitritte in unter {} Sekunden",
            RAID_JOINS, RAID_WINDOW_SECS
        ));
    }

    // Additional bot suspicion
    if user.discriminator.is_none() && age_days < 7 && user.avatar.is_none() {
        warnings.push("🚨 **Hohes Risiko** — Neues Konto ohne Avatar (möglicher Bot/Alt)".to_string());
    }

    let color = if warnings.iter().any(|w| w.contains("🚨")) {
        0xED4245u32
    } else if !warnings.is_empty() {
        0xFEE75Cu32
    } else {
        0x57F287u32
    };

    let mut embed = CreateEmbed::new()
        .title("📥 Mitglied beigetreten")
        .color(color)
        .thumbnail(user.face())
        .field(
            "Nutzer",
            format!("<@{}> (`{}`)", user.id, user.tag()),
            false,
        )
        .field("ID", user.id.to_string(), true)
        .field("Mitgliederzahl", member_count.to_string(), true)
        .field(
            "Konto erstellt",
            format!("<t:{}:F> (<t:{}:R>)", created_ts, created_ts),
            false,
        )
        .footer(CreateEmbedFooter::new(format!("User ID: {}", user.id)))
        .timestamp(Timestamp::now());

    if !warnings.is_empty() {
        embed = embed.field("⚠️ Sicherheitshinweise", warnings.join("\n"), false);
    }

    send_log(ctx, log_ch, embed).await;

    if is_raid {
        send_bot_log(ctx, data, guild_id, CreateEmbed::new()
            .title("🚨 Raid erkannt")
            .color(0xED4245u32)
            .description(format!(
                "**{} Beitritte** in unter **{} Sekunden** erkannt.\nLetzter Beitritt: <@{}>",
                RAID_JOINS, RAID_WINDOW_SECS, user.id
            ))
            .timestamp(Timestamp::now())
        ).await;
    }
}

async fn member_leave_log(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    user: &serenity::User,
    member: &Option<serenity::Member>,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.join_leave)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let mut embed = CreateEmbed::new()
        .title("📤 Mitglied verlassen")
        .color(0xED4245u32)
        .thumbnail(user.face())
        .field("Nutzer", format!("<@{}> (`{}`)", user.id, user.tag()), false)
        .field("ID", user.id.to_string(), true)
        .footer(CreateEmbedFooter::new(format!("User ID: {}", user.id)))
        .timestamp(Timestamp::now());

    if let Some(m) = member {
        if let Some(joined_at) = m.joined_at {
            let joined_ts = joined_at.unix_timestamp();
            let duration_secs = chrono::Utc::now().timestamp() - joined_ts;
            embed = embed
                .field(
                    "Beigetreten",
                    format!("<t:{}:F> (<t:{}:R>)", joined_ts, joined_ts),
                    false,
                )
                .field("Zeit auf Server", format_duration(duration_secs), true);
        }

        if !m.roles.is_empty() {
            let roles = m
                .roles
                .iter()
                .map(|r| format!("<@&{}>", r))
                .collect::<Vec<_>>()
                .join(", ");
            embed = embed.field("Rollen", truncate(&roles, 1020), false);
        }
    }

    send_log(ctx, log_ch, embed).await;
}

// ── server logs ───────────────────────────────────────────────────────────────

async fn role_create_log(ctx: &serenity::Context, data: &AppData, role: &serenity::Role) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&role.guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let color_hex = format!("#{:06X}", role.colour.0);

    let embed = CreateEmbed::new()
        .title("✅ Rolle erstellt")
        .color(0x57F287u32)
        .field("Name", format!("<@&{}>  ({})", role.id, role.name), false)
        .field("Farbe", &color_hex, true)
        .field("Angeheftet", yes_no(role.hoist), true)
        .field("Erwähnbar", yes_no(role.mentionable), true)
        .footer(CreateEmbedFooter::new(format!("Rollen-ID: {}", role.id)))
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

async fn role_update_log(
    ctx: &serenity::Context,
    data: &AppData,
    old: &Option<serenity::Role>,
    new: &serenity::Role,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&new.guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let mut changes: Vec<(String, String)> = Vec::new();

    if let Some(old) = old {
        if old.name != new.name {
            changes.push(("Name".to_string(), format!("`{}` → `{}`", old.name, new.name)));
        }
        if old.colour != new.colour {
            changes.push(("Farbe".to_string(), format!("`#{:06X}` → `#{:06X}`", old.colour.0, new.colour.0)));
        }
        if old.hoist != new.hoist {
            changes.push(("Angeheftet".to_string(), format!("{} → {}", yes_no(old.hoist), yes_no(new.hoist))));
        }
        if old.mentionable != new.mentionable {
            changes.push(("Erwähnbar".to_string(), format!("{} → {}", yes_no(old.mentionable), yes_no(new.mentionable))));
        }
        if old.permissions != new.permissions {
            changes.push(("Berechtigungen".to_string(), "geändert".to_string()));
        }
    }

    if changes.is_empty() && old.is_some() {
        return; // Nothing visible changed
    }

    let mut embed = CreateEmbed::new()
        .title("✏️ Rolle aktualisiert")
        .color(0xFEE75Cu32)
        .field("Rolle", format!("<@&{}>  ({})", new.id, new.name), false)
        .footer(CreateEmbedFooter::new(format!("Rollen-ID: {}", new.id)))
        .timestamp(Timestamp::now());

    for (name, value) in changes {
        embed = embed.field(name, value, true);
    }

    send_log(ctx, log_ch, embed).await;
}

async fn role_delete_log(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    role_id: serenity::RoleId,
    role: &Option<serenity::Role>,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let name = role
        .as_ref()
        .map(|r| r.name.as_str())
        .unwrap_or("Unbekannt");

    let embed = CreateEmbed::new()
        .title("🗑️ Rolle gelöscht")
        .color(0xED4245u32)
        .field("Name", name, true)
        .footer(CreateEmbedFooter::new(format!("Rollen-ID: {}", role_id)))
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

async fn channel_create_log(
    ctx: &serenity::Context,
    data: &AppData,
    channel: &serenity::GuildChannel,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&channel.guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let embed = CreateEmbed::new()
        .title("✅ Kanal erstellt")
        .color(0x57F287u32)
        .field("Name", format!("<#{}> (`{}`)", channel.id, channel.name), false)
        .field("Typ", channel_kind_de(channel.kind), true)
        .footer(CreateEmbedFooter::new(format!("Kanal-ID: {}", channel.id)))
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

async fn channel_update_log(
    ctx: &serenity::Context,
    data: &AppData,
    old: &Option<serenity::GuildChannel>,
    new: &serenity::GuildChannel,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&new.guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let mut changes: Vec<(String, String)> = Vec::new();

    if let Some(old) = old {
        if old.name != new.name {
            changes.push(("Name".to_string(), format!("`{}` → `{}`", old.name, new.name)));
        }
        if old.topic != new.topic {
            let old_t = old.topic.as_deref().unwrap_or("_keines_");
            let new_t = new.topic.as_deref().unwrap_or("_keines_");
            changes.push(("Thema".to_string(), format!("`{}` → `{}`", old_t, new_t)));
        }
        if old.rate_limit_per_user != new.rate_limit_per_user {
            let old_s = old.rate_limit_per_user.unwrap_or(0);
            let new_s = new.rate_limit_per_user.unwrap_or(0);
            changes.push(("Langsamschritt".to_string(), format!("{}s → {}s", old_s, new_s)));
        }
        if old.nsfw != new.nsfw {
            changes.push(("NSFW".to_string(), format!("{} → {}", yes_no(old.nsfw), yes_no(new.nsfw))));
        }
    }

    if changes.is_empty() && old.is_some() {
        return;
    }

    let mut embed = CreateEmbed::new()
        .title("✏️ Kanal aktualisiert")
        .color(0xFEE75Cu32)
        .field("Kanal", format!("<#{}> (`{}`)", new.id, new.name), false)
        .footer(CreateEmbedFooter::new(format!("Kanal-ID: {}", new.id)))
        .timestamp(Timestamp::now());

    for (name, value) in changes {
        embed = embed.field(name, value, false);
    }

    send_log(ctx, log_ch, embed).await;
}

async fn channel_delete_log(
    ctx: &serenity::Context,
    data: &AppData,
    channel: &serenity::GuildChannel,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&channel.guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let embed = CreateEmbed::new()
        .title("🗑️ Kanal gelöscht")
        .color(0xED4245u32)
        .field("Name", format!("`{}`", channel.name), true)
        .field("Typ", channel_kind_de(channel.kind), true)
        .footer(CreateEmbedFooter::new(format!("Kanal-ID: {}", channel.id)))
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

async fn guild_update_log(
    ctx: &serenity::Context,
    data: &AppData,
    old: &Option<serenity::Guild>,
    new: &serenity::PartialGuild,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&new.id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let mut changes: Vec<(String, String)> = Vec::new();

    if let Some(old) = old {
        if old.name != new.name {
            changes.push(("Name".to_string(), format!("`{}` → `{}`", old.name, new.name)));
        }
        if old.verification_level != new.verification_level {
            changes.push(("Verifizierungsstufe".to_string(), format!(
                "`{:?}` → `{:?}`",
                old.verification_level, new.verification_level
            )));
        }
        if old.owner_id != new.owner_id {
            changes.push(("Besitzer".to_string(), format!(
                "<@{}> → <@{}>",
                old.owner_id, new.owner_id
            )));
        }
        if old.icon != new.icon {
            changes.push(("Icon".to_string(), "geändert".to_string()));
        }
    }

    if changes.is_empty() && old.is_some() {
        return;
    }

    let mut embed = CreateEmbed::new()
        .title("⚙️ Server aktualisiert")
        .color(0xFEE75Cu32)
        .footer(CreateEmbedFooter::new(format!("Server-ID: {}", new.id)))
        .timestamp(Timestamp::now());

    if changes.is_empty() {
        embed = embed.description("Server-Einstellungen wurden geändert.");
    } else {
        for (name, value) in changes {
            embed = embed.field(name, value, false);
        }
    }

    send_log(ctx, log_ch, embed).await;
}

async fn ban_log(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    user: &serenity::User,
    is_ban: bool,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let (title, color) = if is_ban {
        ("🔨 Nutzer gebannt", 0xED4245u32)
    } else {
        ("✅ Nutzer entbannt", 0x57F287u32)
    };

    let embed = CreateEmbed::new()
        .title(title)
        .color(color)
        .thumbnail(user.face())
        .field("Nutzer", format!("<@{}> (`{}`)", user.id, user.tag()), false)
        .field("ID", user.id.to_string(), true)
        .footer(CreateEmbedFooter::new(format!("User ID: {}", user.id)))
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

async fn emoji_update_log(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    count: usize,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.server)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let embed = CreateEmbed::new()
        .title("😀 Emojis aktualisiert")
        .description(format!("Der Server hat jetzt **{}** Emojis.", count))
        .color(0x5865F2u32)
        .timestamp(Timestamp::now());

    send_log(ctx, log_ch, embed).await;
}

// ── member logs ───────────────────────────────────────────────────────────────

async fn member_update_log(
    ctx: &serenity::Context,
    data: &AppData,
    old: &Option<serenity::Member>,
    new: Option<&serenity::Member>,
    event: &serenity::GuildMemberUpdateEvent,
) {
    let guild_id = event.guild_id;

    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.members)
    };
    let log_ch = match log_ch {
        Some(ch) => ch,
        None => return,
    };

    let user = &event.user;
    let mut changes: Vec<(String, String)> = Vec::new();
    let mut avatar_change: Option<(Option<String>, Option<String>)> = None;

    // Nick change
    let old_nick = old.as_ref().and_then(|m| m.nick.clone());
    let new_nick = event.nick.clone().or_else(|| new.and_then(|m| m.nick.clone()));

    if old_nick != new_nick {
        let old_display = old_nick.as_deref().unwrap_or("_kein Nickname_");
        let new_display = new_nick.as_deref().unwrap_or("_kein Nickname_");
        changes.push(("Nickname".to_string(), format!("`{}` → `{}`", old_display, new_display)));
    }

    // Role changes
    if let Some(old_m) = old {
        let old_roles: HashSet<serenity::RoleId> = old_m.roles.iter().copied().collect();
        let new_roles: HashSet<serenity::RoleId> = event.roles.iter().copied().collect();

        let added: Vec<_> = new_roles.difference(&old_roles).collect();
        let removed: Vec<_> = old_roles.difference(&new_roles).collect();

        if !added.is_empty() {
            let roles = added.iter().map(|r| format!("<@&{}>", r)).collect::<Vec<_>>().join(", ");
            changes.push(("Rolle hinzugefügt".to_string(), roles));
        }
        if !removed.is_empty() {
            let roles = removed.iter().map(|r| format!("<@&{}>", r)).collect::<Vec<_>>().join(", ");
            changes.push(("Rolle entfernt".to_string(), roles));
        }

        // Global avatar change
        if old_m.user.avatar != event.user.avatar {
            let old_url = old_m.user.avatar.map(|h| {
                format!("https://cdn.discordapp.com/avatars/{}/{}.png?size=256", user.id, h)
            });
            let new_url = event.user.avatar.map(|h| {
                format!("https://cdn.discordapp.com/avatars/{}/{}.png?size=256", user.id, h)
            });
            avatar_change = Some((old_url, new_url));
        }

        // Guild-specific avatar change
        if old_m.avatar != event.avatar {
            let new_url = event.avatar.as_ref().map(|hash| {
                format!(
                    "https://cdn.discordapp.com/guilds/{}/users/{}/avatars/{}.png?size=256",
                    event.guild_id, user.id, hash
                )
            });
            let val = new_url.as_deref().unwrap_or("entfernt");
            changes.push(("Server-Avatar".to_string(), val.to_string()));
        }
    }

    // Timeout change
    if let Some(old_m) = old {
        if old_m.communication_disabled_until != event.communication_disabled_until {
            match event.communication_disabled_until {
                Some(ts) => changes.push((
                    "Timeout".to_string(),
                    format!("bis <t:{}:F>", ts.unix_timestamp()),
                )),
                None => changes.push(("Timeout".to_string(), "aufgehoben".to_string())),
            }
        }
    }

    if changes.is_empty() && avatar_change.is_none() {
        return;
    }

    let mut embed = CreateEmbed::new()
        .title("👤 Mitglied aktualisiert")
        .color(0x5865F2u32)
        .field("Nutzer", format!("<@{}> (`{}`)", user.id, user.tag()), false)
        .footer(CreateEmbedFooter::new(format!("User ID: {}", user.id)))
        .timestamp(Timestamp::now());

    for (name, value) in changes {
        embed = embed.field(name, truncate(&value, 1020), false);
    }

    if let Some((old_url, new_url)) = avatar_change {
        let mut desc = String::new();
        if let Some(ref url) = old_url {
            desc.push_str(&format!("**Vorher:** [Link]({})\n", url));
        } else {
            desc.push_str("**Vorher:** Standard-Avatar\n");
        }
        if let Some(ref url) = new_url {
            desc.push_str(&format!("**Nachher:** [Link]({})", url));
        } else {
            desc.push_str("**Nachher:** Standard-Avatar (entfernt)");
        }
        embed = embed.field("🖼️ Profilbild geändert", desc, false);
        // Show old as thumbnail, new as image
        if let Some(url) = old_url {
            embed = embed.thumbnail(url);
        }
        if let Some(url) = new_url {
            embed = embed.image(url);
        }
    } else {
        embed = embed.thumbnail(user.face());
    }

    send_log(ctx, log_ch, embed).await;
}

// ── welcome message ───────────────────────────────────────────────────────────

async fn welcome_message(
    ctx: &serenity::Context,
    data: &AppData,
    member: &serenity::Member,
) {
    let guild_id = member.guild_id;

    let welcome_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.welcome)
    };
    let welcome_ch = match welcome_ch {
        Some(ch) => ch,
        None => return,
    };

    let user = &member.user;

    let member_count = ctx
        .cache
        .guild(guild_id)
        .map(|g| g.member_count)
        .unwrap_or(0);

    let guild_name = ctx
        .cache
        .guild(guild_id)
        .map(|g| g.name.clone())
        .unwrap_or_else(|| guild_id.to_string());

    let description = "_ _\n_ _ \n_ _                            Willkommen \n_ _                       \n_ _ \n_ _";

    let embed = CreateEmbed::new()
        .description(description)
        .thumbnail(user.face())
        .footer(CreateEmbedFooter::new(format!("{} @ {}", member_count, guild_name)))
        .color(0x2B2D31u32);

    match welcome_ch
        .send_message(
            ctx,
            CreateMessage::new()
                .content(format!("<@{}>", user.id))
                .embed(embed),
        )
        .await
    {
        Ok(_) => {
            send_bot_log(ctx, data, guild_id, CreateEmbed::new()
                .title("👋 Willkommensnachricht gesendet")
                .color(0x5865F2u32)
                .field("Nutzer", format!("<@{}> ({})", user.id, user.tag()), true)
                .field("Kanal", format!("<#{}>", welcome_ch), true)
                .timestamp(Timestamp::now())
            ).await;
        }
        Err(e) => {
            tracing::warn!("Willkommensnachricht konnte nicht gesendet werden: {}", e);
        }
    }
}

// ── invite tracking ───────────────────────────────────────────────────────────

async fn bot_join_log(ctx: &serenity::Context, data: &AppData, member: &serenity::Member) {
    let guild_id = member.guild_id;
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.join_leave)
    };
    let Some(log_ch) = log_ch else { return };
    let user = &member.user;
    let embed = CreateEmbed::new()
        .title("🤖 Bot hinzugefügt")
        .color(0x5865F2u32)
        .thumbnail(user.face())
        .field("Bot", format!("<@{}> (`{}`)", user.id, user.tag()), false)
        .field("ID", user.id.to_string(), true)
        .footer(CreateEmbedFooter::new(format!("Bot ID: {}", user.id)))
        .timestamp(Timestamp::now());
    send_log(ctx, log_ch, embed).await;
}

async fn reaction_log(
    ctx: &serenity::Context,
    data: &AppData,
    reaction: &serenity::Reaction,
    is_add: bool,
) {
    let guild_id = match reaction.guild_id { Some(g) => g, None => return };
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let Some(log_ch) = log_ch else { return };
    let Some(user_id) = reaction.user_id else { return };

    let (title, color) = if is_add {
        ("➕ Reaktion hinzugefügt", 0x57F287u32)
    } else {
        ("➖ Reaktion entfernt", 0xED4245u32)
    };
    let embed = CreateEmbed::new()
        .title(title)
        .color(color)
        .field("Nutzer", format!("<@{}>", user_id), true)
        .field("Emoji", reaction.emoji.to_string(), true)
        .field("Kanal", format!("<#{}>", reaction.channel_id), true)
        .footer(CreateEmbedFooter::new(format!("Nachrichten-ID: {}", reaction.message_id)))
        .timestamp(Timestamp::now());
    send_log(ctx, log_ch, embed).await;
}

async fn thread_create_log(ctx: &serenity::Context, data: &AppData, thread: &serenity::GuildChannel) {
    let guild_id = thread.guild_id;
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let Some(log_ch) = log_ch else { return };
    let parent = thread.parent_id.map(|id| format!("<#{}>", id)).unwrap_or_else(|| "Unbekannt".to_string());
    let embed = CreateEmbed::new()
        .title("🧵 Thread erstellt")
        .color(0x57F287u32)
        .field("Name", format!("<#{}> (`{}`)", thread.id, thread.name), false)
        .field("Elternkanal", parent, true)
        .field("Typ", channel_kind_de(thread.kind), true)
        .footer(CreateEmbedFooter::new(format!("Thread-ID: {}", thread.id)))
        .timestamp(Timestamp::now());
    send_log(ctx, log_ch, embed).await;
}

async fn thread_delete_log(ctx: &serenity::Context, data: &AppData, thread: &serenity::PartialGuildChannel) {
    let guild_id = thread.guild_id;
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let Some(log_ch) = log_ch else { return };
    let parent = format!("<#{}>", thread.parent_id);
    let embed = CreateEmbed::new()
        .title("🗑️ Thread gelöscht")
        .color(0xED4245u32)
        .field("Elternkanal", parent, true)
        .footer(CreateEmbedFooter::new(format!("Thread-ID: {}", thread.id)))
        .timestamp(Timestamp::now());
    send_log(ctx, log_ch, embed).await;
}

async fn channel_pins_log(ctx: &serenity::Context, data: &AppData, pin: &serenity::ChannelPinsUpdateEvent) {
    let guild_id = match pin.guild_id { Some(g) => g, None => return };
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&guild_id).and_then(|c| c.messages)
    };
    let Some(log_ch) = log_ch else { return };
    let mut embed = CreateEmbed::new()
        .title("📌 Pins aktualisiert")
        .color(0xFEE75Cu32)
        .field("Kanal", format!("<#{}>", pin.channel_id), true)
        .timestamp(Timestamp::now());
    if let Some(ts) = pin.last_pin_timestamp {
        embed = embed.field("Letzter Pin", format!("<t:{}:R>", ts.unix_timestamp()), true);
    }
    send_log(ctx, log_ch, embed).await;
}

async fn stage_instance_log(
    ctx: &serenity::Context,
    data: &AppData,
    stage: &serenity::StageInstance,
    action: &str,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&stage.guild_id).and_then(|c| c.voice)
    };
    let Some(log_ch) = log_ch else { return };
    let color = if action == "gestartet" { 0x57F287u32 } else { 0xED4245u32 };
    let embed = CreateEmbed::new()
        .title(format!("🎭 Bühne {}", action))
        .color(color)
        .field("Thema", &stage.topic, false)
        .field("Kanal", format!("<#{}>", stage.channel_id), true)
        .timestamp(Timestamp::now());
    send_log(ctx, log_ch, embed).await;
}

async fn scheduled_event_log(
    ctx: &serenity::Context,
    data: &AppData,
    event: &serenity::ScheduledEvent,
    action: &str,
) {
    let log_ch = {
        let c = data.log_configs.lock().await;
        c.get(&event.guild_id).and_then(|c| c.server)
    };
    let Some(log_ch) = log_ch else { return };
    let color = match action { "erstellt" => 0x57F287u32, "gelöscht" => 0xED4245u32, _ => 0x5865F2u32 };
    let start_ts = event.start_time.unix_timestamp();
    let mut embed = CreateEmbed::new()
        .title(format!("📅 Event {}", action))
        .color(color)
        .field("Name", &event.name, false)
        .field("Start", format!("<t:{}:F>", start_ts), true)
        .timestamp(Timestamp::now());
    if let Some(desc) = &event.description {
        if !desc.is_empty() {
            embed = embed.field("Beschreibung", truncate(desc, 200), false);
        }
    }
    if let Some(creator) = &event.creator {
        embed = embed.field("Erstellt von", format!("<@{}>", creator.id), true);
    }
    send_log(ctx, log_ch, embed).await;
}

async fn assign_base_role_on_join(ctx: &serenity::Context, data: &AppData, member: &serenity::Member) {
    let guild_id = member.guild_id;
    let base_role = {
        let configs = data.log_configs.lock().await;
        configs.get(&guild_id).and_then(|c| c.base_role)
    };
    let Some(role_id) = base_role else { return };

    if let Err(e) = ctx.http.add_member_role(guild_id, member.user.id, role_id, Some("Basisrolle")).await {
        tracing::warn!("Basisrolle für {} fehlgeschlagen: {e}", member.user.id);
    }
}

async fn apply_base_role_on_startup(ctx: &serenity::Context, data: &AppData, guild: &serenity::Guild) {
    let guild_id = guild.id;
    let base_role = {
        let configs = data.log_configs.lock().await;
        configs.get(&guild_id).and_then(|c| c.base_role)
    };
    let Some(role_id) = base_role else { return };

    let jailed: std::collections::HashSet<serenity::UserId> =
        crate::db::get_jailed_user_ids(&data.db, guild_id).await.into_iter().collect();

    let needs_role: Vec<serenity::UserId> = guild
        .members
        .iter()
        .filter(|(id, m)| !m.user.bot && !jailed.contains(*id) && !m.roles.contains(&role_id))
        .map(|(id, _)| *id)
        .collect();

    for user_id in needs_role {
        if let Err(e) = ctx.http.add_member_role(guild_id, user_id, role_id, Some("Basisrolle")).await {
            tracing::warn!("Basisrolle für {user_id} fehlgeschlagen: {e}");
        }
    }
}

async fn cache_invites(ctx: &serenity::Context, data: &AppData, guild_id: GuildId) {
    match guild_id.invites(ctx.http.as_ref()).await {
        Ok(invites) => {
            let mut cache = data.invite_cache.lock().await;
            let map = cache.entry(guild_id).or_default();
            map.clear();
            for inv in &invites {
                map.insert(inv.code.clone(), inv.uses);
            }
        }
        Err(e) => tracing::warn!("Einladungs-Cache für {guild_id} fehlgeschlagen: {e}"),
    }
}

async fn invite_join(ctx: &serenity::Context, data: &AppData, member: &serenity::Member) {
    let guild_id = member.guild_id;
    let bot_id = ctx.cache.current_user().id;

    let current: Vec<serenity::RichInvite> = match guild_id.invites(ctx.http.as_ref()).await {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("Einladungen abrufen fehlgeschlagen: {e}");
            return;
        }
    };

    // Find which invite gained a use compared to our cached counts
    let inviter_id = {
        let cache = data.invite_cache.lock().await;
        let cached = match cache.get(&guild_id) {
            Some(m) => m,
            None => return,
        };
        current.iter().find_map(|inv| {
            let prev = cached.get(&inv.code).copied().unwrap_or(0);
            if inv.uses > prev {
                inv.inviter.as_ref().map(|u| u.id)
            } else {
                None
            }
        })
    };

    // Refresh cache with new counts
    {
        let mut cache = data.invite_cache.lock().await;
        let map = cache.entry(guild_id).or_default();
        map.clear();
        for inv in &current {
            map.insert(inv.code.clone(), inv.uses);
        }
    }

    let Some(inviter_id) = inviter_id else { return };
    if inviter_id == bot_id { return }

    let new_balance = crate::db::record_invite(&data.db, guild_id, inviter_id).await;

    send_bot_log(ctx, data, guild_id, CreateEmbed::new()
        .title("🎟️ Einladung")
        .color(0xF1C40Fu32)
        .field("Eingeladen", format!("<@{}>", member.user.id), true)
        .field("Einlader", format!("<@{}>", inviter_id), true)
        .field("Belohnung", format!("+100 Coins → **{} Coins** gesamt", new_balance), false)
        .timestamp(Timestamp::now())
    ).await;
}

// ── utilities ─────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn yes_no(b: bool) -> &'static str {
    if b { "Ja" } else { "Nein" }
}

fn format_duration(secs: i64) -> String {
    if secs < 60 {
        format!("{} Sekunden", secs)
    } else if secs < 3600 {
        format!("{} Minuten", secs / 60)
    } else if secs < 86400 {
        format!("{} Stunden", secs / 3600)
    } else {
        format!("{} Tage", secs / 86400)
    }
}

// ── giveaway helpers ──────────────────────────────────────────────────────────

/// Called from GuildCreate — reschedules any giveaways that survived a restart.
/// Uses a static flag so it only runs once even if multiple guilds fire GuildCreate.
async fn reschedule_giveaways(ctx: &serenity::Context, data: &AppData) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) { return; }

    let active = crate::db::get_active_giveaways(&data.db).await;
    if active.is_empty() { return; }
    tracing::info!("{} aktive Gewinnspiele werden nach Neustart neu geplant", active.len());

    for g in active {
        crate::commands::giveaway::schedule_giveaway_end(
            ctx.clone(),
            data.db.clone(),
            g.id,
            g.ends_at,
        );
    }
}

// ── reminder button ───────────────────────────────────────────────────────────

async fn handle_remind_button(ctx: &serenity::Context, comp: &serenity::ComponentInteraction) {
    // custom_id format: remind_fish_{user_id}_{ready_at}  or  remind_arbeit_{user_id}_{ready_at}
    let parts: Vec<&str> = comp.data.custom_id.splitn(4, '_').collect();
    // parts: ["remind", "fish"|"arbeit", user_id, ready_at]
    let (kind, user_id_str, ready_at_str) = match parts.as_slice() {
        [_, kind, uid, ts] => (*kind, *uid, *ts),
        _ => return,
    };

    let clicker_id = comp.user.id;
    let Ok(owner_id) = user_id_str.parse::<u64>() else { return };
    let Ok(ready_at) = ready_at_str.parse::<i64>()  else { return };

    // Only the original command user may click
    if clicker_id.get() != owner_id {
        let _ = comp.create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content("Das ist nicht deine Erinnerung.")
                    .ephemeral(true),
            ),
        ).await;
        return;
    }

    let now = chrono::Utc::now().timestamp();
    let delay_secs = (ready_at - now).max(0) as u64;

    let label = if kind == "fish" { "angeln" } else { "arbeiten" };
    let _ = comp.create_response(
        ctx,
        serenity::CreateInteractionResponse::Message(
            serenity::CreateInteractionResponseMessage::new()
                .content(format!("🔔 Ich erinnere dich, wenn du wieder {} kannst!", label))
                .ephemeral(true),
        ),
    ).await;

    let http = ctx.http.clone();
    let channel_id = comp.channel_id;
    let dm_content = if kind == "fish" {
        format!("🎣 **Du kannst wieder angeln!** → <#{}>", channel_id)
    } else {
        format!("💼 **Du kannst wieder arbeiten!** → <#{}>", channel_id)
    };

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
        if let Ok(ch) = serenity::UserId::new(owner_id).create_dm_channel(&http).await {
            let _ = ch.say(&http, dm_content).await;
        }
    });
}

/// Handles a click on the "🎟️ Teilnehmen" button.
async fn handle_giveaway_join(
    ctx:  &serenity::Context,
    data: &AppData,
    comp: &serenity::ComponentInteraction,
) {
    use serenity::{CreateInteractionResponse, CreateInteractionResponseMessage};

    let message_id = comp.message.id;
    let user_id    = comp.user.id;
    let pool       = &data.db;

    let Some(giveaway) = crate::db::get_giveaway_by_message(pool, message_id).await else {
        comp.create_response(&ctx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content("Dieses Gewinnspiel ist nicht mehr aktiv.")
                .ephemeral(true),
        )).await.ok();
        return;
    };

    // Level check
    if giveaway.required_level > 0 {
        let xp    = crate::db::get_xp(pool, giveaway.guild_id, user_id).await;
        let level = crate::xp::level_from_xp(xp) as i64;
        if level < giveaway.required_level {
            comp.create_response(&ctx.http, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(format!(
                        "Du brauchst mindestens **Level {}** zum Teilnehmen (du bist Level {}).",
                        giveaway.required_level, level,
                    ))
                    .ephemeral(true),
            )).await.ok();
            return;
        }
    }

    // Already entered?
    if crate::db::is_entered(pool, giveaway.id, user_id).await {
        comp.create_response(&ctx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content("Du nimmst bereits teil!")
                .ephemeral(true),
        )).await.ok();
        return;
    }

    // Ticket price check
    if giveaway.ticket_price > 0 {
        let balance = crate::db::get_coins(pool, giveaway.guild_id, user_id).await;
        if balance < giveaway.ticket_price {
            comp.create_response(&ctx.http, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(format!(
                        "Du hast nicht genug Coins. Ticketpreis: **{} Coins**, dein Kontostand: **{} Coins**.",
                        giveaway.ticket_price, balance,
                    ))
                    .ephemeral(true),
            )).await.ok();
            return;
        }
    }

    // Enter first, then deduct — so coins are never lost if the DB entry fails.
    crate::db::enter_giveaway(pool, giveaway.id, user_id).await;
    if giveaway.ticket_price > 0 {
        crate::db::add_coins(pool, giveaway.guild_id, user_id, -giveaway.ticket_price).await;
    }
    let new_count = crate::db::get_giveaway_entry_count(pool, giveaway.id).await;

    // Update the participant count on the embed
    use serenity::EditMessage;
    let updated_embed = crate::commands::giveaway::giveaway_embed(
        &giveaway.prize,
        giveaway.ticket_price,
        giveaway.required_level,
        giveaway.ends_at,
        new_count,
        false,
        None,
    );
    if let Some(msg_id) = giveaway.message_id {
        giveaway.channel_id.edit_message(
            &ctx.http,
            msg_id,
            EditMessage::new().embed(updated_embed),
        ).await.ok();
    }

    let confirm = if giveaway.ticket_price > 0 {
        format!("✅ Du nimmst jetzt teil! **{} Coins** wurden abgezogen.", giveaway.ticket_price)
    } else {
        "✅ Du nimmst jetzt teil! Viel Glück!".to_string()
    };

    comp.create_response(&ctx.http, CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(confirm)
            .ephemeral(true),
    )).await.ok();
}

// ── voice XP tracking ─────────────────────────────────────────────────────────

async fn voice_xp_track(
    data: &AppData,
    old:  &Option<serenity::VoiceState>,
    new:  &serenity::VoiceState,
) {
    let guild_id = match new.guild_id { Some(g) => g, None => return };
    let user_id  = new.user_id;

    // skip bots
    if new.member.as_ref().map(|m| m.user.bot).unwrap_or(false) { return; }

    let old_ch = old.as_ref().and_then(|v| v.channel_id);
    let new_ch = new.channel_id;

    match (old_ch, new_ch) {
        (None, Some(_)) => {
            // joined VC
            data.voice_sessions.lock().await.insert((guild_id, user_id), std::time::Instant::now());
        }
        (Some(_), None) => {
            // left VC
            data.voice_sessions.lock().await.remove(&(guild_id, user_id));
        }
        _ => {} // move between channels or mute/deaf changes — keep session
    }
}


// ── anti-nuke audit log helper ────────────────────────────────────────────────

fn kind_to_audit_action(kind: crate::config::ActionKind) -> serenity::audit_log::Action {
    use serenity::audit_log::{
        Action, ChannelAction, MemberAction, RoleAction, WebhookAction,
    };
    match kind {
        crate::config::ActionKind::ChannelDelete  => Action::Channel(ChannelAction::Delete),
        crate::config::ActionKind::ChannelCreate  => Action::Channel(ChannelAction::Create),
        crate::config::ActionKind::RoleDelete     => Action::Role(RoleAction::Delete),
        crate::config::ActionKind::RoleCreate     => Action::Role(RoleAction::Create),
        crate::config::ActionKind::Ban            => Action::Member(MemberAction::BanAdd),
        crate::config::ActionKind::WebhookCreate  => Action::Webhook(WebhookAction::Create),
    }
}

async fn antinuke_event(
    ctx: &serenity::Context,
    data: &AppData,
    guild_id: GuildId,
    kind: crate::config::ActionKind,
) {
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let action = kind_to_audit_action(kind);
    let logs = match ctx
        .http
        .get_audit_logs(guild_id, Some(action), None, None, Some(1))
        .await
    {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("AntiNuke: Audit-Log konnte nicht abgerufen werden: {}", e);
            return;
        }
    };

    if let Some(entry) = logs.entries.first() {
        crate::antinuke::record_action(ctx, data, guild_id, entry.user_id, kind).await;
    }
}
