use chrono::Utc;
use rand::seq::SliceRandom;

use poise::serenity_prelude as serenity;
use serenity::{
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateMessage, EditMessage,
};

use crate::{Context, Error};

// ── embed builder ─────────────────────────────────────────────────────────────

pub fn giveaway_embed(
    prize:          &str,
    ticket_price:   i64,
    required_level: i64,
    ends_at:        i64,
    entries:        i64,
    ended:          bool,
    winner:         Option<serenity::UserId>,
) -> CreateEmbed {
    let price_str = if ticket_price == 0 {
        "Kostenlos".to_string()
    } else {
        format!("{} Coins", ticket_price)
    };
    let level_str = if required_level == 0 {
        "Kein".to_string()
    } else {
        format!("Level {}", required_level)
    };

    if ended {
        let winner_line = match winner {
            Some(id) => format!("<@{}>", id),
            None     => "Niemand hat teilgenommen.".to_string(),
        };
        CreateEmbed::new()
            .title("🎉 Gewinnspiel beendet!")
            .description(format!("**Preis:** {}", prize))
            .field("🏆 Gewinner",     winner_line,          false)
            .field("🎟️ Teilnehmer",  entries.to_string(),  true)
            .field("💰 Ticketpreis", price_str,             true)
            .color(0x57F287u32)
    } else {
        CreateEmbed::new()
            .title("🎉 Gewinnspiel!")
            .description(format!("**Preis:** {}", prize))
            .field("💰 Ticketpreis",   price_str,                        true)
            .field("📊 Mindest-Level", level_str,                        true)
            .field("🎟️ Teilnehmer",   entries.to_string(),              true)
            .field("⏰ Endet",         format!("<t:{}:R>", ends_at),     false)
            .color(0x5865F2u32)
            .footer(CreateEmbedFooter::new("Klicke auf den Button um teilzunehmen"))
    }
}

pub fn join_button(disabled: bool) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new("giveaway_join")
            .label("🎟️ Teilnehmen")
            .style(serenity::ButtonStyle::Primary)
            .disabled(disabled),
    ])
}

// ── background end task ───────────────────────────────────────────────────────

pub fn schedule_giveaway_end(
    ctx:         serenity::Context,
    pool:        sqlx::SqlitePool,
    giveaway_id: i64,
    ends_at:     i64,
) {
    tokio::spawn(async move {
        let now   = Utc::now().timestamp();
        let delay = (ends_at - now).max(0) as u64;
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        finish_giveaway(&ctx, &pool, giveaway_id).await;
    });
}

pub async fn finish_giveaway(
    ctx:         &serenity::Context,
    pool:        &sqlx::SqlitePool,
    giveaway_id: i64,
) {
    // Guard against double-finish (e.g. after restart reschedule)
    let row = sqlx::query(
        "SELECT ended FROM giveaways WHERE id = ?",
    )
    .bind(giveaway_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    use sqlx::Row as _;
    if let Some(r) = &row {
        if r.get::<i64, _>("ended") == 1 { return; }
    } else {
        return;
    }

    let entries = crate::db::get_giveaway_entries(pool, giveaway_id).await;
    let count   = entries.len() as i64;

    let winner: Option<serenity::UserId> = {
        let mut rng = rand::thread_rng();
        entries.choose(&mut rng).copied()
    };

    crate::db::end_giveaway(pool, giveaway_id, winner).await;

    // Fetch full giveaway row
    let row = sqlx::query(
        "SELECT guild_id, channel_id, message_id, prize, ticket_price, required_level, ends_at
         FROM giveaways WHERE id = ?",
    )
    .bind(giveaway_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let Some(row) = row else { return };

    let guild_id:       serenity::GuildId   = serenity::GuildId::new(row.get::<i64, _>("guild_id") as u64);
    let channel_id:     serenity::ChannelId = serenity::ChannelId::new(row.get::<i64, _>("channel_id") as u64);
    let msg_id_raw:     Option<i64>         = row.get("message_id");
    let prize:          String              = row.get("prize");
    let ticket_price:   i64                 = row.get("ticket_price");
    let required_level: i64                 = row.get("required_level");
    let ends_at:        i64                 = row.get("ends_at");

    // Edit the original message to show the result
    if let Some(raw) = msg_id_raw {
        let message_id = serenity::MessageId::new(raw as u64);
        let embed = giveaway_embed(&prize, ticket_price, required_level, ends_at, count, true, winner);
        channel_id.edit_message(
            &ctx.http,
            message_id,
            EditMessage::new().embed(embed).components(vec![join_button(true)]),
        ).await.ok();
    }

    // Public announcement
    let announcement = match winner {
        Some(id) => format!(
            "🎉 Herzlichen Glückwunsch <@{}>! Du hast **{}** gewonnen!",
            id, prize,
        ),
        None => format!(
            "Das Gewinnspiel für **{}** ist beendet — niemand hat teilgenommen.",
            prize,
        ),
    };
    channel_id.send_message(&ctx.http, CreateMessage::new().content(&announcement)).await.ok();

    // DM the winner
    if let Some(winner_id) = winner {
        let server_name = guild_id.name(&ctx.cache).unwrap_or_else(|| "dem Server".to_string());
        if let Ok(dm) = winner_id.create_dm_channel(&ctx.http).await {
            dm.send_message(
                &ctx.http,
                CreateMessage::new().embed(
                    CreateEmbed::new()
                        .title("🎉 Du hast gewonnen!")
                        .description(format!(
                            "Glückwunsch! Du hast das Gewinnspiel auf **{}** gewonnen!\n\n**Preis:** {}",
                            server_name, prize,
                        ))
                        .color(0xFFD700u32),
                ),
            ).await.ok();
        }
    }
}

// ── /giveaway command ─────────────────────────────────────────────────────────

/// Ein Gewinnspiel starten.
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR",
    guild_only,
    rename = "giveaway"
)]
pub async fn giveaway(
    ctx: Context<'_>,
    #[description = "Was wird verlost?"] preis: String,
    #[description = "Wie viele Minuten läuft das Gewinnspiel?"] dauer_minuten: u32,
    #[description = "Ticketpreis in Coins (0 = kostenlos)"] ticketpreis: Option<i64>,
    #[description = "Mindest-Level zum Teilnehmen (0 = keins)"] mindest_level: Option<i64>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id     = ctx.guild_id().unwrap();
    let channel_id   = ctx.channel_id();
    let ticket_price = ticketpreis.unwrap_or(0).max(0);
    let required_lvl = mindest_level.unwrap_or(0).max(0);
    let now          = Utc::now().timestamp();
    let ends_at      = now + (dauer_minuten as i64) * 60;
    let pool         = &ctx.data().db;

    let giveaway_id = crate::db::create_giveaway(
        pool, guild_id, channel_id, &preis, ticket_price, required_lvl, ends_at,
    ).await;

    let embed = giveaway_embed(&preis, ticket_price, required_lvl, ends_at, 0, false, None);
    let msg = channel_id.send_message(
        ctx.http(),
        CreateMessage::new()
            .embed(embed)
            .components(vec![join_button(false)]),
    ).await?;

    crate::db::set_giveaway_message(pool, giveaway_id, msg.id).await;

    schedule_giveaway_end(
        ctx.serenity_context().clone(),
        pool.clone(),
        giveaway_id,
        ends_at,
    );

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("Gewinnspiel gestartet!")
            .description(format!(
                "Das Gewinnspiel für **{}** läuft jetzt für **{} Minuten**.",
                preis, dauer_minuten,
            ))
            .color(0x57F287u32),
    )).await?;

    Ok(())
}
