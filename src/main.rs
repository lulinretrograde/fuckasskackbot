mod antinuke;
mod commands;
mod config;
mod db;
mod events;
mod xp;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use poise::serenity_prelude as serenity;
use tokio::sync::Mutex;

use config::{
    AwaitingTicketReply, BugCooldowns, InviteCache, JoinTracker, LockdownState, LogConfigs,
    MessageCache, NukeCounters, RaidCounters, VoiceSessions, XpCooldowns,
};

// ── types ────────────────────────────────────────────────────────────────────

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, AppData, Error>;

// ── shared state ─────────────────────────────────────────────────────────────

pub struct AppData {
    pub db:                sqlx::SqlitePool,
    pub log_configs:       LogConfigs,        // in-memory cache, DB-backed
    pub join_tracker:      JoinTracker,       // ephemeral
    pub message_cache:     MessageCache,      // ephemeral
    pub xp_cooldowns:      XpCooldowns,       // ephemeral
    pub invite_cache:      InviteCache,       // ephemeral, rebuilt on GuildCreate
    pub voice_sessions:    VoiceSessions,     // ephemeral, VC join tracking
    pub nuke_counters:     NukeCounters,      // anti-nuke action counters
    pub raid_counters:     RaidCounters,      // anti-nuke raid join counters
    pub lockdown_state:    LockdownState,     // guilds currently in lockdown
    pub bug_cooldowns:          BugCooldowns,          // ephemeral, /bug rate limiting
    pub awaiting_ticket_reply:  AwaitingTicketReply,   // ephemeral, ticket DM reply state
}

// ── entry point ───────────────────────────────────────────────────────────────

/// Binds a TCP socket on loopback as an exclusive instance lock.
/// If another instance is already running (port taken), waits until it exits.
/// The returned listener must stay alive for the entire process lifetime:
/// dropping it releases the lock and lets the next instance proceed.
fn acquire_instance_lock() -> std::net::TcpListener {
    loop {
        match std::net::TcpListener::bind("127.0.0.1:27016") {
            Ok(l) => return l,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(500)),
        }
    }
}

#[tokio::main]
async fn main() {
    // Must be held alive until process exit: drop = lock released.
    let _instance_lock = acquire_instance_lock();

    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let token = std::env::var("BOT_TOKEN").expect("BOT_TOKEN not set");
    let guild_id: u64 = 1405892779782967337;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::help(),
                commands::level(),
                commands::leaderboard(),
                commands::scan_xp(),
                commands::reset_xp(),
                commands::ban(),
                commands::unban(),
                commands::kick(),
                commands::mute(),
                commands::unmute(),
                commands::warn(),
                commands::warnings(),
                commands::clearwarnings(),
                commands::purge(),
                commands::jail(),
                commands::unjail(),
                commands::setup_logs(),
                commands::setup_jail(),
                commands::baserole(),
                commands::welcome_channel(),
                commands::arbeit(),
                commands::klauen(),
                commands::bankueberfall(),
                commands::coins(),
                commands::coins_leaderboard(),
                commands::giveaway(),
                commands::slots(),
                commands::blackjack(),
                commands::wuerfeln(),
                commands::muenzwurf(),
                commands::roulette(),
                commands::kartenspiel(),
                commands::lotto(),
                commands::casino_stats(),
                commands::casino_rangliste(),
                commands::casino_setup(),
                commands::casino_tresor(),
                commands::casino_limit(),
                commands::casino_jackpot(),
                commands::bot_channel(),
                commands::stealemoji(),
                commands::stealsticker(),
                commands::angeln(),
                commands::inventar(),
                commands::alles_verkaufen(),
                commands::fischmarkt(),
                commands::angelshop(),
                commands::rute_kaufen(),
                commands::laden(),
                commands::kaufen(),
                commands::prestige(),
                commands::ueberweisung(),
                commands::level_coins_migrate(),
                commands::antinuke(),
                commands::bug(),
                commands::ticket_reward(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(events::handle(ctx, event, framework, data))
            },
            command_check: Some(|ctx| Box::pin(async move {
                // Only economy and fun commands are restricted to the bot channel
                const RESTRICTED: &[&str] = &[
                    "arbeit", "klauen", "bankueberfall", "coins", "coins-leaderboard",
                    "slots", "blackjack", "wuerfeln", "muenzwurf", "roulette",
                    "kartenspiel", "lotto", "casino-stats", "casino-rangliste",
                    "angeln", "inventar", "alles-verkaufen", "fischmarkt", "angelshop", "rute-kaufen",
                    "level", "leaderboard",
                    "laden", "kaufen", "prestige", "ueberweisung",
                ];

                if !RESTRICTED.contains(&ctx.command().name.as_str()) {
                    return Ok(true);
                }

                let Some(guild_id) = ctx.guild_id() else { return Ok(true); };

                let bot_ch = crate::db::get_bot_channel(&ctx.data().db, guild_id).await;
                let Some(bot_ch) = bot_ch else { return Ok(true); };

                if ctx.channel_id() == bot_ch {
                    return Ok(true);
                }

                // Wrong channel: send ephemeral notice and block
                let _ = ctx.send(
                    poise::CreateReply::default()
                        .content(format!(
                            "❌ Dieser Befehl ist nur im Bot-Kanal <#{}> verfügbar.",
                            bot_ch
                        ))
                        .ephemeral(true),
                ).await;

                Ok(false)
            })),
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                tracing::info!("Logged in as {}", _ready.user.name);

                ctx.set_activity(Some(serenity::ActivityData::watching("Bot kaputt? -> /bug")));

                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    serenity::GuildId::new(guild_id),
                )
                .await?;

                tracing::info!("Commands registered in guild");

                let pool = db::init().await;
                tracing::info!("Datenbank initialisiert");

                // Schedule daily lotto drawing at midnight UTC
                commands::casino::schedule_lotto(ctx.clone(), pool.clone());

                // Seed fish market prices on first run, then refresh hourly
                commands::fishing::refresh_market_prices(&pool).await;
                {
                    let pool_bg = pool.clone();
                    tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                            commands::fishing::refresh_market_prices(&pool_bg).await;
                            tracing::info!("Fischmarktpreise aktualisiert");
                        }
                    });
                }

                // Daily salary at midnight UTC
                commands::shop::schedule_salary(ctx.clone(), pool.clone());

                // Restore any loot drops that survived a restart, then schedule new ones
                commands::shop::restore_loot_drops(ctx.clone(), pool.clone()).await;
                commands::shop::schedule_loot_drops(ctx.clone(), pool.clone());

                // Voice XP: award 10 XP per user every 5 minutes in VC
                let voice_sessions: VoiceSessions = Arc::new(Mutex::new(HashMap::new()));
                {
                    let vs  = voice_sessions.clone();
                    let ctx_bg = ctx.clone();
                    let pool_bg = pool.clone();
                    tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await;

                            let entries: Vec<_> = {
                                vs.lock().await.keys().cloned().collect()
                            };

                            for (guild_id, user_id) in entries {

                                let has_booster = crate::db::has_active_shop_item(
                                    &pool_bg, guild_id, user_id, "xp_booster",
                                ).await;
                                let xp_gain: u64 = if has_booster { 20 } else { 10 };

                                let old_xp  = crate::db::get_xp(&pool_bg, guild_id, user_id).await;
                                let new_xp  = crate::db::add_xp(&pool_bg, guild_id, user_id, xp_gain).await;
                                let old_lvl = crate::xp::level_from_xp(old_xp);
                                let new_lvl = crate::xp::level_from_xp(new_xp);

                                if new_lvl > old_lvl && old_lvl < 50 {
                                    let reward = (new_lvl * 100) as i64;
                                    crate::db::add_coins(&pool_bg, guild_id, user_id, reward).await;
                                    crate::db::set_credited_level(&pool_bg, guild_id, user_id, new_lvl).await;

                                    let bot_ch = crate::db::get_bot_channel(&pool_bg, guild_id).await;
                                    if let Some(ch) = bot_ch {
                                        let _ = ch.send_message(
                                            &ctx_bg,
                                            crate::commands::levels::level_up_embed(user_id, new_lvl),
                                        ).await;
                                    }
                                }
                            }
                        }
                    });
                }

                // Load log configs from DB into memory
                let config_data: HashMap<_, _> =
                    db::get_all_log_configs(&pool).await.into_iter().collect();
                tracing::info!("Log-Konfigurationen geladen: {} Server", config_data.len());

                Ok(AppData {
                    db: pool,
                    log_configs:        Arc::new(Mutex::new(config_data)),
                    join_tracker:       Arc::new(Mutex::new(HashMap::new())),
                    message_cache:      Arc::new(Mutex::new((HashMap::new(), VecDeque::new()))),
                    xp_cooldowns:       Arc::new(Mutex::new(HashMap::new())),
                    invite_cache:       Arc::new(Mutex::new(HashMap::new())),
                    voice_sessions,
                    nuke_counters:      Arc::new(Mutex::new(HashMap::new())),
                    raid_counters:      Arc::new(Mutex::new(HashMap::new())),
                    lockdown_state:     Arc::new(Mutex::new(HashMap::new())),
                    bug_cooldowns:          Arc::new(Mutex::new(HashMap::new())),
                    awaiting_ticket_reply:  Arc::new(Mutex::new(HashMap::new())),
                })
            })
        })
        .build();

    let intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::GUILD_MODERATION
        | serenity::GatewayIntents::GUILD_EMOJIS_AND_STICKERS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::GUILD_MESSAGE_REACTIONS
        | serenity::GatewayIntents::GUILD_VOICE_STATES
        | serenity::GatewayIntents::GUILD_SCHEDULED_EVENTS
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let mut client = serenity::ClientBuilder::new(&token, intents)
        .framework(framework)
        .await
        .expect("Failed to create client");

    tracing::info!("Starting idf-soldat...");
    client.start().await.expect("Client error");
}
