use poise::serenity_prelude as serenity;
use serenity::{CreateEmbed, Timestamp};

use crate::config::ActionKind;
use crate::db::AntiNukeConfig;
use crate::{Context, Error};

// ── choice enums ──────────────────────────────────────────────────────────────

#[derive(Debug, poise::ChoiceParameter)]
pub enum ActionChoice {
    #[name = "Channel Delete"]
    ChannelDelete,
    #[name = "Channel Create"]
    ChannelCreate,
    #[name = "Role Delete"]
    RoleDelete,
    #[name = "Role Create"]
    RoleCreate,
    #[name = "Ban"]
    Ban,
    #[name = "Webhook Create"]
    WebhookCreate,
}

impl ActionChoice {
    fn to_kind(&self) -> ActionKind {
        match self {
            ActionChoice::ChannelDelete => ActionKind::ChannelDelete,
            ActionChoice::ChannelCreate => ActionKind::ChannelCreate,
            ActionChoice::RoleDelete    => ActionKind::RoleDelete,
            ActionChoice::RoleCreate    => ActionKind::RoleCreate,
            ActionChoice::Ban           => ActionKind::Ban,
            ActionChoice::WebhookCreate => ActionKind::WebhookCreate,
        }
    }
}

#[derive(Debug, poise::ChoiceParameter)]
pub enum PunishmentChoice {
    #[name = "Bann"]
    Ban,
    #[name = "Kick"]
    Kick,
    #[name = "Rollen entfernen"]
    Strip,
}

impl PunishmentChoice {
    fn as_str(&self) -> &'static str {
        match self {
            PunishmentChoice::Ban   => "ban",
            PunishmentChoice::Kick  => "kick",
            PunishmentChoice::Strip => "strip",
        }
    }
}

// ── parent command ────────────────────────────────────────────────────────────

/// Anti-Nuke Schutz konfigurieren
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "ADMINISTRATOR",
    subcommands(
        "enable",
        "disable",
        "status",
        "whitelist_add",
        "whitelist_remove",
        "set",
        "unlock"
    ),
    subcommand_required
)]
pub async fn antinuke(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

// ── /antinuke enable ──────────────────────────────────────────────────────────

/// Anti-Nuke Schutz aktivieren
#[poise::command(slash_command, guild_only)]
async fn enable(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.enabled = 1;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Anti-Nuke aktiviert", "Der Schutz ist jetzt aktiv."))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

// ── /antinuke disable ─────────────────────────────────────────────────────────

/// Anti-Nuke Schutz deaktivieren
#[poise::command(slash_command, guild_only)]
async fn disable(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.enabled = 0;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Anti-Nuke deaktiviert", "Der Schutz ist jetzt inaktiv."))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

// ── /antinuke status ──────────────────────────────────────────────────────────

/// Aktuelle Anti-Nuke Konfiguration anzeigen
#[poise::command(slash_command, guild_only)]
async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    let whitelist = crate::db::get_antinuke_whitelist(&ctx.data().db, guild_id).await;

    let wl_str = if whitelist.is_empty() {
        "_Keine_".to_string()
    } else {
        whitelist
            .iter()
            .map(|u| format!("<@{}>", u))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let status_str = if cfg.enabled != 0 { "✅ Aktiv" } else { "❌ Inaktiv" };

    let embed = CreateEmbed::new()
        .title("🛡️ Anti-Nuke Konfiguration")
        .color(if cfg.enabled != 0 { 0x57F287u32 } else { 0xED4245u32 })
        .field("Status", status_str, true)
        .field("Bestrafung", &cfg.punishment, true)
        .field("Zeitfenster", format!("{} Sekunden", cfg.window_secs), true)
        .field("Kanal löschen (Max)", cfg.chan_del_max.to_string(), true)
        .field("Kanal erstellen (Max)", cfg.chan_cre_max.to_string(), true)
        .field("Rolle löschen (Max)", cfg.role_del_max.to_string(), true)
        .field("Rolle erstellen (Max)", cfg.role_cre_max.to_string(), true)
        .field("Bann (Max)", cfg.ban_max.to_string(), true)
        .field("Webhook erstellen (Max)", cfg.webhook_max.to_string(), true)
        .field("Raid-Beitritte (Max)", cfg.raid_joins.to_string(), true)
        .field("Raid-Zeitfenster", format!("{} Sekunden", cfg.raid_window), true)
        .field("Min. Kontoalter", format!("{} Tage", cfg.min_account_age_days), true)
        .field("Lockdown-Dauer", format!("{} Minuten", cfg.lockdown_mins), true)
        .field("Whitelist", wl_str, false)
        .timestamp(Timestamp::now());

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

// ── /antinuke whitelist_add ───────────────────────────────────────────────────

/// Nutzer zur Anti-Nuke Whitelist hinzufügen
#[poise::command(slash_command, guild_only)]
async fn whitelist_add(
    ctx: Context<'_>,
    #[description = "Nutzer, der zur Whitelist hinzugefügt werden soll"] user: serenity::User,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    crate::db::add_antinuke_whitelist(&ctx.data().db, guild_id, user.id).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Whitelist aktualisiert",
                &format!("<@{}> wurde zur Anti-Nuke Whitelist hinzugefügt.", user.id),
            ))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

// ── /antinuke whitelist_remove ────────────────────────────────────────────────

/// Nutzer von der Anti-Nuke Whitelist entfernen
#[poise::command(slash_command, guild_only)]
async fn whitelist_remove(
    ctx: Context<'_>,
    #[description = "Nutzer, der von der Whitelist entfernt werden soll"] user: serenity::User,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    crate::db::remove_antinuke_whitelist(&ctx.data().db, guild_id, user.id).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Whitelist aktualisiert",
                &format!("<@{}> wurde von der Anti-Nuke Whitelist entfernt.", user.id),
            ))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

// ── /antinuke set (group) ─────────────────────────────────────────────────────

/// Anti-Nuke Einstellungen anpassen
#[poise::command(
    slash_command,
    guild_only,
    subcommands(
        "threshold",
        "window",
        "punishment",
        "raid_joins",
        "raid_window",
        "min_age",
        "lockdown_duration"
    ),
    subcommand_required
)]
async fn set(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Schwellenwert für eine Aktion setzen
#[poise::command(slash_command, guild_only)]
async fn threshold(
    ctx: Context<'_>,
    #[description = "Aktion"] action: ActionChoice,
    #[description = "Schwellenwert (Anzahl Aktionen im Zeitfenster)"] count: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    let kind = action.to_kind();
    set_threshold(&mut cfg, kind, count as i64);
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Schwellenwert gesetzt",
                &format!("{:?} → max. {} Aktionen im Zeitfenster", kind, count),
            ))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

fn set_threshold(cfg: &mut AntiNukeConfig, kind: ActionKind, val: i64) {
    match kind {
        ActionKind::ChannelDelete => cfg.chan_del_max = val,
        ActionKind::ChannelCreate => cfg.chan_cre_max = val,
        ActionKind::RoleDelete    => cfg.role_del_max = val,
        ActionKind::RoleCreate    => cfg.role_cre_max = val,
        ActionKind::Ban           => cfg.ban_max = val,
        ActionKind::WebhookCreate => cfg.webhook_max = val,
    }
}

/// Zeitfenster in Sekunden setzen
#[poise::command(slash_command, guild_only)]
async fn window(
    ctx: Context<'_>,
    #[description = "Zeitfenster in Sekunden"] seconds: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.window_secs = seconds as i64;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Zeitfenster gesetzt", &format!("Zeitfenster: {} Sekunden", seconds)))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Bestrafung für Anti-Nuke Verstöße setzen
#[poise::command(slash_command, guild_only)]
async fn punishment(
    ctx: Context<'_>,
    #[description = "Bestrafung"] p: PunishmentChoice,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.punishment = p.as_str().to_string();
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Bestrafung gesetzt", &format!("Bestrafung: {}", p.as_str())))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Maximale Raid-Beitritte im Zeitfenster setzen
#[poise::command(slash_command, guild_only)]
async fn raid_joins(
    ctx: Context<'_>,
    #[description = "Anzahl Beitritte, ab der ein Raid erkannt wird"] count: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.raid_joins = count as i64;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Raid-Beitritte gesetzt", &format!("Raid-Schwelle: {} Beitritte", count)))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Raid-Zeitfenster in Sekunden setzen
#[poise::command(slash_command, guild_only)]
async fn raid_window(
    ctx: Context<'_>,
    #[description = "Zeitfenster für Raid-Erkennung in Sekunden"] seconds: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.raid_window = seconds as i64;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Raid-Zeitfenster gesetzt",
                &format!("Zeitfenster: {} Sekunden", seconds),
            ))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Minimales Kontoalter in Tagen setzen (0 = deaktiviert)
#[poise::command(slash_command, guild_only)]
async fn min_age(
    ctx: Context<'_>,
    #[description = "Minimales Kontoalter in Tagen (0 = deaktiviert)"] days: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.min_account_age_days = days as i64;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    let desc = if days == 0 {
        "Alterscheck deaktiviert.".to_string()
    } else {
        format!("Minimum: {} Tage", days)
    };
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Minimales Kontoalter gesetzt", &desc))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Lockdown-Dauer in Minuten setzen
#[poise::command(slash_command, guild_only)]
async fn lockdown_duration(
    ctx: Context<'_>,
    #[description = "Lockdown-Dauer in Minuten"] minutes: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let mut cfg = crate::db::get_or_create_antinuke_config(&ctx.data().db, guild_id).await;
    cfg.lockdown_mins = minutes as i64;
    crate::db::set_antinuke_config(&ctx.data().db, &cfg).await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok(
                "Lockdown-Dauer gesetzt",
                &format!("Dauer: {} Minuten", minutes),
            ))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

// ── /antinuke unlock ──────────────────────────────────────────────────────────

/// Lockdown sofort beenden
#[poise::command(slash_command, guild_only)]
async fn unlock(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let in_lockdown = {
        let ls = ctx.data().lockdown_state.lock().await;
        ls.contains_key(&guild_id)
    };
    if !in_lockdown {
        ctx.send(
            poise::CreateReply::default()
                .embed(info("Kein Lockdown aktiv", "Der Server befindet sich aktuell nicht im Lockdown."))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    crate::antinuke::exit_lockdown(
        ctx.serenity_context().clone(),
        ctx.data().lockdown_state.clone(),
        ctx.data().log_configs.clone(),
        guild_id,
    )
    .await;
    ctx.send(
        poise::CreateReply::default()
            .embed(ok("Lockdown beendet", "Der Server-Lockdown wurde manuell aufgehoben."))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

// ── embed helpers ─────────────────────────────────────────────────────────────

fn ok(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .description(format!("✅ **{}**\n{}", title, description))
        .color(0x57F287u32)
}

fn info(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .description(format!("ℹ️ **{}**\n{}", title, description))
        .color(0x5865F2u32)
}
