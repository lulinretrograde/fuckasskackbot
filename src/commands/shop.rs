use rand::Rng;

use poise::serenity_prelude as serenity;
use serenity::{
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, CreateMessage,
};

use crate::commands::moderation::err;
use crate::lang::lang;
use crate::{Context, Error};

// ── store items ───────────────────────────────────────────────────────────────

struct ShopItem {
    id:    &'static str,
    emoji: &'static str,
    price: i64,
}

static SHOP_ITEMS: &[ShopItem] = &[
    ShopItem { id: "xp_booster",      emoji: "🚀", price: 500  },
    ShopItem { id: "angelkoder",       emoji: "🪱", price: 300  },
    ShopItem { id: "diebstahlschutz",  emoji: "🔒", price: 400  },
    ShopItem { id: "doppelgehalt",     emoji: "💰", price: 600  },
    ShopItem { id: "lotto_rabatt",     emoji: "🎟️", price: 150  },
];

// ── /laden ────────────────────────────────────────────────────────────────────

/// Den Laden anzeigen und Items kaufen
#[poise::command(slash_command, guild_only)]
pub async fn laden(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let balance  = crate::db::get_coins(&ctx.data().db, guild_id, user_id).await;

    let mut lines = Vec::new();
    for item in SHOP_ITEMS {
        let name = lang().shop_item_name(item.id);
        let desc = lang().shop_item_desc(item.id);
        let owned = crate::db::get_shop_item_qty(&ctx.data().db, guild_id, user_id, item.id).await;
        let owned_str = if owned > 0 {
            lang().shop_owned_qty.replace("{n}", &owned.to_string())
        } else {
            String::new()
        };
        lines.push(format!(
            "{} **{}**: {} Coins{}\n> {}",
            item.emoji, name, item.price, owned_str, desc
        ));
    }

    let embed = CreateEmbed::new()
        .title(lang().shop_title)
        .description(lines.join("\n\n"))
        .color(0xF1C40Fu32)
        .footer(CreateEmbedFooter::new(
            lang().shop_footer.replace("{balance}", &balance.to_string())
        ));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

// ── /kaufen ───────────────────────────────────────────────────────────────────

#[derive(Debug, poise::ChoiceParameter)]
pub enum ShopChoice {
    #[name = "🚀 XP-Booster (500)"]
    XpBooster,
    #[name = "🪱 Angelköder (300)"]
    Angelkoder,
    #[name = "🔒 Diebstahlschutz (400)"]
    Diebstahlschutz,
    #[name = "💰 Doppelgehalt (600)"]
    Doppelgehalt,
    #[name = "🎟️ Lotto-Rabatt (150)"]
    LottoRabatt,
}

/// Ein Item im Laden kaufen
#[poise::command(slash_command, guild_only)]
pub async fn kaufen(
    ctx: Context<'_>,
    #[description = "Welches Item möchtest du kaufen?"] item: ShopChoice,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user     = ctx.author();

    let (item_id, price) = match item {
        ShopChoice::XpBooster       => ("xp_booster",      500i64),
        ShopChoice::Angelkoder      => ("angelkoder",       300i64),
        ShopChoice::Diebstahlschutz => ("diebstahlschutz",  400i64),
        ShopChoice::Doppelgehalt    => ("doppelgehalt",      600i64),
        ShopChoice::LottoRabatt     => ("lotto_rabatt",      150i64),
    };

    let shop_item = SHOP_ITEMS.iter().find(|i| i.id == item_id).unwrap();
    let item_name = lang().shop_item_name(item_id);
    let item_desc = lang().shop_item_desc(item_id);

    let balance = crate::db::get_coins(&ctx.data().db, guild_id, user.id).await;
    if balance < price {
        ctx.send(poise::CreateReply::default().embed(
            err(lang().shop_not_enough_label,
                &lang().shop_not_enough_desc
                    .replace("{have}", &balance.to_string())
                    .replace("{need}", &price.to_string())),
        )).await?;
        return Ok(());
    }

    crate::db::add_coins(&ctx.data().db, guild_id, user.id, -price).await;

    let now = chrono::Utc::now().timestamp();
    let expires_at = if item_id == "xp_booster" { now + 3600 } else { 0 };
    let qty = if item_id == "angelkoder" { 5i64 } else { 1i64 };

    crate::db::add_shop_item(&ctx.data().db, guild_id, user.id, item_id, qty, expires_at).await;

    let new_balance = crate::db::get_coins(&ctx.data().db, guild_id, user.id).await;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .author(serenity::CreateEmbedAuthor::new(user.tag()).icon_url(user.face()))
            .title(
                lang().shop_bought_title
                    .replace("{emoji}", shop_item.emoji)
                    .replace("{name}", item_name)
            )
            .description(item_desc)
            .color(0x57F287u32)
            .field(lang().shop_paid_field,        format!("-{} Coins", price),           true)
            .field(lang().shop_new_balance_field, format!("**{} Coins**", new_balance), true),
    )).await?;

    Ok(())
}

// ── /prestige ─────────────────────────────────────────────────────────────────

/// Bei Level 50 deinen Fortschritt zurücksetzen und eine Prestige-Marke erhalten
#[poise::command(slash_command, guild_only)]
pub async fn prestige(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user     = ctx.author();

    let total_xp = crate::db::get_xp(&ctx.data().db, guild_id, user.id).await;
    let level    = crate::xp::level_from_xp(total_xp);

    if level < 50 {
        ctx.send(poise::CreateReply::default().embed(
            err(lang().prestige_not_ready_label,
                &lang().prestige_not_ready_desc.replace("{level}", &level.to_string())),
        )).await?;
        return Ok(());
    }

    let current_prestige = crate::db::get_prestige(&ctx.data().db, guild_id, user.id).await;

    crate::db::reset_xp_to_zero(&ctx.data().db, guild_id, user.id).await;
    crate::db::increment_prestige(&ctx.data().db, guild_id, user.id).await;
    crate::db::set_credited_level(&ctx.data().db, guild_id, user.id, 0).await;

    let new_prestige = current_prestige + 1;
    let stars = "⭐".repeat(new_prestige as usize);

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .author(serenity::CreateEmbedAuthor::new(user.tag()).icon_url(user.face()))
            .title(lang().prestige_title)
            .description(
                lang().prestige_desc
                    .replace("{user}", &user.id.to_string())
                    .replace("{prestige}", &new_prestige.to_string())
                    .replace("{stars}", &stars)
            )
            .color(0xEB459Eu32)
            .field(lang().prestige_rank_field,     format!("**{}**", new_prestige), true)
            .field(lang().prestige_xp_reset_field, lang().prestige_xp_reset_value, true),
    )).await?;

    Ok(())
}

// ── daily salary background task ──────────────────────────────────────────────

pub fn schedule_salary(ctx: serenity::Context, pool: sqlx::SqlitePool) {
    tokio::spawn(async move {
        loop {
            let now      = chrono::Utc::now();
            let tomorrow = (now + chrono::Duration::days(1))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let next_midnight = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(tomorrow, chrono::Utc);
            let secs_until = (next_midnight - now).num_seconds().max(0) as u64;

            tokio::time::sleep(std::time::Duration::from_secs(secs_until)).await;

            run_salary(&ctx, &pool).await;
        }
    });
}

async fn run_salary(ctx: &serenity::Context, pool: &sqlx::SqlitePool) {
    let guilds = crate::db::get_guilds_with_bot_channel(pool).await;
    for (guild_id, bot_ch) in guilds {
        let users = crate::db::get_guild_xp_users(pool, guild_id).await;
        let mut total_paid: i64 = 0;
        let mut count: usize = 0;

        for (user_id, total_xp) in users {
            let level = crate::xp::level_from_xp(total_xp);
            if level == 0 { continue; }

            let base_salary = (level as i64) * 100;
            let doubled = crate::db::has_active_shop_item(pool, guild_id, user_id, "doppelgehalt").await;
            let salary   = if doubled { base_salary * 2 } else { base_salary };

            if doubled {
                crate::db::consume_shop_item(pool, guild_id, user_id, "doppelgehalt").await;
            }

            crate::db::add_coins(pool, guild_id, user_id, salary).await;
            total_paid += salary;
            count += 1;
        }

        if count > 0 {
            let _ = bot_ch.send_message(ctx, CreateMessage::new().embed(
                CreateEmbed::new()
                    .title(lang().salary_title)
                    .description(
                        lang().salary_desc
                            .replace("{count}", &count.to_string())
                            .replace("{total}", &total_paid.to_string())
                    )
                    .color(0xF1C40Fu32)
                    .footer(CreateEmbedFooter::new(lang().salary_footer)),
            )).await;
        }
    }
}

// ── loot drop background task ─────────────────────────────────────────────────

pub fn schedule_loot_drops(
    ctx:  serenity::Context,
    pool: sqlx::SqlitePool,
) {
    tokio::spawn(async move {
        loop {
            spawn_loot_drop(&ctx, &pool).await;
            tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
        }
    });
}

/// Called at startup to delete expired drops and re-arm timers for pending ones.
pub async fn restore_loot_drops(ctx: serenity::Context, pool: sqlx::SqlitePool) {
    let now = chrono::Utc::now().timestamp();
    let pending = crate::db::get_pending_loot_drops(&pool).await;
    for drop in pending {
        if drop.expires_at <= now {
            // already expired: delete the message so it doesn't pile up
            let _ = drop.channel_id.delete_message(&ctx, drop.message_id).await;
            crate::db::delete_loot_drop_row(&pool, drop.id).await;
        } else {
            // still live: spawn a cleanup timer for the remaining time
            let remaining = (drop.expires_at - now) as u64;
            let ctx_c  = ctx.clone();
            let pool_c = pool.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(remaining)).await;
                expire_loot_drop(&ctx_c, &pool_c, drop.id, drop.channel_id, drop.message_id).await;
            });
        }
    }
    // If any expired drops were cleaned up (or no live drop exists), post one now.
    spawn_loot_drop(&ctx, &pool).await;
}

async fn expire_loot_drop(
    ctx:        &serenity::Context,
    pool:       &sqlx::SqlitePool,
    drop_id:    i64,
    channel_id: serenity::ChannelId,
    message_id: serenity::MessageId,
) {
    let won = crate::db::claim_loot_drop(pool, drop_id).await;
    if won {
        // Nobody claimed it: delete so expired drops don't pile up in the channel.
        let _ = channel_id.delete_message(ctx, message_id).await;
    }
    crate::db::delete_loot_drop_row(pool, drop_id).await;
}

async fn spawn_loot_drop(
    ctx:  &serenity::Context,
    pool: &sqlx::SqlitePool,
) {
    let guilds = crate::db::get_guilds_with_bot_channel(pool).await;
    for (guild_id, bot_ch) in guilds {
        if crate::db::has_active_loot_drop(pool, guild_id).await {
            continue;
        }

        let drop_token = format!("loot_{}_{}", guild_id, chrono::Utc::now().timestamp());

        let (tier_key, color, fish_id, coin_min, coin_max, bonus_xp) = {
            let mut rng = rand::thread_rng();
            let roll: u8 = rng.gen_range(0..100);
            if roll < 60 {
                let fish = if rng.gen_bool(0.5) { "hering" } else { "forelle" };
                ("common", 0x99AAB5u32, fish, 50i64, 100i64, 0i64)
            } else if roll < 90 {
                let fish = if rng.gen_bool(0.5) { "barsch" } else { "hecht" };
                ("rare", 0x5865F2u32, fish, 100i64, 200i64, 0i64)
            } else {
                let fish = if rng.gen_bool(0.5) { "goldfisch" } else { "quantenbarsch" };
                ("legendary", 0xFEE75Cu32, fish, 300i64, 500i64, 50i64)
            }
        };

        let tier_name = match tier_key {
            "common"    => lang().loot_tier_common,
            "rare"      => lang().loot_tier_rare,
            _           => lang().loot_tier_legendary,
        };

        let coins: i64 = {
            let mut rng = rand::thread_rng();
            rng.gen_range(coin_min..=coin_max)
        };

        let fish_kind = crate::commands::fishing::find_fish(fish_id);
        let fish_display = fish_kind.map(|f| format!("{} {}", f.emoji, lang().fish_display_name(f.id))).unwrap_or_default();

        let mut desc = lang().loot_desc
            .replace("{fish}", &fish_display)
            .replace("{coins}", &coins.to_string());
        if bonus_xp > 0 {
            desc.push_str(&lang().loot_desc_xp.replace("{xp}", &bonus_xp.to_string()));
        }

        let claim_btn = CreateButton::new(format!("loot_claim_{}", drop_token))
            .label(lang().loot_claim_btn)
            .style(serenity::ButtonStyle::Success);

        let msg = bot_ch.send_message(ctx, CreateMessage::new()
            .embed(
                CreateEmbed::new()
                    .title(format!("📦 Loot-Drop: {}", tier_name))
                    .description(&desc)
                    .color(color)
                    .footer(CreateEmbedFooter::new(lang().loot_footer)),
            )
            .components(vec![CreateActionRow::Buttons(vec![claim_btn])])
        ).await;

        if let Ok(msg) = msg {
            let expires_at = chrono::Utc::now().timestamp() + 1800;
            let drop_id = crate::db::insert_loot_drop(
                pool, guild_id, bot_ch, msg.id, expires_at, fish_id, coins, bonus_xp,
            ).await;
            tracing::info!(
                "loot drop spawned: guild={} channel={} message={} drop_id={}",
                guild_id, bot_ch, msg.id, drop_id
            );

            let ctx_c  = ctx.clone();
            let pool_c = pool.clone();
            let msg_id = msg.id;
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
                expire_loot_drop(&ctx_c, &pool_c, drop_id, bot_ch, msg_id).await;
            });
        }
    }
}

/// Called from events.rs when loot_claim_ button is pressed.
pub async fn handle_loot_claim(
    ctx:  &serenity::Context,
    pool: &sqlx::SqlitePool,
    comp: &serenity::ComponentInteraction,
) {
    use serenity::{
        CreateInteractionResponse, CreateInteractionResponseMessage,
    };

    let guild_id = match comp.guild_id {
        Some(g) => g,
        None => {
            let _ = comp.create_response(ctx, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(lang().loot_no_guild)
                    .ephemeral(true),
            )).await;
            return;
        }
    };

    let msg_id  = comp.message.id;
    let channel = comp.channel_id;
    let claimer = &comp.user;

    let drop = match crate::db::get_loot_drop_by_message(pool, channel, msg_id).await {
        Some(d) => d,
        None => {
            tracing::warn!(
                "loot_claim: no unclaimed drop found for channel={} message={} (user={})",
                channel, msg_id, claimer.id
            );
            let _ = comp.create_response(ctx, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(lang().loot_already_claimed)
                    .ephemeral(true),
            )).await;
            return;
        }
    };

    if !crate::db::claim_loot_drop(pool, drop.id).await {
        let _ = comp.create_response(ctx, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(lang().loot_already_claimed)
                .ephemeral(true),
        )).await;
        return;
    }

    let fish_id  = drop.fish_id.as_str();
    let coins    = drop.coins;
    let bonus_xp = drop.bonus_xp as u64;

    let now = chrono::Utc::now().timestamp();
    crate::db::add_fish_to_inventory(pool, guild_id, claimer.id, fish_id, now).await;
    crate::db::add_coins(pool, guild_id, claimer.id, coins).await;

    if bonus_xp > 0 {
        let old_xp  = crate::db::get_xp(pool, guild_id, claimer.id).await;
        let new_xp  = crate::db::add_xp(pool, guild_id, claimer.id, bonus_xp).await;
        let old_lvl = crate::xp::level_from_xp(old_xp);
        let new_lvl = crate::xp::level_from_xp(new_xp);
        if new_lvl > old_lvl && old_lvl < 50 {
            let reward = (new_lvl * 100) as i64;
            crate::db::add_coins(pool, guild_id, claimer.id, reward).await;
            crate::db::set_credited_level(pool, guild_id, claimer.id, new_lvl).await;
            let _ = channel.send_message(
                ctx,
                crate::commands::levels::level_up_embed(claimer.id, new_lvl),
            ).await;
        }
    }

    crate::db::delete_loot_drop_row(pool, drop.id).await;

    let fish_kind = crate::commands::fishing::find_fish(fish_id);
    let fish_display = fish_kind.map(|f| format!("{} {}", f.emoji, lang().fish_display_name(f.id))).unwrap_or_default();

    let mut claimed_desc = lang().loot_claimed_desc
        .replace("{user}", &claimer.id.to_string())
        .replace("{fish}", &fish_display)
        .replace("{coins}", &coins.to_string());
    if bonus_xp > 0 {
        claimed_desc.push_str(&lang().loot_claimed_desc_xp.replace("{xp}", &bonus_xp.to_string()));
    }

    let _ = comp.create_response(ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .embed(
                CreateEmbed::new()
                    .title(lang().loot_claimed_title)
                    .description(claimed_desc)
                    .color(0x57F287u32),
            )
            .components(vec![])
    )).await;

    let ctx_del = ctx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        let _ = channel.delete_message(&ctx_del, msg_id).await;
    });
}
