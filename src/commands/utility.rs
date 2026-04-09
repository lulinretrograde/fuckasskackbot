use std::time::Instant;
use poise::serenity_prelude as serenity;
use serenity::{CreateActionRow, CreateButton, CreateEmbed, CreateMessage};
use crate::{Context, Error};

// ── emoji parsing ─────────────────────────────────────────────────────────────

fn parse_emojis(input: &str) -> Vec<(bool, String, u64)> {
    let mut result = Vec::new();
    let mut s = input;
    while let Some(start) = s.find('<') {
        s = &s[start..];
        let Some(end) = s.find('>') else { break };
        let inner = &s[1..end];
        let (animated, rest) = if inner.starts_with("a:") {
            (true, &inner[2..])
        } else if inner.starts_with(':') {
            (false, &inner[1..])
        } else {
            s = &s[end + 1..];
            continue;
        };
        if let Some(colon) = rest.rfind(':') {
            let name = &rest[..colon];
            if let Ok(id) = rest[colon + 1..].parse::<u64>() {
                result.push((animated, name.to_string(), id));
            }
        }
        s = &s[end + 1..];
    }
    result
}

fn base64_encode(data: &[u8]) -> String {
    const C: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(C[b0 >> 2] as char);
        out.push(C[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 { C[((b1 & 0xf) << 2) | (b2 >> 6)] as char } else { '=' });
        out.push(if chunk.len() > 2 { C[b2 & 0x3f] as char } else { '=' });
    }
    out
}

async fn download(url: &str) -> Option<Vec<u8>> {
    reqwest::get(url).await.ok()?.bytes().await.ok().map(|b| b.to_vec())
}

// ── /stealemoji ───────────────────────────────────────────────────────────────

/// Einen oder mehrere Custom-Emojis von anderen Servern auf diesen Server kopieren.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_EMOJIS_AND_STICKERS",
    rename = "stealemoji"
)]
pub async fn stealemoji(
    ctx: Context<'_>,
    #[description = "Custom-Emojis einfügen (z.B. :kek: :based: mehrere auf einmal möglich)"]
    emojis: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let parsed = parse_emojis(&emojis);

    if parsed.is_empty() {
        ctx.say("❌ Keine gültigen Custom-Emojis gefunden. Kopiere einfach Custom-Emojis direkt in das Feld.")
            .await?;
        return Ok(());
    }

    let mut added: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    for (animated, name, id) in parsed {
        let ext = if animated { "gif" } else { "png" };
        let url = format!("https://cdn.discordapp.com/emojis/{}.{}?size=96&quality=lossless", id, ext);

        let Some(bytes) = download(&url).await else {
            failed.push(format!("`{}` (Download fehlgeschlagen)", name));
            continue;
        };

        let mime = if animated { "image/gif" } else { "image/png" };
        let image = format!("data:{};base64,{}", mime, base64_encode(&bytes));

        match guild_id.create_emoji(ctx.http(), &name, &image).await {
            Ok(e) => added.push(format!("<{}:{}:{}>",
                if animated { "a" } else { "" }, e.name, e.id)),
            Err(e) => failed.push(format!("`{}` ({})", name, e)),
        }
    }

    let mut msg = String::new();
    if !added.is_empty() {
        msg.push_str(&format!("✅ **Hinzugefügt:** {}\n", added.join("  ")));
    }
    if !failed.is_empty() {
        msg.push_str(&format!("❌ **Fehlgeschlagen:** {}", failed.join(", ")));
    }
    ctx.say(msg.trim()).await?;
    Ok(())
}

// ── /stealsticker ─────────────────────────────────────────────────────────────

/// Einen Sticker aus einer Nachricht auf diesen Server kopieren.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_EMOJIS_AND_STICKERS",
    rename = "stealsticker"
)]
pub async fn stealsticker(
    ctx: Context<'_>,
    #[description = "Nachrichtenlink der Nachricht mit dem Sticker"]
    nachricht: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();

    // Parse https://discord.com/channels/{guild}/{channel}/{message}
    let segments: Vec<&str> = nachricht.trim_end_matches('/').rsplit('/').take(3).collect();
    let (msg_id, ch_id) = match segments.as_slice() {
        [m, c, _g] => match (m.parse::<u64>(), c.parse::<u64>()) {
            (Ok(m), Ok(c)) => (m, c),
            _ => {
                ctx.say("❌ Ungültiger Nachrichtenlink.").await?;
                return Ok(());
            }
        },
        _ => {
            ctx.say("❌ Bitte einen gültigen Nachrichtenlink einfügen.").await?;
            return Ok(());
        }
    };

    let msg = match serenity::ChannelId::new(ch_id)
        .message(ctx.http(), serenity::MessageId::new(msg_id))
        .await
    {
        Ok(m) => m,
        Err(_) => {
            ctx.say("❌ Nachricht nicht gefunden. Der Bot muss Zugriff auf den Kanal haben.").await?;
            return Ok(());
        }
    };

    if msg.sticker_items.is_empty() {
        ctx.say("❌ Diese Nachricht enthält keinen Sticker.").await?;
        return Ok(());
    }

    let sticker = &msg.sticker_items[0];

    if sticker.format_type == serenity::StickerFormatType::Lottie {
        ctx.say("❌ Lottie-Sticker können leider nicht kopiert werden (Discord-Einschränkung).").await?;
        return Ok(());
    }

    let ext = match sticker.format_type {
        serenity::StickerFormatType::Gif  => "gif",
        _                                  => "png",
    };

    let url = format!("https://media.discordapp.net/stickers/{}.{}?size=320", sticker.id, ext);
    let Some(bytes) = download(&url).await else {
        ctx.say("❌ Sticker-Bild konnte nicht heruntergeladen werden.").await?;
        return Ok(());
    };

    let filename = format!("{}.{}", sticker.name, ext);
    let attachment = serenity::CreateAttachment::bytes(bytes, filename);
    let builder = serenity::CreateSticker::new(sticker.name.clone(), attachment)
        .tags("⭐")
        .description(sticker.name.clone());

    match guild_id.create_sticker(ctx.http(), builder).await {
        Ok(s) => { ctx.say(format!("✅ Sticker **{}** wurde hinzugefügt!", s.name)).await?; }
        Err(e) => { ctx.say(format!("❌ Fehler: {}", e)).await?; }
    }

    Ok(())
}

// ── /bug + ticket system ─────────────────────────────────────────────────────

pub const OWNER_ID: u64 = 598134897265082398;

/// Einen Bug oder ein Problem melden.
#[poise::command(slash_command, rename = "bug")]
pub async fn bug(
    ctx: Context<'_>,
    #[description = "Beschreibe den Bug so genau wie möglich"]
    erklärung: String,
) -> Result<(), Error> {
    let user = ctx.author();
    let is_owner = user.id.get() == OWNER_ID;

    // Rate limit (owner exempt)
    if !is_owner {
        let mut cooldowns = ctx.data().bug_cooldowns.lock().await;
        if let Some(&last) = cooldowns.get(&user.id) {
            let elapsed = last.elapsed().as_secs();
            if elapsed < crate::config::BUG_COOLDOWN_SECS {
                let remaining = crate::config::BUG_COOLDOWN_SECS - elapsed;
                ctx.send(
                    poise::CreateReply::default()
                        .content(format!(
                            "❌ Du hast bereits einen Bug gemeldet. Bitte warte noch **{} Sekunden**.",
                            remaining
                        ))
                        .ephemeral(true),
                ).await?;
                return Ok(());
            }
        }
        cooldowns.insert(user.id, Instant::now());
    }

    let guild_id = ctx.guild_id().map(|g| g.get()).unwrap_or(0);
    let reward   = crate::db::get_ticket_reward(&ctx.data().db).await;

    // Create ticket in DB
    let ticket_id = crate::db::insert_ticket(
        &ctx.data().db, user.id, guild_id, &erklärung, reward,
    ).await;

    // DM owner
    let owner = serenity::UserId::new(OWNER_ID);
    let Ok(dm_ch) = owner.create_dm_channel(ctx.http()).await else {
        ctx.send(poise::CreateReply::default()
            .content("❌ Der Report konnte nicht gesendet werden. Bitte versuche es später erneut.")
            .ephemeral(true)).await?;
        return Ok(());
    };

    let guild_label = if guild_id != 0 {
        format!("Server `{}`", guild_id)
    } else {
        "DM".to_string()
    };

    let embed = CreateEmbed::new()
        .title(format!("🐛 Ticket #{}", ticket_id))
        .description(&erklärung)
        .field("Reporter", format!("<@{}> (`{}`)", user.id, user.name), true)
        .field("Herkunft", guild_label, true)
        .field("Status", "🟡 Offen", true)
        .color(0xFEE75Cu32);

    let channel_disabled = guild_id == 0;
    let msg = dm_ch.send_message(ctx.http(), CreateMessage::new()
        .embed(embed)
        .components(vec![CreateActionRow::Buttons(vec![
            CreateButton::new(format!("tr_{}", ticket_id))
                .label(format!("✅ Erledigt (+{} Coins)", reward))
                .style(serenity::ButtonStyle::Success),
            CreateButton::new(format!("td_{}", ticket_id))
                .label("❌ Ablehnen")
                .style(serenity::ButtonStyle::Danger),
            CreateButton::new(format!("tc_{}", ticket_id))
                .label("💬 Kanal erstellen")
                .style(serenity::ButtonStyle::Primary)
                .disabled(channel_disabled),
        ])])
    ).await?;

    crate::db::update_ticket_dm(
        &ctx.data().db, ticket_id,
        dm_ch.id.get() as i64,
        msg.id.get() as i64,
    ).await;

    ctx.send(poise::CreateReply::default()
        .content(
            "✅ **Dein Bug-Report wurde eingereicht.**\n\n\
            Der Entwickler wurde benachrichtigt und prüft dein Ticket. \
            Du wirst per DM informiert sobald eine Entscheidung getroffen wurde.\n\n\
            **Mögliche Ausgänge:**\n\
            - **Erledigt**: der Bug wurde behoben, du erhältst eine Belohnung in Coins\n\
            - **Abgelehnt**: der Report wurde nicht als Bug eingestuft\n\
            - **Kanal**: ein privater Kanal wird für euch erstellt um den Bug gemeinsam zu besprechen"
        )
        .ephemeral(true),
    ).await?;

    Ok(())
}

/// Coin-Belohnung für erledigte Bug-Tickets setzen.
#[poise::command(slash_command, rename = "ticket-reward")]
pub async fn ticket_reward(
    ctx: Context<'_>,
    #[description = "Neue Belohnung in Coins"] amount: i64,
) -> Result<(), Error> {
    if ctx.author().id.get() != OWNER_ID {
        ctx.send(poise::CreateReply::default()
            .content("❌ Nur der Bot-Entwickler kann das ändern.")
            .ephemeral(true)).await?;
        return Ok(());
    }
    crate::db::set_ticket_reward(&ctx.data().db, amount).await;
    ctx.send(poise::CreateReply::default()
        .content(format!("✅ Ticket-Belohnung auf **{} Coins** gesetzt.", amount))
        .ephemeral(true)).await?;
    Ok(())
}
