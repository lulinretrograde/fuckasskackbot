use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{ChannelId, GuildId, MessageId, RoleId, UserId};
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::str::FromStr;

use crate::config::LogConfig;

// ── init ──────────────────────────────────────────────────────────────────────

pub async fn init() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite:idf-soldat.db")
        .expect("Ungültiger Datenbankpfad")
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options)
        .await
        .expect("Datenbankverbindung fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS log_configs (
            guild_id     INTEGER PRIMARY KEY,
            voice        INTEGER,
            messages     INTEGER,
            join_leave   INTEGER,
            server       INTEGER,
            members      INTEGER,
            welcome      INTEGER,
            mod_log      INTEGER,
            bot_log      INTEGER,
            jail_role    INTEGER,
            jail_channel INTEGER,
            base_role    INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("log_configs Tabelle erstellen fehlgeschlagen");

    // Migrate existing tables: add new columns if missing (errors are ignored if column exists)
    let _ = sqlx::query("ALTER TABLE log_configs ADD COLUMN mod_log INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE log_configs ADD COLUMN bot_log INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE log_configs ADD COLUMN base_role INTEGER").execute(&pool).await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS xp (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            xp       INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("xp Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS warnings (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id     INTEGER NOT NULL,
            user_id      INTEGER NOT NULL,
            moderator_id INTEGER NOT NULL,
            reason       TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("warnings Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS jailed_users (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            roles    TEXT NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("jailed_users Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS economy (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            coins    INTEGER NOT NULL DEFAULT 0,
            invites  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("economy Tabelle erstellen fehlgeschlagen");

    let _ = sqlx::query("ALTER TABLE economy ADD COLUMN invites INTEGER NOT NULL DEFAULT 0")
        .execute(&pool).await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS arbeit_cooldowns (
            guild_id  INTEGER NOT NULL,
            user_id   INTEGER NOT NULL,
            last_used INTEGER NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("arbeit_cooldowns Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS klauen_cooldowns (
            guild_id  INTEGER NOT NULL,
            user_id   INTEGER NOT NULL,
            last_used INTEGER NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("klauen_cooldowns Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bankraub_cooldowns (
            guild_id  INTEGER NOT NULL,
            user_id   INTEGER NOT NULL,
            last_used INTEGER NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("bankraub_cooldowns Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bank (
            guild_id INTEGER PRIMARY KEY,
            coins    INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await
    .expect("bank Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS robbery_jail (
            guild_id   INTEGER NOT NULL,
            user_id    INTEGER NOT NULL,
            jail_until INTEGER NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("robbery_jail Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS messages (
            message_id       INTEGER PRIMARY KEY,
            guild_id         INTEGER NOT NULL,
            channel_id       INTEGER NOT NULL,
            user_id          INTEGER NOT NULL,
            user_tag         TEXT NOT NULL DEFAULT '',
            content          TEXT NOT NULL DEFAULT '',
            attachment_names TEXT NOT NULL DEFAULT '[]'
        )",
    )
    .execute(&pool)
    .await
    .expect("messages Tabelle erstellen fehlgeschlagen");

    // ── giveaways ─────────────────────────────────────────────────────────────

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS giveaways (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id       INTEGER NOT NULL,
            channel_id     INTEGER NOT NULL,
            message_id     INTEGER,
            prize          TEXT    NOT NULL,
            ticket_price   INTEGER NOT NULL DEFAULT 0,
            required_level INTEGER NOT NULL DEFAULT 0,
            ends_at        INTEGER NOT NULL,
            ended          INTEGER NOT NULL DEFAULT 0,
            winner_id      INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("giveaways Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS giveaway_entries (
            giveaway_id INTEGER NOT NULL,
            user_id     INTEGER NOT NULL,
            PRIMARY KEY (giveaway_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("giveaway_entries Tabelle erstellen fehlgeschlagen");

    // ── casino ───────────────────────────────────────────────────────────────

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS casino_stats (
            guild_id      INTEGER NOT NULL,
            user_id       INTEGER NOT NULL,
            total_wagered INTEGER NOT NULL DEFAULT 0,
            total_won     INTEGER NOT NULL DEFAULT 0,
            total_lost    INTEGER NOT NULL DEFAULT 0,
            biggest_win   INTEGER NOT NULL DEFAULT 0,
            win_streak    INTEGER NOT NULL DEFAULT 0,
            lose_streak   INTEGER NOT NULL DEFAULT 0,
            games_played  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("casino_stats Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS casino_vault (
            guild_id INTEGER PRIMARY KEY,
            balance  INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await
    .expect("casino_vault Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS guild_settings (
            guild_id   INTEGER PRIMARY KEY,
            bot_channel INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("guild_settings Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS casino_config (
            guild_id    INTEGER PRIMARY KEY,
            channel_id  INTEGER,
            daily_limit INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await
    .expect("casino_config Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS casino_daily (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            day      TEXT    NOT NULL,
            lost     INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("casino_daily Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lotto_drawings (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id   INTEGER NOT NULL,
            jackpot    INTEGER NOT NULL DEFAULT 0,
            numbers    TEXT,
            drawn_at   INTEGER,
            channel_id INTEGER
        )",
    )
    .execute(&pool)
    .await
    .expect("lotto_drawings Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lotto_tickets (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            drawing_id INTEGER NOT NULL,
            guild_id   INTEGER NOT NULL,
            user_id    INTEGER NOT NULL,
            numbers    TEXT    NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("lotto_tickets Tabelle erstellen fehlgeschlagen");

    // ── fishing ───────────────────────────────────────────────────────────────

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fishing_inventory (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id   INTEGER NOT NULL,
            user_id    INTEGER NOT NULL,
            fish_id    TEXT    NOT NULL,
            caught_at  INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("fishing_inventory Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fishing_prices (
            fish_id    TEXT    PRIMARY KEY,
            price      INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("fishing_prices Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fishing_rods (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            rod_id   TEXT    NOT NULL DEFAULT 'grundangel',
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("fishing_rods Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fishing_cooldowns (
            guild_id  INTEGER NOT NULL,
            user_id   INTEGER NOT NULL,
            last_used INTEGER NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("fishing_cooldowns Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS shop_purchases (
            guild_id   INTEGER NOT NULL,
            user_id    INTEGER NOT NULL,
            item_id    TEXT    NOT NULL,
            quantity   INTEGER NOT NULL DEFAULT 0,
            expires_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id, item_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("shop_purchases Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS prestige (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            count    INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("prestige Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS level_coins_credited (
            guild_id       INTEGER NOT NULL,
            user_id        INTEGER NOT NULL,
            credited_level INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("level_coins_credited Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS antinuke_config (
            guild_id             INTEGER PRIMARY KEY,
            enabled              INTEGER NOT NULL DEFAULT 1,
            chan_del_max         INTEGER NOT NULL DEFAULT 3,
            chan_cre_max         INTEGER NOT NULL DEFAULT 5,
            role_del_max         INTEGER NOT NULL DEFAULT 3,
            role_cre_max         INTEGER NOT NULL DEFAULT 5,
            ban_max              INTEGER NOT NULL DEFAULT 5,
            webhook_max          INTEGER NOT NULL DEFAULT 5,
            window_secs          INTEGER NOT NULL DEFAULT 10,
            raid_joins           INTEGER NOT NULL DEFAULT 10,
            raid_window          INTEGER NOT NULL DEFAULT 10,
            min_account_age_days INTEGER NOT NULL DEFAULT 0,
            lockdown_mins        INTEGER NOT NULL DEFAULT 15,
            punishment           TEXT    NOT NULL DEFAULT 'ban'
        )",
    )
    .execute(&pool)
    .await
    .expect("antinuke_config Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS antinuke_whitelist (
            guild_id INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            PRIMARY KEY (guild_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .expect("antinuke_whitelist Tabelle erstellen fehlgeschlagen");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            guild_id   INTEGER NOT NULL,
            channel_id INTEGER NOT NULL,
            message_id INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            claimed    INTEGER NOT NULL DEFAULT 0,
            fish_id    TEXT NOT NULL DEFAULT '',
            coins      INTEGER NOT NULL DEFAULT 0,
            bonus_xp   INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await
    .expect("loot_drops Tabelle erstellen fehlgeschlagen");

    pool
}

// ── loot drops ────────────────────────────────────────────────────────────────

pub async fn insert_loot_drop(
    pool:       &SqlitePool,
    guild_id:   GuildId,
    channel_id: ChannelId,
    message_id: MessageId,
    expires_at: i64,
    fish_id:    &str,
    coins:      i64,
    bonus_xp:   i64,
) -> i64 {
    sqlx::query(
        "INSERT INTO loot_drops (guild_id, channel_id, message_id, expires_at, fish_id, coins, bonus_xp)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(guild_id.get() as i64)
    .bind(channel_id.get() as i64)
    .bind(message_id.get() as i64)
    .bind(expires_at)
    .bind(fish_id)
    .bind(coins)
    .bind(bonus_xp)
    .execute(pool)
    .await
    .map(|r| r.last_insert_rowid())
    .unwrap_or(0)
}

/// Atomically marks a drop as claimed. Returns true if this caller won the race.
pub async fn claim_loot_drop(pool: &SqlitePool, drop_id: i64) -> bool {
    sqlx::query("UPDATE loot_drops SET claimed = 1 WHERE id = ? AND claimed = 0")
        .bind(drop_id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected() > 0)
        .unwrap_or(false)
}

pub async fn delete_loot_drop_row(pool: &SqlitePool, drop_id: i64) {
    let _ = sqlx::query("DELETE FROM loot_drops WHERE id = ?")
        .bind(drop_id)
        .execute(pool)
        .await;
}

pub struct LootDropRow {
    pub id:         i64,
    pub channel_id: ChannelId,
    pub message_id: MessageId,
    pub expires_at: i64,
    pub fish_id:    String,
    pub coins:      i64,
    pub bonus_xp:   i64,
}

/// Look up an unclaimed drop by channel + message ID.
pub async fn get_loot_drop_by_message(
    pool:       &SqlitePool,
    channel_id: ChannelId,
    message_id: MessageId,
) -> Option<LootDropRow> {
    let row = sqlx::query(
        "SELECT id, channel_id, message_id, expires_at, fish_id, coins, bonus_xp
         FROM loot_drops WHERE channel_id = ? AND message_id = ? AND claimed = 0",
    )
    .bind(channel_id.get() as i64)
    .bind(message_id.get() as i64)
    .fetch_optional(pool)
    .await
    .ok()??;

    Some(LootDropRow {
        id:         row.get("id"),
        channel_id: ChannelId::new(row.get::<i64, _>("channel_id") as u64),
        message_id: MessageId::new(row.get::<i64, _>("message_id") as u64),
        expires_at: row.get("expires_at"),
        fish_id:    row.get("fish_id"),
        coins:      row.get("coins"),
        bonus_xp:   row.get("bonus_xp"),
    })
}

/// Returns true if the guild already has an unclaimed (active) loot drop.
pub async fn has_active_loot_drop(pool: &SqlitePool, guild_id: GuildId) -> bool {
    sqlx::query("SELECT id FROM loot_drops WHERE guild_id = ? AND claimed = 0 LIMIT 1")
        .bind(guild_id.get() as i64)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some()
}

/// Returns all unclaimed loot drops (for startup recovery).
pub async fn get_pending_loot_drops(pool: &SqlitePool) -> Vec<LootDropRow> {
    let rows = sqlx::query(
        "SELECT id, channel_id, message_id, expires_at, fish_id, coins, bonus_xp
         FROM loot_drops WHERE claimed = 0",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| LootDropRow {
            id:         r.get("id"),
            channel_id: ChannelId::new(r.get::<i64, _>("channel_id") as u64),
            message_id: MessageId::new(r.get::<i64, _>("message_id") as u64),
            expires_at: r.get("expires_at"),
            fish_id:    r.get("fish_id"),
            coins:      r.get("coins"),
            bonus_xp:   r.get("bonus_xp"),
        })
        .collect()
}

// ── log configs ───────────────────────────────────────────────────────────────

pub async fn get_all_log_configs(pool: &SqlitePool) -> Vec<(GuildId, LogConfig)> {
    let rows = sqlx::query(
        "SELECT guild_id, voice, messages, join_leave, server, members, welcome, mod_log, bot_log, jail_role, jail_channel, base_role
         FROM log_configs",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|r| {
        let guild_id = GuildId::new(r.get::<i64, _>("guild_id") as u64);
        let config = row_to_log_config(&r);
        (guild_id, config)
    }).collect()
}

pub async fn save_log_config(pool: &SqlitePool, guild_id: GuildId, config: &LogConfig) {
    let r = sqlx::query(
        "INSERT INTO log_configs (guild_id, voice, messages, join_leave, server, members, welcome, mod_log, bot_log, jail_role, jail_channel, base_role)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET
             voice        = excluded.voice,
             messages     = excluded.messages,
             join_leave   = excluded.join_leave,
             server       = excluded.server,
             members      = excluded.members,
             welcome      = excluded.welcome,
             mod_log      = excluded.mod_log,
             bot_log      = excluded.bot_log,
             jail_role    = excluded.jail_role,
             jail_channel = excluded.jail_channel,
             base_role    = excluded.base_role",
    )
    .bind(guild_id.get() as i64)
    .bind(config.voice.map(|c| c.get() as i64))
    .bind(config.messages.map(|c| c.get() as i64))
    .bind(config.join_leave.map(|c| c.get() as i64))
    .bind(config.server.map(|c| c.get() as i64))
    .bind(config.members.map(|c| c.get() as i64))
    .bind(config.welcome.map(|c| c.get() as i64))
    .bind(config.mod_log.map(|c| c.get() as i64))
    .bind(config.bot_log.map(|c| c.get() as i64))
    .bind(config.jail_role.map(|r| r.get() as i64))
    .bind(config.jail_channel.map(|c| c.get() as i64))
    .bind(config.base_role.map(|r| r.get() as i64))
    .execute(pool)
    .await;

    if let Err(e) = r {
        tracing::error!("Log-Konfiguration speichern fehlgeschlagen: {e}");
    }
}

fn row_to_log_config(r: &sqlx::sqlite::SqliteRow) -> LogConfig {
    LogConfig {
        voice:        r.get::<Option<i64>, _>("voice").map(|v| ChannelId::new(v as u64)),
        messages:     r.get::<Option<i64>, _>("messages").map(|v| ChannelId::new(v as u64)),
        join_leave:   r.get::<Option<i64>, _>("join_leave").map(|v| ChannelId::new(v as u64)),
        server:       r.get::<Option<i64>, _>("server").map(|v| ChannelId::new(v as u64)),
        members:      r.get::<Option<i64>, _>("members").map(|v| ChannelId::new(v as u64)),
        welcome:      r.get::<Option<i64>, _>("welcome").map(|v| ChannelId::new(v as u64)),
        mod_log:      r.get::<Option<i64>, _>("mod_log").map(|v| ChannelId::new(v as u64)),
        bot_log:      r.get::<Option<i64>, _>("bot_log").map(|v| ChannelId::new(v as u64)),
        jail_role:    r.get::<Option<i64>, _>("jail_role").map(|v| RoleId::new(v as u64)),
        jail_channel: r.get::<Option<i64>, _>("jail_channel").map(|v| ChannelId::new(v as u64)),
        base_role:    r.get::<Option<i64>, _>("base_role").map(|v| RoleId::new(v as u64)),
    }
}

pub async fn get_jailed_user_ids(pool: &SqlitePool, guild_id: GuildId) -> Vec<UserId> {
    sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM jailed_users WHERE guild_id = ?",
    )
    .bind(guild_id.get() as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|v| UserId::new(v as u64))
    .collect()
}

// ── xp ────────────────────────────────────────────────────────────────────────

pub async fn get_xp(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> u64 {
    sqlx::query("SELECT xp FROM xp WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .map(|r| r.get::<i64, _>("xp") as u64)
        .unwrap_or(0)
}

/// Adds XP and returns the new total.
pub async fn add_xp(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, amount: u64) -> u64 {
    let r = sqlx::query(
        "INSERT INTO xp (guild_id, user_id, xp) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET xp = xp + excluded.xp",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(amount as i64)
    .execute(pool)
    .await;

    if let Err(e) = r {
        tracing::error!("XP aktualisieren fehlgeschlagen: {e}");
        return 0;
    }

    get_xp(pool, guild_id, user_id).await
}

/// Returns 1-based rank position (users with more XP counted above).
pub async fn get_xp_rank(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> u64 {
    let current = get_xp(pool, guild_id, user_id).await;
    let row = sqlx::query(
        "SELECT COUNT(*) as cnt FROM xp WHERE guild_id = ? AND xp > ?",
    )
    .bind(guild_id.get() as i64)
    .bind(current as i64)
    .fetch_one(pool)
    .await;

    match row {
        Ok(r) => r.get::<i64, _>("cnt") as u64 + 1,
        Err(_) => 1,
    }
}

pub async fn get_guild_leaderboard(pool: &SqlitePool, guild_id: GuildId, limit: i64) -> Vec<(UserId, u64)> {
    sqlx::query("SELECT user_id, xp FROM xp WHERE guild_id = ? ORDER BY xp DESC LIMIT ?")
        .bind(guild_id.get() as i64)
        .bind(limit)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            (
                UserId::new(r.get::<i64, _>("user_id") as u64),
                r.get::<i64, _>("xp") as u64,
            )
        })
        .collect()
}

pub async fn reset_user_xp(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> bool {
    let r = sqlx::query("DELETE FROM xp WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .execute(pool)
        .await;

    matches!(r, Ok(res) if res.rows_affected() > 0)
}

pub async fn bulk_add_xp(pool: &SqlitePool, guild_id: GuildId, counts: &[(UserId, u64)]) {
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => { tracing::error!("Transaktion starten fehlgeschlagen: {e}"); return; }
    };

    for (user_id, count) in counts {
        let r = sqlx::query(
            "INSERT INTO xp (guild_id, user_id, xp) VALUES (?, ?, ?)
             ON CONFLICT(guild_id, user_id) DO UPDATE SET xp = xp + excluded.xp",
        )
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .bind((count * 20) as i64)
        .execute(&mut *tx)
        .await;

        if let Err(e) = r {
            tracing::error!("Bulk XP fehlgeschlagen für {user_id}: {e}");
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("Transaktion commit fehlgeschlagen: {e}");
    }
}

// ── warnings ──────────────────────────────────────────────────────────────────

pub async fn add_warning(
    pool: &SqlitePool,
    guild_id: GuildId,
    user_id: UserId,
    moderator_id: UserId,
    reason: &str,
) {
    let r = sqlx::query(
        "INSERT INTO warnings (guild_id, user_id, moderator_id, reason) VALUES (?, ?, ?, ?)",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(moderator_id.get() as i64)
    .bind(reason)
    .execute(pool)
    .await;

    if let Err(e) = r {
        tracing::error!("Verwarnung speichern fehlgeschlagen: {e}");
    }
}

pub async fn get_warnings(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Vec<(UserId, String)> {
    sqlx::query(
        "SELECT moderator_id, reason FROM warnings WHERE guild_id = ? AND user_id = ? ORDER BY id ASC",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        (
            UserId::new(r.get::<i64, _>("moderator_id") as u64),
            r.get::<String, _>("reason"),
        )
    })
    .collect()
}

pub async fn clear_warnings(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> u64 {
    sqlx::query("DELETE FROM warnings WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0)
}

// ── jailed users ──────────────────────────────────────────────────────────────

pub async fn jail_user(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, roles: &[RoleId]) {
    let roles_json =
        serde_json::to_string(&roles.iter().map(|r| r.get()).collect::<Vec<u64>>())
            .unwrap_or_else(|_| "[]".to_string());

    let r = sqlx::query(
        "INSERT INTO jailed_users (guild_id, user_id, roles) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET roles = excluded.roles",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(roles_json)
    .execute(pool)
    .await;

    if let Err(e) = r {
        tracing::error!("Jail speichern fehlgeschlagen: {e}");
    }
}

/// Removes the user from jail and returns their original roles.
pub async fn unjail_user(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Vec<RoleId> {
    let gid = guild_id.get() as i64;
    let uid = user_id.get() as i64;

    let row = sqlx::query("SELECT roles FROM jailed_users WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(uid)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let roles = match row {
        Some(r) => {
            let json: String = r.get("roles");
            serde_json::from_str::<Vec<u64>>(&json)
                .unwrap_or_default()
                .into_iter()
                .map(RoleId::new)
                .collect()
        }
        None => vec![],
    };

    let r = sqlx::query("DELETE FROM jailed_users WHERE guild_id = ? AND user_id = ?")
        .bind(gid)
        .bind(uid)
        .execute(pool)
        .await;

    if let Err(e) = r {
        tracing::error!("Jail löschen fehlgeschlagen: {e}");
    }

    roles
}

// ── economy ───────────────────────────────────────────────────────────────────

pub async fn get_coins(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT coins FROM economy WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0)
}

pub async fn get_invites(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT invites FROM economy WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0)
}

// ── bank ──────────────────────────────────────────────────────────────────────

pub async fn get_bank(pool: &SqlitePool, guild_id: GuildId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT coins FROM bank WHERE guild_id = ?")
        .bind(guild_id.get() as i64)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .unwrap_or(0)
}

pub async fn add_to_bank(pool: &SqlitePool, guild_id: GuildId, amount: i64) {
    let r = sqlx::query(
        "INSERT INTO bank (guild_id, coins) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET coins = coins + excluded.coins",
    )
    .bind(guild_id.get() as i64)
    .bind(amount)
    .execute(pool)
    .await;
    if let Err(e) = r { tracing::error!("Bank-Einzahlung fehlgeschlagen: {e}"); }
}

/// Drains the bank to zero and returns what was in it. Atomic — safe against concurrent robberies.
pub async fn drain_bank(pool: &SqlitePool, guild_id: GuildId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "UPDATE bank SET coins = 0 WHERE guild_id = ? RETURNING coins",
    )
    .bind(guild_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0)
}

// ── robbery jail ──────────────────────────────────────────────────────────────

/// Returns the unix timestamp until which the user is jailed, or None.
pub async fn get_jail_until(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT jail_until FROM robbery_jail WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn set_jail_until(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, until: i64) {
    let r = sqlx::query(
        "INSERT INTO robbery_jail (guild_id, user_id, jail_until) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET jail_until = excluded.jail_until",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(until)
    .execute(pool)
    .await;
    if let Err(e) = r { tracing::error!("Jail-Setzen fehlgeschlagen: {e}"); }
}

// ── bankraub cooldown ─────────────────────────────────────────────────────────

pub async fn get_bankraub_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT last_used FROM bankraub_cooldowns WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn set_bankraub_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, ts: i64) {
    let r = sqlx::query(
        "INSERT INTO bankraub_cooldowns (guild_id, user_id, last_used) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET last_used = excluded.last_used",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(ts)
    .execute(pool)
    .await;
    if let Err(e) = r { tracing::error!("Bankraub-Cooldown fehlgeschlagen: {e}"); }
}

pub async fn get_klauen_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT last_used FROM klauen_cooldowns WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn set_klauen_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, ts: i64) {
    let r = sqlx::query(
        "INSERT INTO klauen_cooldowns (guild_id, user_id, last_used) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET last_used = excluded.last_used",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(ts)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::error!("Klauen-Cooldown speichern fehlgeschlagen: {e}");
    }
}

/// Records one invite: awards 250 coins, increments invite count, returns new coin balance.
pub async fn record_invite(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO economy (guild_id, user_id, coins, invites) VALUES (?, ?, 250, 1)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET coins = coins + 250, invites = invites + 1
         RETURNING coins",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// Adds `amount` coins and returns the new total.
pub async fn add_coins(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, amount: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO economy (guild_id, user_id, coins) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET coins = MAX(coins + excluded.coins, 0)
         RETURNING coins",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(amount)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

// ── arbeit cooldowns ──────────────────────────────────────────────────────────

/// Returns the unix timestamp of the last /arbeit use, or None.
pub async fn get_arbeit_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT last_used FROM arbeit_cooldowns WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn set_arbeit_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, ts: i64) {
    let r = sqlx::query(
        "INSERT INTO arbeit_cooldowns (guild_id, user_id, last_used) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET last_used = excluded.last_used",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(ts)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::error!("Arbeit-Cooldown speichern fehlgeschlagen: {e}");
    }
}

pub async fn get_coins_leaderboard(
    pool: &SqlitePool,
    guild_id: GuildId,
    limit: u32,
) -> Vec<(UserId, i64)> {
    let rows = sqlx::query(
        "SELECT user_id, coins FROM economy WHERE guild_id = ? ORDER BY coins DESC LIMIT ?",
    )
    .bind(guild_id.get() as i64)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| (
            UserId::new(r.get::<i64, _>("user_id") as u64),
            r.get::<i64, _>("coins"),
        ))
        .collect()
}

// ── message log persistence ───────────────────────────────────────────────────

pub struct StoredMessage {
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub user_id: UserId,
    pub user_tag: String,
    pub content: String,
    pub attachment_names: Vec<String>,
}

pub async fn store_message(
    pool: &SqlitePool,
    message_id: MessageId,
    guild_id: GuildId,
    channel_id: ChannelId,
    user_id: UserId,
    user_tag: &str,
    content: &str,
    attachment_names: &[String],
) {
    let att = serde_json::to_string(attachment_names).unwrap_or_else(|_| "[]".to_string());
    let _ = sqlx::query(
        "INSERT OR REPLACE INTO messages (message_id, guild_id, channel_id, user_id, user_tag, content, attachment_names)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(message_id.get() as i64)
    .bind(guild_id.get() as i64)
    .bind(channel_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(user_tag)
    .bind(content)
    .bind(att)
    .execute(pool)
    .await;
}

pub async fn get_message(pool: &SqlitePool, message_id: MessageId) -> Option<StoredMessage> {
    let row = sqlx::query(
        "SELECT guild_id, channel_id, user_id, user_tag, content, attachment_names
         FROM messages WHERE message_id = ?",
    )
    .bind(message_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)?;

    let att_json: String = row.get("attachment_names");
    let attachment_names = serde_json::from_str::<Vec<String>>(&att_json).unwrap_or_default();

    Some(StoredMessage {
        guild_id:         GuildId::new(row.get::<i64, _>("guild_id") as u64),
        channel_id:       ChannelId::new(row.get::<i64, _>("channel_id") as u64),
        user_id:          UserId::new(row.get::<i64, _>("user_id") as u64),
        user_tag:         row.get("user_tag"),
        content:          row.get("content"),
        attachment_names,
    })
}

pub async fn update_message_content(pool: &SqlitePool, message_id: MessageId, new_content: &str) {
    let _ = sqlx::query("UPDATE messages SET content = ? WHERE message_id = ?")
        .bind(new_content)
        .bind(message_id.get() as i64)
        .execute(pool)
        .await;
}

// ── fishing ───────────────────────────────────────────────────────────────────

pub async fn get_fishing_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT last_used FROM fishing_cooldowns WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn set_fishing_cooldown(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, ts: i64) {
    let _ = sqlx::query(
        "INSERT INTO fishing_cooldowns (guild_id, user_id, last_used) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET last_used = excluded.last_used",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(ts)
    .execute(pool)
    .await;
}

pub async fn get_fishing_rod(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT rod_id FROM fishing_rods WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "grundangel".to_string())
}

pub async fn set_fishing_rod(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, rod_id: &str) {
    let _ = sqlx::query(
        "INSERT INTO fishing_rods (guild_id, user_id, rod_id) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET rod_id = excluded.rod_id",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(rod_id)
    .execute(pool)
    .await;
}

pub async fn add_fish_to_inventory(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, fish_id: &str, ts: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO fishing_inventory (guild_id, user_id, fish_id, caught_at) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(fish_id)
    .bind(ts)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

pub struct FishEntry {
    pub id: i64,
    pub fish_id: String,
    pub caught_at: i64,
}

pub async fn get_fish_inventory(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Vec<FishEntry> {
    let rows = sqlx::query(
        "SELECT id, fish_id, caught_at FROM fishing_inventory
         WHERE guild_id = ? AND user_id = ? ORDER BY caught_at DESC",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|r| FishEntry {
        id:        r.get::<i64, _>("id"),
        fish_id:   r.get("fish_id"),
        caught_at: r.get::<i64, _>("caught_at"),
    }).collect()
}

pub async fn remove_fish_from_inventory(pool: &SqlitePool, entry_id: i64) {
    let _ = sqlx::query("DELETE FROM fishing_inventory WHERE id = ?")
        .bind(entry_id)
        .execute(pool)
        .await;
}

pub async fn remove_all_fish(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> Vec<FishEntry> {
    let entries = get_fish_inventory(pool, guild_id, user_id).await;
    let _ = sqlx::query("DELETE FROM fishing_inventory WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .execute(pool)
        .await;
    entries
}

pub async fn get_fish_price(pool: &SqlitePool, fish_id: &str) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT price FROM fishing_prices WHERE fish_id = ?",
    )
    .bind(fish_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn set_fish_price(pool: &SqlitePool, fish_id: &str, price: i64, ts: i64) {
    let _ = sqlx::query(
        "INSERT INTO fishing_prices (fish_id, price, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(fish_id) DO UPDATE SET price = excluded.price, updated_at = excluded.updated_at",
    )
    .bind(fish_id)
    .bind(price)
    .bind(ts)
    .execute(pool)
    .await;
}

pub async fn get_all_fish_prices(pool: &SqlitePool) -> Vec<(String, i64)> {
    let rows = sqlx::query("SELECT fish_id, price FROM fishing_prices")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    rows.into_iter().map(|r| (r.get("fish_id"), r.get::<i64, _>("price"))).collect()
}

// ── giveaways ─────────────────────────────────────────────────────────────────

pub struct GiveawayRow {
    pub id:             i64,
    pub guild_id:       GuildId,
    pub channel_id:     ChannelId,
    pub message_id:     Option<serenity::MessageId>,
    pub prize:          String,
    pub ticket_price:   i64,
    pub required_level: i64,
    pub ends_at:        i64,
}

pub async fn create_giveaway(
    pool:           &SqlitePool,
    guild_id:       GuildId,
    channel_id:     ChannelId,
    prize:          &str,
    ticket_price:   i64,
    required_level: i64,
    ends_at:        i64,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO giveaways (guild_id, channel_id, prize, ticket_price, required_level, ends_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(guild_id.get() as i64)
    .bind(channel_id.get() as i64)
    .bind(prize)
    .bind(ticket_price)
    .bind(required_level)
    .bind(ends_at)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

pub async fn set_giveaway_message(pool: &SqlitePool, giveaway_id: i64, message_id: serenity::MessageId) {
    let _ = sqlx::query("UPDATE giveaways SET message_id = ? WHERE id = ?")
        .bind(message_id.get() as i64)
        .bind(giveaway_id)
        .execute(pool)
        .await;
}

pub async fn enter_giveaway(pool: &SqlitePool, giveaway_id: i64, user_id: UserId) -> bool {
    sqlx::query(
        "INSERT OR IGNORE INTO giveaway_entries (giveaway_id, user_id) VALUES (?, ?)",
    )
    .bind(giveaway_id)
    .bind(user_id.get() as i64)
    .execute(pool)
    .await
    .map(|r| r.rows_affected() > 0)
    .unwrap_or(false)
}

pub async fn get_giveaway_entries(pool: &SqlitePool, giveaway_id: i64) -> Vec<UserId> {
    sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM giveaway_entries WHERE giveaway_id = ?",
    )
    .bind(giveaway_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|id| UserId::new(id as u64))
    .collect()
}

pub async fn get_giveaway_entry_count(pool: &SqlitePool, giveaway_id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM giveaway_entries WHERE giveaway_id = ?",
    )
    .bind(giveaway_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

pub async fn is_entered(pool: &SqlitePool, giveaway_id: i64, user_id: UserId) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM giveaway_entries WHERE giveaway_id = ? AND user_id = ?",
    )
    .bind(giveaway_id)
    .bind(user_id.get() as i64)
    .fetch_one(pool)
    .await
    .unwrap_or(0) > 0
}

pub async fn end_giveaway(pool: &SqlitePool, giveaway_id: i64, winner_id: Option<UserId>) {
    let _ = sqlx::query(
        "UPDATE giveaways SET ended = 1, winner_id = ? WHERE id = ?",
    )
    .bind(winner_id.map(|u| u.get() as i64))
    .bind(giveaway_id)
    .execute(pool)
    .await;
}

/// Returns all active (not yet ended) giveaways — used to reschedule on restart.
pub async fn get_active_giveaways(pool: &SqlitePool) -> Vec<GiveawayRow> {
    let rows = sqlx::query(
        "SELECT id, guild_id, channel_id, message_id, prize, ticket_price, required_level, ends_at
         FROM giveaways WHERE ended = 0",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|r| GiveawayRow {
        id:             r.get::<i64, _>("id"),
        guild_id:       GuildId::new(r.get::<i64, _>("guild_id") as u64),
        channel_id:     ChannelId::new(r.get::<i64, _>("channel_id") as u64),
        message_id:     r.get::<Option<i64>, _>("message_id").map(|v| serenity::MessageId::new(v as u64)),
        prize:          r.get("prize"),
        ticket_price:   r.get::<i64, _>("ticket_price"),
        required_level: r.get::<i64, _>("required_level"),
        ends_at:        r.get::<i64, _>("ends_at"),
    }).collect()
}

pub async fn get_giveaway_by_message(pool: &SqlitePool, message_id: serenity::MessageId) -> Option<GiveawayRow> {
    let r = sqlx::query(
        "SELECT id, guild_id, channel_id, message_id, prize, ticket_price, required_level, ends_at
         FROM giveaways WHERE message_id = ? AND ended = 0",
    )
    .bind(message_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)?;

    Some(GiveawayRow {
        id:             r.get::<i64, _>("id"),
        guild_id:       GuildId::new(r.get::<i64, _>("guild_id") as u64),
        channel_id:     ChannelId::new(r.get::<i64, _>("channel_id") as u64),
        message_id:     r.get::<Option<i64>, _>("message_id").map(|v| serenity::MessageId::new(v as u64)),
        prize:          r.get("prize"),
        ticket_price:   r.get::<i64, _>("ticket_price"),
        required_level: r.get::<i64, _>("required_level"),
        ends_at:        r.get::<i64, _>("ends_at"),
    })
}

// ── casino ────────────────────────────────────────────────────────────────────

pub struct CasinoStats {
    pub total_wagered: i64,
    pub total_won:     i64,
    pub total_lost:    i64,
    pub biggest_win:   i64,
    pub win_streak:    i64,
    pub lose_streak:   i64,
    pub games_played:  i64,
}

impl Default for CasinoStats {
    fn default() -> Self {
        Self { total_wagered: 0, total_won: 0, total_lost: 0, biggest_win: 0, win_streak: 0, lose_streak: 0, games_played: 0 }
    }
}

pub async fn get_casino_stats(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> CasinoStats {
    let row = sqlx::query(
        "SELECT total_wagered, total_won, total_lost, biggest_win, win_streak, lose_streak, games_played
         FROM casino_stats WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some(r) => CasinoStats {
            total_wagered: r.get("total_wagered"),
            total_won:     r.get("total_won"),
            total_lost:    r.get("total_lost"),
            biggest_win:   r.get("biggest_win"),
            win_streak:    r.get("win_streak"),
            lose_streak:   r.get("lose_streak"),
            games_played:  r.get("games_played"),
        },
        None => CasinoStats::default(),
    }
}

/// Returns (new_win_streak, new_lose_streak).
pub async fn update_casino_stats(
    pool:        &SqlitePool,
    guild_id:    GuildId,
    user_id:     UserId,
    wagered:     i64,
    won:         bool,
    net_gain:    i64,
    biggest_win: i64,
) -> (i64, i64) {
    let cur = get_casino_stats(pool, guild_id, user_id).await;
    let new_win_streak  = if won  { cur.win_streak + 1  } else { 0 };
    let new_lose_streak = if !won { cur.lose_streak + 1 } else { 0 };
    let won_delta  = if won  { net_gain } else { 0 };
    let lost_delta = if !won { wagered  } else { 0 };
    let new_biggest = cur.biggest_win.max(biggest_win);

    let _ = sqlx::query(
        "INSERT INTO casino_stats (guild_id, user_id, total_wagered, total_won, total_lost, biggest_win, win_streak, lose_streak, games_played)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET
             total_wagered = total_wagered + excluded.total_wagered,
             total_won     = total_won     + excluded.total_won,
             total_lost    = total_lost    + excluded.total_lost,
             biggest_win   = excluded.biggest_win,
             win_streak    = excluded.win_streak,
             lose_streak   = excluded.lose_streak,
             games_played  = games_played  + 1",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(wagered)
    .bind(won_delta)
    .bind(lost_delta)
    .bind(new_biggest)
    .bind(new_win_streak)
    .bind(new_lose_streak)
    .execute(pool)
    .await;

    (new_win_streak, new_lose_streak)
}

pub async fn get_casino_leaderboard(pool: &SqlitePool, guild_id: GuildId) -> Vec<(UserId, CasinoStats)> {
    let rows = sqlx::query(
        "SELECT user_id, total_wagered, total_won, total_lost, biggest_win, win_streak, lose_streak, games_played
         FROM casino_stats WHERE guild_id = ? ORDER BY total_won DESC LIMIT 10",
    )
    .bind(guild_id.get() as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|r| (
        UserId::new(r.get::<i64, _>("user_id") as u64),
        CasinoStats {
            total_wagered: r.get("total_wagered"),
            total_won:     r.get("total_won"),
            total_lost:    r.get("total_lost"),
            biggest_win:   r.get("biggest_win"),
            win_streak:    r.get("win_streak"),
            lose_streak:   r.get("lose_streak"),
            games_played:  r.get("games_played"),
        },
    )).collect()
}

pub async fn casino_vault_get(pool: &SqlitePool, guild_id: GuildId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT balance FROM casino_vault WHERE guild_id = ?")
        .bind(guild_id.get() as i64)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .unwrap_or(0)
}

pub async fn casino_vault_add(pool: &SqlitePool, guild_id: GuildId, amount: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO casino_vault (guild_id, balance) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET balance = balance + excluded.balance
         RETURNING balance",
    )
    .bind(guild_id.get() as i64)
    .bind(amount)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

pub async fn get_casino_channel(pool: &SqlitePool, guild_id: GuildId) -> Option<ChannelId> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT channel_id FROM casino_config WHERE guild_id = ?",
    )
    .bind(guild_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten()
    .map(|v| ChannelId::new(v as u64))
}

pub async fn set_casino_channel(pool: &SqlitePool, guild_id: GuildId, channel_id: Option<ChannelId>) {
    let _ = sqlx::query(
        "INSERT INTO casino_config (guild_id, channel_id) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET channel_id = excluded.channel_id",
    )
    .bind(guild_id.get() as i64)
    .bind(channel_id.map(|c| c.get() as i64))
    .execute(pool)
    .await;
}

pub async fn get_casino_daily_limit(pool: &SqlitePool, guild_id: GuildId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT daily_limit FROM casino_config WHERE guild_id = ?")
        .bind(guild_id.get() as i64)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .unwrap_or(0)
}

pub async fn set_casino_daily_limit(pool: &SqlitePool, guild_id: GuildId, limit: i64) {
    let _ = sqlx::query(
        "INSERT INTO casino_config (guild_id, daily_limit) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET daily_limit = excluded.daily_limit",
    )
    .bind(guild_id.get() as i64)
    .bind(limit)
    .execute(pool)
    .await;
}

pub async fn get_casino_daily_loss(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> i64 {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let row = sqlx::query(
        "SELECT lost, day FROM casino_daily WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some(r) => {
            let day: String = r.get("day");
            if day == today { r.get::<i64, _>("lost") } else { 0 }
        }
        None => 0,
    }
}

pub async fn add_casino_daily_loss(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, amount: i64) {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let current_day = sqlx::query_scalar::<_, String>(
        "SELECT day FROM casino_daily WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    if current_day.as_deref() == Some(&today) {
        let _ = sqlx::query(
            "UPDATE casino_daily SET lost = lost + ? WHERE guild_id = ? AND user_id = ?",
        )
        .bind(amount)
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .execute(pool)
        .await;
    } else {
        let _ = sqlx::query(
            "INSERT INTO casino_daily (guild_id, user_id, day, lost) VALUES (?, ?, ?, ?)
             ON CONFLICT(guild_id, user_id) DO UPDATE SET day = excluded.day, lost = excluded.lost",
        )
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .bind(&today)
        .bind(amount)
        .execute(pool)
        .await;
    }
}

// ── lotto ─────────────────────────────────────────────────────────────────────

pub struct LottoDrawing {
    pub id:       i64,
    pub guild_id: GuildId,
    pub jackpot:  i64,
    pub channel_id: Option<ChannelId>,
}

pub async fn get_active_lotto_drawing(pool: &SqlitePool, guild_id: GuildId) -> Option<LottoDrawing> {
    let r = sqlx::query(
        "SELECT id, jackpot, channel_id FROM lotto_drawings WHERE guild_id = ? AND drawn_at IS NULL LIMIT 1",
    )
    .bind(guild_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)?;

    Some(LottoDrawing {
        id:         r.get::<i64, _>("id"),
        guild_id,
        jackpot:    r.get::<i64, _>("jackpot"),
        channel_id: r.get::<Option<i64>, _>("channel_id").map(|v| ChannelId::new(v as u64)),
    })
}

pub async fn get_or_create_lotto_drawing(pool: &SqlitePool, guild_id: GuildId, channel_id: ChannelId) -> LottoDrawing {
    if let Some(d) = get_active_lotto_drawing(pool, guild_id).await {
        return d;
    }
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO lotto_drawings (guild_id, channel_id) VALUES (?, ?) RETURNING id",
    )
    .bind(guild_id.get() as i64)
    .bind(channel_id.get() as i64)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    LottoDrawing { id, guild_id, jackpot: 0, channel_id: Some(channel_id) }
}

pub async fn add_lotto_ticket(pool: &SqlitePool, drawing_id: i64, guild_id: GuildId, user_id: UserId, numbers: &[u8]) {
    let nums = serde_json::to_string(numbers).unwrap_or_default();
    let _ = sqlx::query(
        "INSERT INTO lotto_tickets (drawing_id, guild_id, user_id, numbers) VALUES (?, ?, ?, ?)",
    )
    .bind(drawing_id)
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(nums)
    .execute(pool)
    .await;
}

pub async fn add_to_lotto_jackpot(pool: &SqlitePool, drawing_id: i64, amount: i64) {
    let _ = sqlx::query("UPDATE lotto_drawings SET jackpot = jackpot + ? WHERE id = ?")
        .bind(amount)
        .bind(drawing_id)
        .execute(pool)
        .await;
}

pub struct LottoTicket {
    pub user_id: UserId,
    pub numbers: Vec<u8>,
}

pub async fn get_lotto_tickets(pool: &SqlitePool, drawing_id: i64) -> Vec<LottoTicket> {
    let rows = sqlx::query(
        "SELECT user_id, numbers FROM lotto_tickets WHERE drawing_id = ?",
    )
    .bind(drawing_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|r| LottoTicket {
        user_id: UserId::new(r.get::<i64, _>("user_id") as u64),
        numbers: serde_json::from_str::<Vec<u8>>(r.get("numbers")).unwrap_or_default(),
    }).collect()
}

pub async fn close_lotto_drawing(pool: &SqlitePool, drawing_id: i64, winning_numbers: &[u8]) {
    let nums = serde_json::to_string(winning_numbers).unwrap_or_default();
    let now  = chrono::Utc::now().timestamp();
    let _ = sqlx::query(
        "UPDATE lotto_drawings SET numbers = ?, drawn_at = ? WHERE id = ?",
    )
    .bind(nums)
    .bind(now)
    .bind(drawing_id)
    .execute(pool)
    .await;
}

pub async fn get_guilds_with_active_lotto(pool: &SqlitePool) -> Vec<(GuildId, LottoDrawing)> {
    let rows = sqlx::query(
        "SELECT id, guild_id, jackpot, channel_id FROM lotto_drawings WHERE drawn_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|r| {
        let guild_id = GuildId::new(r.get::<i64, _>("guild_id") as u64);
        (guild_id, LottoDrawing {
            id:         r.get::<i64, _>("id"),
            guild_id,
            jackpot:    r.get::<i64, _>("jackpot"),
            channel_id: r.get::<Option<i64>, _>("channel_id").map(|v| ChannelId::new(v as u64)),
        })
    }).collect()
}

// ── bot channel ───────────────────────────────────────────────────────────────

pub async fn get_bot_channel(pool: &SqlitePool, guild_id: GuildId) -> Option<ChannelId> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT bot_channel FROM guild_settings WHERE guild_id = ?",
    )
    .bind(guild_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten()
    .map(|v| ChannelId::new(v as u64))
}

pub async fn set_bot_channel(pool: &SqlitePool, guild_id: GuildId, channel_id: Option<ChannelId>) {
    let _ = sqlx::query(
        "INSERT INTO guild_settings (guild_id, bot_channel) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET bot_channel = excluded.bot_channel",
    )
    .bind(guild_id.get() as i64)
    .bind(channel_id.map(|c| c.get() as i64))
    .execute(pool)
    .await;
}

// ── shop purchases ────────────────────────────────────────────────────────────

pub async fn get_shop_item_qty(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, item_id: &str) -> i64 {
    let now = chrono::Utc::now().timestamp();
    sqlx::query_scalar::<_, i64>(
        "SELECT quantity FROM shop_purchases
         WHERE guild_id = ? AND user_id = ? AND item_id = ?
           AND quantity > 0 AND (expires_at = 0 OR expires_at > ?)",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(item_id)
    .bind(now)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0)
}

pub async fn has_active_shop_item(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, item_id: &str) -> bool {
    get_shop_item_qty(pool, guild_id, user_id, item_id).await > 0
}

/// Adds qty items. For time-limited items, extends if already active.
pub async fn add_shop_item(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, item_id: &str, qty: i64, expires_at: i64) {
    let r = if expires_at > 0 {
        sqlx::query(
            "INSERT INTO shop_purchases (guild_id, user_id, item_id, quantity, expires_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(guild_id, user_id, item_id) DO UPDATE SET
                 quantity   = quantity + excluded.quantity,
                 expires_at = CASE WHEN expires_at > strftime('%s','now')
                                THEN expires_at + 3600
                                ELSE excluded.expires_at END",
        )
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .bind(item_id)
        .bind(qty)
        .bind(expires_at)
        .execute(pool)
        .await
    } else {
        sqlx::query(
            "INSERT INTO shop_purchases (guild_id, user_id, item_id, quantity, expires_at) VALUES (?, ?, ?, ?, 0)
             ON CONFLICT(guild_id, user_id, item_id) DO UPDATE SET quantity = quantity + excluded.quantity",
        )
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .bind(item_id)
        .bind(qty)
        .execute(pool)
        .await
    };
    if let Err(e) = r { tracing::error!("Shop-Item hinzufügen fehlgeschlagen: {e}"); }
}

/// Decrements quantity by 1. Returns true if item was present and consumed.
pub async fn consume_shop_item(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, item_id: &str) -> bool {
    if get_shop_item_qty(pool, guild_id, user_id, item_id).await <= 0 { return false; }
    let _ = sqlx::query(
        "UPDATE shop_purchases SET quantity = quantity - 1
         WHERE guild_id = ? AND user_id = ? AND item_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(item_id)
    .execute(pool)
    .await;
    true
}

// ── prestige ──────────────────────────────────────────────────────────────────

pub async fn get_prestige(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> u64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count FROM prestige WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0) as u64
}

pub async fn increment_prestige(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) {
    let _ = sqlx::query(
        "INSERT INTO prestige (guild_id, user_id, count) VALUES (?, ?, 1)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET count = count + 1",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .execute(pool)
    .await;
}

pub async fn reset_xp_to_zero(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) {
    let _ = sqlx::query("UPDATE xp SET xp = 0 WHERE guild_id = ? AND user_id = ?")
        .bind(guild_id.get() as i64)
        .bind(user_id.get() as i64)
        .execute(pool)
        .await;
}

// ── level coins credited ──────────────────────────────────────────────────────

pub async fn get_credited_level(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) -> u64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT credited_level FROM level_coins_credited WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .unwrap_or(0) as u64
}

pub async fn set_credited_level(pool: &SqlitePool, guild_id: GuildId, user_id: UserId, level: u64) {
    let _ = sqlx::query(
        "INSERT INTO level_coins_credited (guild_id, user_id, credited_level) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET credited_level = excluded.credited_level",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .bind(level as i64)
    .execute(pool)
    .await;
}

// ── guild-wide helpers ────────────────────────────────────────────────────────

/// All (GuildId, ChannelId) pairs that have a bot_channel configured.
pub async fn get_guilds_with_bot_channel(pool: &SqlitePool) -> Vec<(GuildId, ChannelId)> {
    let rows = sqlx::query(
        "SELECT guild_id, bot_channel FROM guild_settings WHERE bot_channel IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter_map(|r| {
            let gid = GuildId::new(r.get::<i64, _>("guild_id") as u64);
            let ch  = r.get::<Option<i64>, _>("bot_channel").map(|v| ChannelId::new(v as u64))?;
            Some((gid, ch))
        })
        .collect()
}

/// All (UserId, total_xp) for a guild.
pub async fn get_guild_xp_users(pool: &SqlitePool, guild_id: GuildId) -> Vec<(UserId, u64)> {
    let rows = sqlx::query("SELECT user_id, xp FROM xp WHERE guild_id = ?")
        .bind(guild_id.get() as i64)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    rows.into_iter()
        .map(|r| (
            UserId::new(r.get::<i64, _>("user_id") as u64),
            r.get::<i64, _>("xp") as u64,
        ))
        .collect()
}

// ── anti-nuke ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AntiNukeConfig {
    pub guild_id:             i64,
    pub enabled:              i64,
    pub chan_del_max:         i64,
    pub chan_cre_max:         i64,
    pub role_del_max:         i64,
    pub role_cre_max:         i64,
    pub ban_max:              i64,
    pub webhook_max:          i64,
    pub window_secs:          i64,
    pub raid_joins:           i64,
    pub raid_window:          i64,
    pub min_account_age_days: i64,
    pub lockdown_mins:        i64,
    pub punishment:           String,
}

impl AntiNukeConfig {
    pub fn default_for(guild_id: GuildId) -> Self {
        AntiNukeConfig {
            guild_id:             guild_id.get() as i64,
            enabled:              1,
            chan_del_max:         3,
            chan_cre_max:         5,
            role_del_max:         3,
            role_cre_max:         5,
            ban_max:              5,
            webhook_max:          5,
            window_secs:          10,
            raid_joins:           10,
            raid_window:          10,
            min_account_age_days: 0,
            lockdown_mins:        15,
            punishment:           "ban".to_string(),
        }
    }
}

pub async fn get_antinuke_config(pool: &SqlitePool, guild_id: GuildId) -> Option<AntiNukeConfig> {
    sqlx::query_as::<_, AntiNukeConfig>(
        "SELECT * FROM antinuke_config WHERE guild_id = ?",
    )
    .bind(guild_id.get() as i64)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

pub async fn get_or_create_antinuke_config(pool: &SqlitePool, guild_id: GuildId) -> AntiNukeConfig {
    if let Some(cfg) = get_antinuke_config(pool, guild_id).await {
        return cfg;
    }
    let cfg = AntiNukeConfig::default_for(guild_id);
    set_antinuke_config(pool, &cfg).await;
    cfg
}

pub async fn set_antinuke_config(pool: &SqlitePool, cfg: &AntiNukeConfig) {
    let r = sqlx::query(
        "INSERT INTO antinuke_config
            (guild_id, enabled, chan_del_max, chan_cre_max, role_del_max, role_cre_max,
             ban_max, webhook_max, window_secs, raid_joins, raid_window,
             min_account_age_days, lockdown_mins, punishment)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(guild_id) DO UPDATE SET
             enabled              = excluded.enabled,
             chan_del_max         = excluded.chan_del_max,
             chan_cre_max         = excluded.chan_cre_max,
             role_del_max         = excluded.role_del_max,
             role_cre_max         = excluded.role_cre_max,
             ban_max              = excluded.ban_max,
             webhook_max          = excluded.webhook_max,
             window_secs          = excluded.window_secs,
             raid_joins           = excluded.raid_joins,
             raid_window          = excluded.raid_window,
             min_account_age_days = excluded.min_account_age_days,
             lockdown_mins        = excluded.lockdown_mins,
             punishment           = excluded.punishment",
    )
    .bind(cfg.guild_id)
    .bind(cfg.enabled)
    .bind(cfg.chan_del_max)
    .bind(cfg.chan_cre_max)
    .bind(cfg.role_del_max)
    .bind(cfg.role_cre_max)
    .bind(cfg.ban_max)
    .bind(cfg.webhook_max)
    .bind(cfg.window_secs)
    .bind(cfg.raid_joins)
    .bind(cfg.raid_window)
    .bind(cfg.min_account_age_days)
    .bind(cfg.lockdown_mins)
    .bind(&cfg.punishment)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::error!("antinuke_config speichern fehlgeschlagen: {e}");
    }
}

pub async fn add_antinuke_whitelist(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) {
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO antinuke_whitelist (guild_id, user_id) VALUES (?, ?)",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .execute(pool)
    .await;
}

pub async fn remove_antinuke_whitelist(pool: &SqlitePool, guild_id: GuildId, user_id: UserId) {
    let _ = sqlx::query(
        "DELETE FROM antinuke_whitelist WHERE guild_id = ? AND user_id = ?",
    )
    .bind(guild_id.get() as i64)
    .bind(user_id.get() as i64)
    .execute(pool)
    .await;
}

pub async fn get_antinuke_whitelist(pool: &SqlitePool, guild_id: GuildId) -> Vec<UserId> {
    sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM antinuke_whitelist WHERE guild_id = ?",
    )
    .bind(guild_id.get() as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|v| UserId::new(v as u64))
    .collect()
}
