use rand::Rng;
use chrono::Utc;

use poise::serenity_prelude as serenity;
use serenity::{
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateInteractionResponse, CreateInteractionResponseMessage,
};

use crate::{Context, Error};

// ── fish data ─────────────────────────────────────────────────────────────────

pub struct FishKind {
    pub id:         &'static str,
    pub name:       &'static str,
    pub emoji:      &'static str,
    pub base_price: i64,
    pub price_min:  i64,
    pub price_max:  i64,
}

pub static FISH: &[FishKind] = &[
    FishKind { id: "muell",         name: "Plastikmüll",                  emoji: "🗑️", base_price: 0,   price_min: 0,   price_max: 0   },
    FishKind { id: "hering",        name: "Kleiner Hering",               emoji: "🐟", base_price: 10,  price_min: 6,   price_max: 18  },
    FishKind { id: "forelle",       name: "Bachforelle",                  emoji: "🐠", base_price: 25,  price_min: 15,  price_max: 45  },
    FishKind { id: "barsch",        name: "Flussbarsch",                  emoji: "🐡", base_price: 50,  price_min: 30,  price_max: 80  },
    FishKind { id: "hecht",         name: "Mächtiger Hecht",              emoji: "🦈", base_price: 120, price_min: 70,  price_max: 200 },
    FishKind { id: "goldfisch",     name: "Goldener Karpfen",             emoji: "✨", base_price: 250, price_min: 150, price_max: 400 },
    FishKind { id: "quantenbarsch", name: "Der Urzeitliche Quantenbarsch",emoji: "🌌", base_price: 500, price_min: 500, price_max: 500 },
];

pub fn find_fish(id: &str) -> Option<&'static FishKind> {
    FISH.iter().find(|f| f.id == id)
}

// ── rod data ──────────────────────────────────────────────────────────────────

pub struct Rod {
    pub id:      &'static str,
    pub name:    &'static str,
    pub price:   i64,
    pub emoji:   &'static str,
    pub desc:    &'static str,
    /// Probability weights for each FISH index (must sum to 1000)
    pub weights: [u32; 7],
}

pub static RODS: &[Rod] = &[
    Rod {
        id: "grundangel", name: "Grundangel", price: 0, emoji: "🎣",
        desc: "Die gute alte Grundangel. Kostenlos und zweckmäßig.",
        //       müll  hering forelle barsch hecht gold  quantum
        weights: [250,  380,   220,    100,   40,   9,    1],
    },
    Rod {
        id: "profiangel", name: "Profiangel", price: 500, emoji: "🎣",
        desc: "Bessere Chancen auf seltene Fische.",
        weights: [150,  300,   270,    170,   80,   25,   5],
    },
    Rod {
        id: "meeresangel", name: "Meeresangel", price: 2000, emoji: "🎣",
        desc: "Speziell für schwer fangbare Tiefsee-Arten.",
        weights: [80,   220,   260,    230,   140,  55,   15],
    },
    Rod {
        id: "quantenangel", name: "Quantenangel", price: 10000, emoji: "🎣",
        desc: "Überwindet Raum und Zeit. Erhöht die Chance auf den Quantenbarsch enorm.",
        weights: [30,   140,   210,    250,   200,  130,  40],
    },
];

pub fn find_rod(id: &str) -> Option<&'static Rod> {
    RODS.iter().find(|r| r.id == id)
}

fn roll_fish(rod: &Rod) -> &'static FishKind {
    let roll: u32 = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..1000)
    };
    let mut cumulative = 0u32;
    for (i, &w) in rod.weights.iter().enumerate() {
        cumulative += w;
        if roll < cumulative {
            return &FISH[i];
        }
    }
    &FISH[1]
}

// ── market price helpers ───────────────────────────────────────────────────────

/// Called once at startup and then hourly.
pub async fn refresh_market_prices(pool: &sqlx::SqlitePool) {
    let now = Utc::now().timestamp();
    for fish in FISH {
        if fish.price_min == fish.price_max {
            crate::db::set_fish_price(pool, fish.id, fish.base_price, now).await;
            continue;
        }
        let price: i64 = {
            let mut rng = rand::thread_rng();
            rng.gen_range(fish.price_min..=fish.price_max)
        };
        crate::db::set_fish_price(pool, fish.id, price, now).await;
    }
}

async fn current_price(pool: &sqlx::SqlitePool, fish: &FishKind) -> i64 {
    crate::db::get_fish_price(pool, fish.id).await.unwrap_or(fish.base_price)
}

// ── /angeln ───────────────────────────────────────────────────────────────────

const FISH_COOLDOWN_SECS: i64 = 300;

/// Mit der Angel einen Fisch fangen. Cooldown: 5 Minuten.
#[poise::command(slash_command, guild_only, rename = "angeln")]
pub async fn angeln(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let now      = Utc::now().timestamp();

    if let Some(last) = crate::db::get_fishing_cooldown(&ctx.data().db, guild_id, user_id).await {
        let elapsed = now - last;
        if elapsed < FISH_COOLDOWN_SECS {
            let remaining = FISH_COOLDOWN_SECS - elapsed;
            let mins = remaining / 60;
            let secs = remaining % 60;
            ctx.send(poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🎣 Noch nicht fertig…")
                    .description(format!(
                        "Deine Angel ist noch im Wasser. Warte noch **{}:{:02}** Minuten.",
                        mins, secs,
                    ))
                    .color(0xED4245u32),
            )).await?;
            return Ok(());
        }
    }

    let rod_id = crate::db::get_fishing_rod(&ctx.data().db, guild_id, user_id).await;
    let rod    = find_rod(&rod_id).unwrap_or(&RODS[0]);
    let fish   = roll_fish(rod);

    crate::db::set_fishing_cooldown(&ctx.data().db, guild_id, user_id, now).await;
    crate::db::add_fish_to_inventory(&ctx.data().db, guild_id, user_id, fish.id, now).await;

    let price = current_price(&ctx.data().db, fish).await;

    let (title, desc, color) = if fish.id == "muell" {
        (
            "Du hast… Müll gefangen.".to_string(),
            format!("{} **{}**\nDas ist halt was. Du hast es ins Inventar gelegt.", fish.emoji, fish.name),
            0x99AAB5u32,
        )
    } else if fish.id == "quantenbarsch" {
        (
            "🌌 LEGENDÄRER FANG!".to_string(),
            format!(
                "{} **{}**\nEin Fisch aus einer anderen Dimension! Aktueller Marktwert: **{} Coins**.",
                fish.emoji, fish.name, price,
            ),
            0xFFD700u32,
        )
    } else {
        (
            "Du hast etwas gefangen!".to_string(),
            format!(
                "{} **{}**\nLandete in deinem Inventar. Aktueller Marktwert: **{} Coins**.",
                fish.emoji, fish.name, price,
            ),
            0x57F287u32,
        )
    };

    let ready_at = now + FISH_COOLDOWN_SECS;
    let remind_btn = CreateButton::new(format!("remind_fish_{}_{}", user_id, ready_at))
        .label("🔔 In 5 Min erinnern")
        .style(serenity::ButtonStyle::Secondary);

    ctx.send(
        poise::CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title(title)
                    .description(desc)
                    .color(color)
                    .footer(CreateEmbedFooter::new(format!(
                        "Rute: {} {} | /inventar anzeigen | /alles-verkaufen",
                        rod.emoji, rod.name,
                    ))),
            )
            .components(vec![CreateActionRow::Buttons(vec![remind_btn])]),
    ).await?;

    Ok(())
}

// ── /inventar ─────────────────────────────────────────────────────────────────

const PAGE_SIZE: usize = 5;

/// Dein Fischinventar anzeigen — blätterbar mit Buttons.
#[poise::command(slash_command, guild_only, rename = "inventar")]
pub async fn inventar(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let entries = crate::db::get_fish_inventory(pool, guild_id, user_id).await;

    if entries.is_empty() {
        ctx.send(poise::CreateReply::default()
            .embed(CreateEmbed::new()
                .title("🎣 Inventar")
                .description("Dein Inventar ist leer. Zeit zum Angeln!")
                .color(0x5865F2u32))
            .ephemeral(true),
        ).await?;
        return Ok(());
    }

    let mut page = 0usize;
    let total_pages = pages(entries.len());

    let (embed, components) = build_page(pool, &entries, page, total_pages, None).await;
    let handle = ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(components)
            .ephemeral(true),
    ).await?;

    let msg = handle.message().await?;

    while let Some(interaction) = msg
        .await_component_interaction(ctx.serenity_context())
        .timeout(std::time::Duration::from_secs(120))
        .await
    {
        let custom_id = interaction.data.custom_id.clone();
        let mut sold_line: Option<String> = None;

        if let Some(id_str) = custom_id.strip_prefix("sell_") {
            if let Ok(entry_id) = id_str.parse::<i64>() {
                let current = crate::db::get_fish_inventory(pool, guild_id, user_id).await;
                if let Some(entry) = current.iter().find(|e| e.id == entry_id) {
                    if let Some(fish) = find_fish(&entry.fish_id) {
                        let price = current_price(pool, fish).await;
                        crate::db::remove_fish_from_inventory(pool, entry_id).await;
                        if price > 0 {
                            crate::db::add_coins(pool, guild_id, user_id, price).await;
                        }
                        sold_line = Some(format!(
                            "✅ {} **{}** für **{} Coins** verkauft!",
                            fish.emoji, fish.name, price,
                        ));
                    }
                }
            }
        } else if custom_id == "inv_prev" && page > 0 {
            page -= 1;
        } else if custom_id == "inv_next" {
            page += 1;
        }

        let entries = crate::db::get_fish_inventory(pool, guild_id, user_id).await;
        let tp = pages(entries.len());
        if page >= tp && page > 0 { page = tp - 1; }

        if entries.is_empty() {
            interaction.create_response(ctx.http(),
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .embed(CreateEmbed::new()
                            .title("🎣 Inventar")
                            .description("Inventar ist leer.")
                            .color(0x5865F2u32))
                        .components(vec![]),
                ),
            ).await.ok();
            break;
        }

        let (embed, components) = build_page(pool, &entries, page, tp, sold_line).await;
        interaction.create_response(ctx.http(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components),
            ),
        ).await.ok();
    }

    Ok(())
}

fn pages(len: usize) -> usize {
    if len == 0 { 1 } else { (len + PAGE_SIZE - 1) / PAGE_SIZE }
}

async fn build_page(
    pool: &sqlx::SqlitePool,
    entries: &[crate::db::FishEntry],
    page: usize,
    total_pages: usize,
    status: Option<String>,
) -> (CreateEmbed, Vec<CreateActionRow>) {
    let start = page * PAGE_SIZE;
    let slice = &entries[start..(start + PAGE_SIZE).min(entries.len())];

    let mut desc = if let Some(s) = status {
        format!("{}\n\n", s)
    } else {
        String::new()
    };
    desc.push_str(&format!(
        "**Seite {}/{}** — {} Fische insgesamt\n\n",
        page + 1, total_pages, entries.len()
    ));

    let mut sell_buttons = Vec::new();
    for entry in slice {
        let (emoji, name, price) = if let Some(f) = find_fish(&entry.fish_id) {
            (f.emoji, f.name.to_string(), current_price(pool, f).await)
        } else {
            ("❓", entry.fish_id.clone(), 0)
        };
        desc.push_str(&format!(
            "{} **{}** — {} Coins *(gefangen <t:{}:R>)*\n",
            emoji, name, price, entry.caught_at,
        ));
        sell_buttons.push(
            CreateButton::new(format!("sell_{}", entry.id))
                .label(format!("Verkaufen ({}C)", price))
                .style(serenity::ButtonStyle::Success),
        );
    }

    let mut components = Vec::new();
    if !sell_buttons.is_empty() {
        components.push(CreateActionRow::Buttons(sell_buttons));
    }
    components.push(CreateActionRow::Buttons(vec![
        CreateButton::new("inv_prev")
            .label("◀ Zurück")
            .style(serenity::ButtonStyle::Secondary)
            .disabled(page == 0),
        CreateButton::new("inv_next")
            .label("Weiter ▶")
            .style(serenity::ButtonStyle::Secondary)
            .disabled(page + 1 >= total_pages),
    ]));

    let embed = CreateEmbed::new()
        .title("🎣 Fischinventar")
        .description(desc)
        .color(0x5865F2u32)
        .footer(CreateEmbedFooter::new(
            "Klicke auf einen Button um den Fisch zu verkaufen",
        ));

    (embed, components)
}

// ── /alles-verkaufen ──────────────────────────────────────────────────────────

/// Alle Fische im Inventar zum aktuellen Marktpreis verkaufen.
#[poise::command(slash_command, guild_only, rename = "alles-verkaufen")]
pub async fn alles_verkaufen(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let entries = crate::db::remove_all_fish(pool, guild_id, user_id).await;

    if entries.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("Inventar leer")
                .description("Du hast keine Fische zu verkaufen.")
                .color(0x5865F2u32),
        ).ephemeral(true)).await?;
        return Ok(());
    }

    let mut total_coins = 0i64;
    let mut sold_lines = Vec::new();

    for entry in &entries {
        if let Some(fish) = find_fish(&entry.fish_id) {
            let price = current_price(pool, fish).await;
            total_coins += price;
            sold_lines.push(format!("{} {} — {} Coins", fish.emoji, fish.name, price));
        }
    }

    if total_coins > 0 {
        crate::db::add_coins(pool, guild_id, user_id, total_coins).await;
    }

    let desc = if sold_lines.len() <= 15 {
        sold_lines.join("\n")
    } else {
        format!("{}\n…und {} weitere", sold_lines[..15].join("\n"), sold_lines.len() - 15)
    };

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("Alles verkauft!")
            .description(desc)
            .field("Gesamterlös", format!("**{} Coins**", total_coins), false)
            .color(0x57F287u32),
    ).ephemeral(true)).await?;

    Ok(())
}

// ── /fischmarkt ───────────────────────────────────────────────────────────────

/// Aktuelle Fischmarktpreise anzeigen.
#[poise::command(slash_command, guild_only, rename = "fischmarkt")]
pub async fn fischmarkt(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let pool = &ctx.data().db;
    let mut lines = Vec::new();

    for fish in FISH {
        if fish.id == "muell" { continue; }
        let price = current_price(pool, fish).await;
        let trend = if price > fish.base_price { "📈" } else if price < fish.base_price { "📉" } else { "➡️" };
        lines.push(format!(
            "{} {} **{}** — {} Coins",
            trend, fish.emoji, fish.name, price,
        ));
    }

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("🐟 Fischmarkt — Aktuelle Preise")
            .description(lines.join("\n"))
            .color(0x5865F2u32)
            .footer(CreateEmbedFooter::new("Preise ändern sich stündlich")),
    )).await?;

    Ok(())
}

// ── /angelshop ────────────────────────────────────────────────────────────────

/// Angel-Shop: verfügbare Ruten anzeigen.
#[poise::command(slash_command, guild_only, rename = "angelshop")]
pub async fn angelshop(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let owned    = crate::db::get_fishing_rod(&ctx.data().db, guild_id, user_id).await;

    let mut lines = Vec::new();
    for rod in RODS {
        let status = if rod.id == owned {
            "✅ **Aktiv**"
        } else if rod.price == 0 {
            "Kostenlos"
        } else {
            "Kaufbar"
        };
        let price_str = if rod.price == 0 {
            "Kostenlos".to_string()
        } else {
            format!("{} Coins", rod.price)
        };
        lines.push(format!(
            "{} **{}** — {}\n{}\n*{}*",
            rod.emoji, rod.name, price_str, rod.desc, status,
        ));
    }

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("🎣 Angel-Shop")
            .description(lines.join("\n\n"))
            .color(0x5865F2u32)
            .footer(CreateEmbedFooter::new("Nutze /rute-kaufen um eine Rute zu kaufen")),
    ).ephemeral(true)).await?;

    Ok(())
}

// ── /rute-kaufen ──────────────────────────────────────────────────────────────

#[derive(Debug, poise::ChoiceParameter)]
pub enum RuteWahl {
    #[name = "Profiangel (500 Coins)"]
    Profiangel,
    #[name = "Meeresangel (2000 Coins)"]
    Meeresangel,
    #[name = "Quantenangel (10000 Coins)"]
    Quantenangel,
}

/// Eine bessere Angelrute kaufen.
#[poise::command(slash_command, guild_only, rename = "rute-kaufen")]
pub async fn rute_kaufen(
    ctx: Context<'_>,
    #[description = "Welche Rute möchtest du kaufen?"] rute: RuteWahl,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let rod_id = match rute {
        RuteWahl::Profiangel   => "profiangel",
        RuteWahl::Meeresangel  => "meeresangel",
        RuteWahl::Quantenangel => "quantenangel",
    };
    let rod = find_rod(rod_id).unwrap();

    let current_rod = crate::db::get_fishing_rod(pool, guild_id, user_id).await;
    if current_rod == rod_id {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("Bereits besessen")
                .description(format!("Du hast die **{}** bereits.", rod.name))
                .color(0xFEE75Cu32),
        ).ephemeral(true)).await?;
        return Ok(());
    }

    let current_idx = RODS.iter().position(|r| r.id == current_rod).unwrap_or(0);
    let new_idx     = RODS.iter().position(|r| r.id == rod_id).unwrap_or(0);
    if new_idx <= current_idx {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("Kein Downgrade möglich")
                .description("Du kannst nicht auf eine schlechtere Rute wechseln.")
                .color(0xED4245u32),
        ).ephemeral(true)).await?;
        return Ok(());
    }

    let balance = crate::db::get_coins(pool, guild_id, user_id).await;
    if balance < rod.price {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("Zu wenig Coins")
                .description(format!(
                    "Du brauchst **{} Coins** für die **{}**, hast aber nur **{} Coins**.",
                    rod.price, rod.name, balance,
                ))
                .color(0xED4245u32),
        ).ephemeral(true)).await?;
        return Ok(());
    }

    crate::db::add_coins(pool, guild_id, user_id, -rod.price).await;
    crate::db::set_fishing_rod(pool, guild_id, user_id, rod_id).await;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("Rute gekauft!")
            .description(format!(
                "{} Du hast die **{}** für **{} Coins** gekauft!\n{}",
                rod.emoji, rod.name, rod.price, rod.desc,
            ))
            .color(0x57F287u32),
    ).ephemeral(true)).await?;

    Ok(())
}
