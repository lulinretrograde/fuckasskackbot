use rand::Rng;
use rand::seq::SliceRandom;
use chrono::Utc;

use poise::serenity_prelude as serenity;
use serenity::{
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
};

use crate::{Context, Error};

// ── constants ─────────────────────────────────────────────────────────────────

const MIN_BET: i64 = 10;
const MAX_BET: i64 = 10_000;
const LOTTO_TICKET_PRICE: i64 = 100;
const CONSOLATION_STREAK: i64 = 5;
const CONSOLATION_COINS:  i64 = 150;

// ── channel guard ─────────────────────────────────────────────────────────────

async fn in_casino_channel(ctx: &Context<'_>) -> bool {
    let guild_id = ctx.guild_id().unwrap();
    let allowed = crate::db::get_casino_channel(&ctx.data().db, guild_id).await;
    if let Some(ch) = allowed {
        if ctx.channel_id() != ch {
            ctx.send(poise::CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("Falscher Kanal")
                    .description(format!("Casino-Befehle sind nur in <#{}> erlaubt.", ch))
                    .color(0xED4245u32))
                .ephemeral(true),
            ).await.ok();
            return false;
        }
    }
    true
}

// ── bet validation ────────────────────────────────────────────────────────────

async fn validate_bet(ctx: &Context<'_>, bet: i64) -> bool {
    if bet < MIN_BET || bet > MAX_BET {
        ctx.send(poise::CreateReply::default()
            .embed(CreateEmbed::new()
                .title("Ungültiger Einsatz")
                .description(format!("Einsatz muss zwischen **{} und {} Coins** liegen.", MIN_BET, MAX_BET))
                .color(0xED4245u32))
            .ephemeral(true),
        ).await.ok();
        return false;
    }
    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let balance  = crate::db::get_coins(&ctx.data().db, guild_id, user_id).await;
    if balance < bet {
        ctx.send(poise::CreateReply::default()
            .embed(CreateEmbed::new()
                .title("Nicht genug Coins")
                .description(format!("Du hast **{} Coins**, brauchst aber **{}**.", balance, bet))
                .color(0xED4245u32))
            .ephemeral(true),
        ).await.ok();
        return false;
    }
    let limit = crate::db::get_casino_daily_limit(&ctx.data().db, guild_id).await;
    if limit > 0 {
        let lost = crate::db::get_casino_daily_loss(&ctx.data().db, guild_id, user_id).await;
        if lost >= limit {
            ctx.send(poise::CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("Tageslimit erreicht")
                    .description(format!("Du hast heute **{} Coins** verloren (Limit: {}).", lost, limit))
                    .color(0xFEE75Cu32))
                .ephemeral(true),
            ).await.ok();
            return false;
        }
    }
    true
}

// ── payout helpers ────────────────────────────────────────────────────────────

/// Win: adds profit to user, returns (new_balance, profit, streak_bonus_applied).
async fn pay_win(
    pool:       &sqlx::SqlitePool,
    guild_id:   serenity::GuildId,
    user_id:    serenity::UserId,
    bet:        i64,
    multiplier: f64,
) -> (i64, i64, bool) {
    let stats = crate::db::get_casino_stats(pool, guild_id, user_id).await;
    let streak_bonus = stats.win_streak >= 2;
    let gross  = (bet as f64 * multiplier) as i64;
    let gross  = if streak_bonus { (gross as f64 * 1.1) as i64 } else { gross };
    let profit = gross - bet;

    crate::db::add_coins(pool, guild_id, user_id, profit).await;
    crate::db::casino_vault_add(pool, guild_id, -profit).await;
    crate::db::update_casino_stats(pool, guild_id, user_id, bet, true, profit, gross).await;

    let new_bal = crate::db::get_coins(pool, guild_id, user_id).await;
    (new_bal, profit, streak_bonus)
}

/// Loss: deducts bet from user, returns (new_balance, consolation_given).
async fn pay_loss(
    pool:     &sqlx::SqlitePool,
    guild_id: serenity::GuildId,
    user_id:  serenity::UserId,
    bet:      i64,
) -> (i64, bool) {
    crate::db::add_coins(pool, guild_id, user_id, -bet).await;
    crate::db::casino_vault_add(pool, guild_id, bet).await;
    crate::db::add_casino_daily_loss(pool, guild_id, user_id, bet).await;
    let (_, new_lose) = crate::db::update_casino_stats(pool, guild_id, user_id, bet, false, 0, 0).await;

    let consolation = new_lose % CONSOLATION_STREAK == 0;
    if consolation {
        crate::db::add_coins(pool, guild_id, user_id, CONSOLATION_COINS).await;
    }
    let new_bal = crate::db::get_coins(pool, guild_id, user_id).await;
    (new_bal, consolation)
}

fn consolation_note(given: bool) -> &'static str {
    if given { "\n\n🎁 *Trostpreis: +150 Coins für die Pechsträhne!*" } else { "" }
}

fn streak_note(given: bool) -> &'static str {
    if given { " *(+10% Gewinnsträhnen-Bonus)*" } else { "" }
}

// ── cards ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Card { suit: u8, rank: u8 }

fn new_shuffled_deck() -> Vec<Card> {
    let mut deck: Vec<Card> = (0u8..4).flat_map(|s| (1u8..=13).map(move |r| Card { suit: s, rank: r })).collect();
    let mut rng = rand::thread_rng();
    deck.shuffle(&mut rng);
    deck
}

fn draw(deck: &mut Vec<Card>) -> Card {
    deck.pop().unwrap_or(Card { suit: 0, rank: 1 })
}

fn card_val(rank: u8) -> u8 {
    match rank { 1 => 11, 2..=10 => rank, _ => 10 }
}

fn hand_total(cards: &[Card]) -> u8 {
    let mut total = 0u16;
    let mut aces  = 0u8;
    for c in cards {
        total += card_val(c.rank) as u16;
        if c.rank == 1 { aces += 1; }
    }
    while total > 21 && aces > 0 { total -= 10; aces -= 1; }
    total as u8
}

fn card_str(c: Card) -> String {
    let suit = ["♠","♥","♦","♣"][c.suit as usize];
    let rank = match c.rank {
        1 => "A".to_string(), 11 => "J".to_string(),
        12 => "Q".to_string(), 13 => "K".to_string(),
        n  => n.to_string(),
    };
    format!("{}{}", rank, suit)
}

fn hand_str(cards: &[Card]) -> String {
    cards.iter().map(|c| card_str(*c)).collect::<Vec<_>>().join(" ")
}

// ── /slots ────────────────────────────────────────────────────────────────────

static SLOTS_SYMBOLS: &[(&str, u32, f64)] = &[
    ("🍒", 300, 1.5),
    ("🍋", 250, 2.0),
    ("🍇", 200, 2.5),
    ("🔔", 120, 3.0),
    ("💎",  80, 5.0),
    ("7️⃣",  40, 10.0),
    ("⭐",  10, 50.0),
];

fn spin_reel() -> usize {
    let total: u32 = SLOTS_SYMBOLS.iter().map(|(_, w, _)| w).sum();
    let roll: u32 = { let mut rng = rand::thread_rng(); rng.gen_range(0..total) };
    let mut cum = 0u32;
    for (i, (_, w, _)) in SLOTS_SYMBOLS.iter().enumerate() {
        cum += w;
        if roll < cum { return i; }
    }
    0
}

/// Einarmiger Bandit: drei Rollen drehen sich. Treffer = Münzen!
#[poise::command(slash_command, guild_only, rename = "slots")]
pub async fn slots(
    ctx: Context<'_>,
    #[description = "Einsatz in Coins"] einsatz: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }
    if !validate_bet(&ctx, einsatz).await { return Ok(()); }

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let (r0, r1, r2) = (spin_reel(), spin_reel(), spin_reel());
    let (e0, e1, e2) = (SLOTS_SYMBOLS[r0].0, SLOTS_SYMBOLS[r1].0, SLOTS_SYMBOLS[r2].0);
    let display = format!("[ {} | {} | {} ]", e0, e1, e2);

    let (title, multiplier, color) = if r0 == r1 && r1 == r2 {
        let m = SLOTS_SYMBOLS[r0].2;
        (format!("JACKPOT! {} {} {}", e0, e1, e2), m, 0xFFD700u32)
    } else if r0 == r1 || r1 == r2 {
        ("Zwei gleiche!".to_string(), 0.5, 0xFEE75Cu32)
    } else {
        ("Kein Treffer".to_string(), 0.0, 0xED4245u32)
    };

    let (desc, color) = if multiplier > 0.0 {
        let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, einsatz, multiplier).await;
        (
            format!("{}\n\n**+{} Coins** Gewinn!{}\nKontostand: **{} Coins**", display, profit, streak_note(bonus), new_bal),
            color,
        )
    } else {
        let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, einsatz).await;
        (
            format!("{}\n\n**-{} Coins**\nKontostand: **{} Coins**{}", display, einsatz, new_bal, consolation_note(consolation)),
            0xED4245u32,
        )
    };

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title(format!("🎰 Slots: {}", title))
            .description(desc)
            .color(color)
            .footer(CreateEmbedFooter::new(format!("Einsatz: {} Coins", einsatz))),
    )).await?;

    Ok(())
}

// ── /blackjack ────────────────────────────────────────────────────────────────

/// Blackjack gegen den Dealer. Hit, Stand oder Double Down.
#[poise::command(slash_command, guild_only, rename = "blackjack")]
pub async fn blackjack(
    ctx: Context<'_>,
    #[description = "Einsatz in Coins"] einsatz: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }
    if !validate_bet(&ctx, einsatz).await { return Ok(()); }

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let mut deck   = new_shuffled_deck();
    let mut player = vec![draw(&mut deck), draw(&mut deck)];
    let mut dealer = vec![draw(&mut deck), draw(&mut deck)];
    let mut bet    = einsatz;
    let mut first_action = true;

    // Natural blackjack
    if hand_total(&player) == 21 {
        let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, bet, 2.5).await;
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🃏 Blackjack: NATURAL BLACKJACK! 🎉")
                .description(format!(
                    "Deine Hand: **{}** ({})\nDealer: **{}** ({})\n\n**+{} Coins** (2.5×)!{}\nKontostand: **{} Coins**",
                    hand_str(&player), hand_total(&player),
                    hand_str(&dealer), hand_total(&dealer),
                    profit, streak_note(bonus), new_bal,
                ))
                .color(0xFFD700u32),
        )).await?;
        return Ok(());
    }

    let bj_embed = |player: &[Card], dealer: &[Card], bet: i64, msg: &str| {
        CreateEmbed::new()
            .title("🃏 Blackjack")
            .description(format!(
                "Deine Hand: **{}** ({})\nDealer zeigt: **{}** + 🂠\n\n{}",
                hand_str(player), hand_total(player),
                card_str(dealer[0]), msg,
            ))
            .color(0x5865F2u32)
            .footer(CreateEmbedFooter::new(format!("Einsatz: {} Coins", bet)))
    };

    let action_row = |first: bool| CreateActionRow::Buttons(vec![
        CreateButton::new("bj_hit").label("Hit").style(serenity::ButtonStyle::Primary),
        CreateButton::new("bj_stand").label("Stand").style(serenity::ButtonStyle::Secondary),
        CreateButton::new("bj_double")
            .label("Double Down")
            .style(serenity::ButtonStyle::Danger)
            .disabled(!first),
    ]);

    let handle = ctx.send(poise::CreateReply::default()
        .embed(bj_embed(&player, &dealer, bet, "Was möchtest du tun?"))
        .components(vec![action_row(true)]),
    ).await?;
    let msg = handle.message().await?;

    loop {
        let Some(interaction) = msg
            .await_component_interaction(ctx.serenity_context())
            .timeout(std::time::Duration::from_secs(60))
            .await
        else {
            // Timeout → stand
            break;
        };

        let id = interaction.data.custom_id.as_str();

        if id == "bj_hit" || id == "bj_double" {
            if id == "bj_double" {
                // Double bet if affordable
                let balance = crate::db::get_coins(pool, guild_id, user_id).await;
                if balance >= bet { bet *= 2; }
            }
            player.push(draw(&mut deck));
            first_action = false;

            if hand_total(&player) > 21 {
                // Bust
                interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .components(vec![])
                        .embed(CreateEmbed::new()
                            .title("🃏 Blackjack: Bust!")
                            .description(format!(
                                "Deine Hand: **{}** (**{}**: BUST)\nDealer: **{}** ({})",
                                hand_str(&player), hand_total(&player),
                                hand_str(&dealer), hand_total(&dealer),
                            ))
                            .color(0xED4245u32)),
                )).await.ok();

                let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, bet).await;
                msg.channel_id.send_message(ctx.http(), CreateMessage::new().embed(
                    CreateEmbed::new()
                        .description(format!("**-{} Coins**: Bust!\nKontostand: **{} Coins**{}", bet, new_bal, consolation_note(consolation)))
                        .color(0xED4245u32),
                )).await.ok();
                return Ok(());
            }

            if id == "bj_double" {
                // After double: auto-stand
                interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .embed(bj_embed(&player, &dealer, bet, "Double Down: stehe automatisch."))
                        .components(vec![]),
                )).await.ok();
                break;
            }

            interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(bj_embed(&player, &dealer, bet, "Noch eine Karte?"))
                    .components(vec![action_row(false)]),
            )).await.ok();

        } else {
            // Stand
            interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(bj_embed(&player, &dealer, bet, "Du stehst. Dealer zieht…"))
                    .components(vec![]),
            )).await.ok();
            break;
        }
    }

    // Dealer draws to 17
    while hand_total(&dealer) < 17 {
        dealer.push(draw(&mut deck));
    }

    let player_val = hand_total(&player);
    let dealer_val = hand_total(&dealer);

    let (title, multiplier, won, color) = if dealer_val > 21 {
        ("Dealer bust: du gewinnst!", 2.0, true, 0x57F287u32)
    } else if player_val > dealer_val {
        ("Du gewinnst!", 2.0, true, 0x57F287u32)
    } else if player_val == dealer_val {
        ("Unentschieden: Push!", 1.0, true, 0xFEE75Cu32)
    } else {
        ("Dealer gewinnt.", 0.0, false, 0xED4245u32)
    };

    let result_embed = if multiplier == 1.0 {
        // Push: return bet
        crate::db::add_coins(pool, guild_id, user_id, 0).await;
        let new_bal = crate::db::get_coins(pool, guild_id, user_id).await;
        CreateEmbed::new()
            .title(format!("🃏 Blackjack: {}", title))
            .description(format!(
                "Deine Hand: **{}** ({})\nDealer: **{}** ({})\n\nEinsatz zurück. Kontostand: **{} Coins**",
                hand_str(&player), player_val, hand_str(&dealer), dealer_val, new_bal,
            ))
            .color(color)
    } else if won {
        let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, bet, multiplier).await;
        CreateEmbed::new()
            .title(format!("🃏 Blackjack: {}", title))
            .description(format!(
                "Deine Hand: **{}** ({})\nDealer: **{}** ({})\n\n**+{} Coins**!{}\nKontostand: **{} Coins**",
                hand_str(&player), player_val, hand_str(&dealer), dealer_val,
                profit, streak_note(bonus), new_bal,
            ))
            .color(color)
    } else {
        let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, bet).await;
        CreateEmbed::new()
            .title(format!("🃏 Blackjack: {}", title))
            .description(format!(
                "Deine Hand: **{}** ({})\nDealer: **{}** ({})\n\n**-{} Coins**\nKontostand: **{} Coins**{}",
                hand_str(&player), player_val, hand_str(&dealer), dealer_val,
                bet, new_bal, consolation_note(consolation),
            ))
            .color(color)
    };

    msg.channel_id.send_message(ctx.http(), CreateMessage::new().embed(result_embed)).await.ok();
    Ok(())
}

// ── /wuerfeln ────────────────────────────────────────────────────────────────

#[derive(Debug, poise::ChoiceParameter)]
pub enum WuerfelTipp {
    #[name = "Hoch (8-12): 1.8×"] Hoch,
    #[name = "Niedrig (2-6): 1.8×"] Niedrig,
}

/// Zwei Würfel: tippe auf Hoch oder Niedrig.
#[poise::command(slash_command, guild_only, rename = "wuerfeln")]
pub async fn wuerfeln(
    ctx: Context<'_>,
    #[description = "Einsatz in Coins"] einsatz: i64,
    #[description = "Vorhersage"] tipp: WuerfelTipp,
) -> Result<(), Error> {
    ctx.defer().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }
    if !validate_bet(&ctx, einsatz).await { return Ok(()); }

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let (d1, d2): (u8, u8) = {
        let mut rng = rand::thread_rng();
        (rng.gen_range(1..=6), rng.gen_range(1..=6))
    };
    let sum = d1 + d2;

    let won = match tipp {
        WuerfelTipp::Hoch    => sum >= 8,
        WuerfelTipp::Niedrig => sum <= 6,
    };
    let tipp_str = match tipp { WuerfelTipp::Hoch => "Hoch (8-12)", WuerfelTipp::Niedrig => "Niedrig (2-6)" };

    let (desc, color) = if won {
        let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, einsatz, 1.8).await;
        (
            format!("🎲 **{}** + **{}** = **{}**\n\nTipp: {} ✅\n**+{} Coins**!{}\nKontostand: **{} Coins**", d1, d2, sum, tipp_str, profit, streak_note(bonus), new_bal),
            0x57F287u32,
        )
    } else {
        let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, einsatz).await;
        (
            format!("🎲 **{}** + **{}** = **{}**\n\nTipp: {} ❌\n**-{} Coins**\nKontostand: **{} Coins**{}", d1, d2, sum, tipp_str, einsatz, new_bal, consolation_note(consolation)),
            0xED4245u32,
        )
    };

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new().title("🎲 Würfeln").description(desc).color(color),
    )).await?;
    Ok(())
}

// ── /muenzwurf ────────────────────────────────────────────────────────────────

#[derive(Debug, poise::ChoiceParameter)]
pub enum MuenzSeite {
    #[name = "Kopf"] Kopf,
    #[name = "Zahl"] Zahl,
}

/// Münzwurf: 50/50, zahlt 1.9×.
#[poise::command(slash_command, guild_only, rename = "muenzwurf")]
pub async fn muenzwurf(
    ctx: Context<'_>,
    #[description = "Einsatz in Coins"] einsatz: i64,
    #[description = "Kopf oder Zahl?"] seite: MuenzSeite,
) -> Result<(), Error> {
    ctx.defer().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }
    if !validate_bet(&ctx, einsatz).await { return Ok(()); }

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let heads: bool = { let mut rng = rand::thread_rng(); rng.gen_bool(0.5) };
    let result_str = if heads { "Kopf 🪙" } else { "Zahl 🔢" };
    let tipp_str   = match seite { MuenzSeite::Kopf => "Kopf", MuenzSeite::Zahl => "Zahl" };
    let won = matches!((&seite, heads), (MuenzSeite::Kopf, true) | (MuenzSeite::Zahl, false));

    let (desc, color) = if won {
        let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, einsatz, 1.9).await;
        (
            format!("Ergebnis: **{}**\nTipp: {} ✅\n\n**+{} Coins**!{}\nKontostand: **{} Coins**", result_str, tipp_str, profit, streak_note(bonus), new_bal),
            0x57F287u32,
        )
    } else {
        let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, einsatz).await;
        (
            format!("Ergebnis: **{}**\nTipp: {} ❌\n\n**-{} Coins**\nKontostand: **{} Coins**{}", result_str, tipp_str, einsatz, new_bal, consolation_note(consolation)),
            0xED4245u32,
        )
    };

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new().title("🪙 Münzwurf").description(desc).color(color),
    )).await?;
    Ok(())
}

// ── /roulette ─────────────────────────────────────────────────────────────────

static RED_NUMS: &[u8] = &[1,3,5,7,9,12,14,16,18,19,21,23,25,27,30,32,34,36];

#[derive(Debug, poise::ChoiceParameter)]
pub enum RouletteWette {
    #[name = "Rot (1.9×)"]          Rot,
    #[name = "Schwarz (1.9×)"]      Schwarz,
    #[name = "Gerade (1.9×)"]       Gerade,
    #[name = "Ungerade (1.9×)"]     Ungerade,
    #[name = "1-18 Niedrig (1.9×)"] Niedrig,
    #[name = "19-36 Hoch (1.9×)"]   Hoch,
    #[name = "1. Dutzend 1-12 (2.9×)"]  Dutzend1,
    #[name = "2. Dutzend 13-24 (2.9×)"] Dutzend2,
    #[name = "3. Dutzend 25-36 (2.9×)"] Dutzend3,
    #[name = "Zahl 0-36 (35×)"]     Zahl,
}

/// Roulette: tippe auf Farbe, Gerade/Ungerade, Dutzend oder eine Zahl.
#[poise::command(slash_command, guild_only, rename = "roulette")]
pub async fn roulette(
    ctx: Context<'_>,
    #[description = "Einsatz in Coins"] einsatz: i64,
    #[description = "Art der Wette"] wette: RouletteWette,
    #[description = "Zahl (0-36) nur für Wette \"Zahl\""] zahl: Option<u8>,
) -> Result<(), Error> {
    ctx.defer().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }
    if !validate_bet(&ctx, einsatz).await { return Ok(()); }

    // Validate number bet
    if matches!(wette, RouletteWette::Zahl) {
        let n = zahl.unwrap_or(255);
        if n > 36 {
            ctx.send(poise::CreateReply::default()
                .embed(CreateEmbed::new()
                    .title("Ungültige Zahl")
                    .description("Bitte gib eine Zahl zwischen 0 und 36 an.")
                    .color(0xED4245u32))
                .ephemeral(true),
            ).await?;
            return Ok(());
        }
    }

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let result: u8 = { let mut rng = rand::thread_rng(); rng.gen_range(0..=36) };
    let is_red   = RED_NUMS.contains(&result);
    let color_str = if result == 0 { "🟢 0" } else if is_red { "🔴" } else { "⚫" };

    let (won, multiplier, wette_str) = match &wette {
        RouletteWette::Rot      => (result != 0 && is_red,          1.9, "Rot"),
        RouletteWette::Schwarz  => (result != 0 && !is_red,         1.9, "Schwarz"),
        RouletteWette::Gerade   => (result != 0 && result % 2 == 0, 1.9, "Gerade"),
        RouletteWette::Ungerade => (result != 0 && result % 2 == 1, 1.9, "Ungerade"),
        RouletteWette::Niedrig  => (result >= 1 && result <= 18,    1.9, "1-18"),
        RouletteWette::Hoch     => (result >= 19,                   1.9, "19-36"),
        RouletteWette::Dutzend1 => (result >= 1  && result <= 12,   2.9, "1. Dutzend"),
        RouletteWette::Dutzend2 => (result >= 13 && result <= 24,   2.9, "2. Dutzend"),
        RouletteWette::Dutzend3 => (result >= 25 && result <= 36,   2.9, "3. Dutzend"),
        RouletteWette::Zahl     => (result == zahl.unwrap_or(255),  35.0, "Zahl"),
    };

    let (desc, color) = if won {
        let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, einsatz, multiplier).await;
        (
            format!("Kugel: {} **{}**\nWette: {} ✅\n\n**+{} Coins**! ({}×){}\nKontostand: **{} Coins**", color_str, result, wette_str, profit, multiplier, streak_note(bonus), new_bal),
            0x57F287u32,
        )
    } else {
        let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, einsatz).await;
        (
            format!("Kugel: {} **{}**\nWette: {} ❌\n\n**-{} Coins**\nKontostand: **{} Coins**{}", color_str, result, wette_str, einsatz, new_bal, consolation_note(consolation)),
            0xED4245u32,
        )
    };

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new().title("🎡 Roulette").description(desc).color(color),
    )).await?;
    Ok(())
}

// ── /kartenspiel (Höher oder Tiefer) ─────────────────────────────────────────

static HOL_MULTIPLIERS: &[f64] = &[1.5, 2.0, 3.0, 5.0, 8.0];

/// Höher oder Tiefer: kette Treffer für wachsende Multiplikatoren.
#[poise::command(slash_command, guild_only, rename = "kartenspiel")]
pub async fn kartenspiel(
    ctx: Context<'_>,
    #[description = "Einsatz in Coins"] einsatz: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }
    if !validate_bet(&ctx, einsatz).await { return Ok(()); }

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let mut deck    = new_shuffled_deck();
    let mut current = draw(&mut deck);
    let mut round   = 0usize;

    let hol_embed = |current: Card, round: usize, msg: &str| {
        let next_mult = HOL_MULTIPLIERS.get(round).copied().unwrap_or(*HOL_MULTIPLIERS.last().unwrap());
        CreateEmbed::new()
            .title("🎴 Höher oder Tiefer")
            .description(format!(
                "Aktuelle Karte: **{}**\n\n{}\nNächster Multiplikator: **{}×**",
                card_str(current), msg, next_mult,
            ))
            .color(0x5865F2u32)
            .footer(CreateEmbedFooter::new(format!("Einsatz: {} Coins | Runde {}", einsatz, round + 1)))
    };

    let hol_buttons = |cash_possible: bool| CreateActionRow::Buttons(vec![
        CreateButton::new("hol_higher").label("Höher ▲").style(serenity::ButtonStyle::Success),
        CreateButton::new("hol_lower").label("Tiefer ▼").style(serenity::ButtonStyle::Danger),
        CreateButton::new("hol_cash").label("Auszahlen 💰")
            .style(serenity::ButtonStyle::Secondary)
            .disabled(!cash_possible),
    ]);

    let handle = ctx.send(poise::CreateReply::default()
        .embed(hol_embed(current, round, "Ist die nächste Karte höher oder tiefer?"))
        .components(vec![hol_buttons(false)]),
    ).await?;
    let msg = handle.message().await?;

    loop {
        let Some(interaction) = msg
            .await_component_interaction(ctx.serenity_context())
            .timeout(std::time::Duration::from_secs(60))
            .await
        else {
            // Timeout → lose
            pay_loss(pool, guild_id, user_id, einsatz).await;
            return Ok(());
        };

        let id = interaction.data.custom_id.as_str();

        if id == "hol_cash" {
            // Cash out at current multiplier
            let mult = HOL_MULTIPLIERS.get(round.saturating_sub(1)).copied().unwrap_or(1.0);
            interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().components(vec![]),
            )).await.ok();
            let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, einsatz, mult).await;
            msg.channel_id.send_message(ctx.http(), CreateMessage::new().embed(
                CreateEmbed::new()
                    .title("🎴 Auszahlung!")
                    .description(format!("**+{} Coins** ({}×)!{}\nKontostand: **{} Coins**", profit, mult, streak_note(bonus), new_bal))
                    .color(0x57F287u32),
            )).await.ok();
            return Ok(());
        }

        let next = draw(&mut deck);
        let next_val = card_val(next.rank);
        let curr_val = card_val(current.rank);

        let correct = match id {
            "hol_higher" => next_val >= curr_val,
            _            => next_val <= curr_val,
        };

        if !correct {
            interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(CreateEmbed::new()
                        .title("🎴 Falsch!")
                        .description(format!("Nächste Karte war **{}**: falsch geraten!\n\n**-{} Coins**", card_str(next), einsatz))
                        .color(0xED4245u32))
                    .components(vec![]),
            )).await.ok();
            let (new_bal, consolation) = pay_loss(pool, guild_id, user_id, einsatz).await;
            msg.channel_id.send_message(ctx.http(), CreateMessage::new().embed(
                CreateEmbed::new()
                    .description(format!("Kontostand: **{} Coins**{}", new_bal, consolation_note(consolation)))
                    .color(0xED4245u32),
            )).await.ok();
            return Ok(());
        }

        current = next;
        round  += 1;

        if round >= HOL_MULTIPLIERS.len() {
            // Max rounds reached: auto cash out
            let mult = *HOL_MULTIPLIERS.last().unwrap();
            interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(CreateEmbed::new()
                        .title("🎴 Maximum erreicht!")
                        .description(format!("Nächste Karte war **{}**: korrekt!\nMaximale Runden erreicht!", card_str(current)))
                        .color(0xFFD700u32))
                    .components(vec![]),
            )).await.ok();
            let (new_bal, profit, bonus) = pay_win(pool, guild_id, user_id, einsatz, mult).await;
            msg.channel_id.send_message(ctx.http(), CreateMessage::new().embed(
                CreateEmbed::new()
                    .description(format!("**+{} Coins** ({}×)!{}\nKontostand: **{} Coins**", profit, mult, streak_note(bonus), new_bal))
                    .color(0xFFD700u32),
            )).await.ok();
            return Ok(());
        }

        interaction.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
            CreateInteractionResponseMessage::new()
                .embed(hol_embed(current, round, &format!("✅ Richtig! Karte war **{}**. Weiter?", card_str(current))))
                .components(vec![hol_buttons(round > 0)]),
        )).await.ok();
    }

    Ok(())
}

// ── /lotto ────────────────────────────────────────────────────────────────────

fn random_lotto_numbers() -> Vec<u8> {
    let mut nums: Vec<u8> = (1..=49).collect();
    let mut rng = rand::thread_rng();
    nums.shuffle(&mut rng);
    nums[..6].to_vec()
}

/// Lotto-Tickets kaufen. Tägliche Ziehung um Mitternacht.
#[poise::command(slash_command, guild_only, rename = "lotto")]
pub async fn lotto(
    ctx: Context<'_>,
    #[description = "Anzahl Tickets (à 100 Coins)"] anzahl: u32,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    if !in_casino_channel(&ctx).await { return Ok(()); }

    let anzahl = anzahl.max(1).min(10) as i64;
    let total  = anzahl * LOTTO_TICKET_PRICE;

    let guild_id = ctx.guild_id().unwrap();
    let user_id  = ctx.author().id;
    let pool     = &ctx.data().db;

    let balance = crate::db::get_coins(pool, guild_id, user_id).await;
    if balance < total {
        ctx.send(poise::CreateReply::default()
            .embed(CreateEmbed::new()
                .title("Nicht genug Coins")
                .description(format!("{} Tickets kosten **{} Coins**, du hast nur **{}**.", anzahl, total, balance))
                .color(0xED4245u32))
            .ephemeral(true),
        ).await?;
        return Ok(());
    }

    crate::db::add_coins(pool, guild_id, user_id, -total).await;
    let drawing = crate::db::get_or_create_lotto_drawing(pool, guild_id, ctx.channel_id()).await;

    // 50% of each ticket to jackpot, 50% to vault
    let jackpot_share = (LOTTO_TICKET_PRICE / 2) * anzahl;
    crate::db::add_to_lotto_jackpot(pool, drawing.id, jackpot_share).await;
    crate::db::casino_vault_add(pool, guild_id, total - jackpot_share).await;

    let mut ticket_lines = Vec::new();
    for _ in 0..anzahl {
        let nums = random_lotto_numbers();
        let display: Vec<String> = nums.iter().map(|n| format!("{:02}", n)).collect();
        ticket_lines.push(display.join(" - "));
        crate::db::add_lotto_ticket(pool, drawing.id, guild_id, user_id, &nums).await;
    }

    let updated_drawing = crate::db::get_active_lotto_drawing(pool, guild_id).await;
    let jackpot = updated_drawing.map(|d| d.jackpot).unwrap_or(drawing.jackpot + jackpot_share);

    ctx.send(poise::CreateReply::default()
        .embed(CreateEmbed::new()
            .title("🎟️ Lotto-Tickets gekauft!")
            .description(format!(
                "**{} Ticket(s)** für **{} Coins**\n\n{}\n\nAktueller Jackpot: **{} Coins**",
                anzahl, total, ticket_lines.join("\n"), jackpot,
            ))
            .color(0x5865F2u32)
            .footer(CreateEmbedFooter::new("Ziehung täglich um Mitternacht UTC")))
        .ephemeral(true),
    ).await?;
    Ok(())
}

/// Lotto-Ziehung durchführen (wird automatisch täglich ausgeführt).
pub async fn run_lotto_drawing(ctx: &serenity::Context, pool: &sqlx::SqlitePool) {
    let active = crate::db::get_guilds_with_active_lotto(pool).await;
    for (guild_id, drawing) in active {
        let tickets = crate::db::get_lotto_tickets(pool, drawing.id).await;
        if tickets.is_empty() {
            // No tickets: just close and create new
            crate::db::close_lotto_drawing(pool, drawing.id, &[]).await;
            continue;
        }

        let winning: Vec<u8> = random_lotto_numbers();
        crate::db::close_lotto_drawing(pool, drawing.id, &winning).await;

        // Calculate winners
        let mut winners_3 = Vec::new();
        let mut winners_4 = Vec::new();
        let mut winners_5 = Vec::new();
        let mut winners_6 = Vec::new();

        for ticket in &tickets {
            let matches = ticket.numbers.iter().filter(|n| winning.contains(n)).count();
            match matches {
                3 => winners_3.push(ticket.user_id),
                4 => winners_4.push(ticket.user_id),
                5 => winners_5.push(ticket.user_id),
                6 => winners_6.push(ticket.user_id),
                _ => {}
            }
        }

        let winning_str: Vec<String> = winning.iter().map(|n| format!("{:02}", n)).collect();
        let mut desc = format!("**Gewinnzahlen:** {}\n\n", winning_str.join(" - "));

        let pay_winners = |winners: &[serenity::UserId], prize: i64, pool: &sqlx::SqlitePool, guild_id: serenity::GuildId| {
            let pool = pool.clone();
            let winners = winners.to_vec();
            async move {
                for uid in winners {
                    crate::db::add_coins(&pool, guild_id, uid, prize).await;
                }
            }
        };

        if winners_6.is_empty() {
            desc.push_str(&format!("🎯 6 Treffer: niemand: Jackpot rollt weiter!\n"));
        } else {
            let share = drawing.jackpot / winners_6.len() as i64;
            let mentions: Vec<String> = winners_6.iter().map(|u| format!("<@{}>", u)).collect();
            desc.push_str(&format!("🏆 6 Treffer: {}: **{} Coins** (Jackpot)!\n", mentions.join(", "), share));
            pay_winners(&winners_6, share, pool, guild_id).await;
        }
        if !winners_5.is_empty() {
            let mentions: Vec<String> = winners_5.iter().map(|u| format!("<@{}>", u)).collect();
            desc.push_str(&format!("🥇 5 Treffer: {}: **10.000 Coins**!\n", mentions.join(", ")));
            pay_winners(&winners_5, 10_000, pool, guild_id).await;
        }
        if !winners_4.is_empty() {
            let mentions: Vec<String> = winners_4.iter().map(|u| format!("<@{}>", u)).collect();
            desc.push_str(&format!("🥈 4 Treffer: {}: **1.000 Coins**\n", mentions.join(", ")));
            pay_winners(&winners_4, 1_000, pool, guild_id).await;
        }
        if !winners_3.is_empty() {
            let mentions: Vec<String> = winners_3.iter().map(|u| format!("<@{}>", u)).collect();
            desc.push_str(&format!("🥉 3 Treffer: {}: **200 Coins**\n", mentions.join(", ")));
            pay_winners(&winners_3, 200, pool, guild_id).await;
        }
        if winners_3.is_empty() && winners_4.is_empty() && winners_5.is_empty() && winners_6.is_empty() {
            desc.push_str("Kein Gewinner heute. Jackpot rollt weiter!");
            // Carry jackpot to next drawing
            if let Some(ch) = drawing.channel_id {
                let next = crate::db::get_or_create_lotto_drawing(pool, guild_id, ch).await;
                crate::db::add_to_lotto_jackpot(pool, next.id, drawing.jackpot).await;
            }
        }

        // Announce in channel
        if let Some(channel_id) = drawing.channel_id {
            channel_id.send_message(&ctx.http, CreateMessage::new().embed(
                CreateEmbed::new()
                    .title("🎟️ Lotto-Ziehung!")
                    .description(desc)
                    .color(0xFFD700u32),
            )).await.ok();
        }
    }
}

/// Background task: draws lotto daily at midnight UTC.
pub fn schedule_lotto(ctx: serenity::Context, pool: sqlx::SqlitePool) {
    tokio::spawn(async move {
        loop {
            let now      = Utc::now();
            let tomorrow = (now + chrono::Duration::days(1))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let secs_until = (tomorrow.and_utc().timestamp() - now.timestamp()).max(0) as u64;
            tokio::time::sleep(std::time::Duration::from_secs(secs_until)).await;
            run_lotto_drawing(&ctx, &pool).await;
            tracing::info!("Lotto-Ziehung abgeschlossen");
        }
    });
}

// ── /casino-stats ─────────────────────────────────────────────────────────────

/// Deine Casino-Statistiken anzeigen.
#[poise::command(slash_command, guild_only, rename = "casino-stats")]
pub async fn casino_stats(
    ctx: Context<'_>,
    #[description = "Nutzer (leer = du selbst)"] nutzer: Option<serenity::User>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().unwrap();
    let target   = nutzer.as_ref().unwrap_or(ctx.author());
    let stats    = crate::db::get_casino_stats(&ctx.data().db, guild_id, target.id).await;
    let net      = stats.total_won as i64 - stats.total_lost as i64;
    let win_rate = if stats.games_played > 0 {
        format!("{:.1}%", stats.total_won as f64 / stats.games_played as f64 * 100.0)
    } else {
        "N/A".to_string()
    };

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title(format!("🎰 Casino-Stats: {}", target.name))
            .field("Gesamt gesetzt",   format!("{} Coins", stats.total_wagered), true)
            .field("Gesamt gewonnen",  format!("{} Coins", stats.total_won),     true)
            .field("Gesamt verloren",  format!("{} Coins", stats.total_lost),    true)
            .field("Nettogewinn",      format!("{:+} Coins", net),               true)
            .field("Größter Gewinn",   format!("{} Coins", stats.biggest_win),   true)
            .field("Spiele gespielt",  stats.games_played.to_string(),           true)
            .field("Gewinnsträhne",    stats.win_streak.to_string(),             true)
            .field("Pechsträhne",      stats.lose_streak.to_string(),            true)
            .color(0x5865F2u32),
    ).ephemeral(true)).await?;
    Ok(())
}

// ── /casino-rangliste ─────────────────────────────────────────────────────────

/// Top 10 Casino-Spieler nach Gesamtgewinn.
#[poise::command(slash_command, guild_only, rename = "casino-rangliste")]
pub async fn casino_rangliste(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().unwrap();
    let board    = crate::db::get_casino_leaderboard(&ctx.data().db, guild_id).await;

    if board.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().title("Casino-Rangliste").description("Noch keine Spieler.").color(0x5865F2u32),
        )).await?;
        return Ok(());
    }

    let lines: Vec<String> = board.iter().enumerate().map(|(i, (uid, s))| {
        let medal = match i { 0 => "🥇", 1 => "🥈", 2 => "🥉", _ => "▪️" };
        format!("{} <@{}>: **{} Coins** gewonnen ({} Spiele)", medal, uid, s.total_won, s.games_played)
    }).collect();

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("🎰 Casino-Rangliste")
            .description(lines.join("\n"))
            .color(0x5865F2u32),
    )).await?;
    Ok(())
}

// ── admin commands ────────────────────────────────────────────────────────────

/// Casino-Kanal festlegen (leer = überall erlaubt).
#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "casino-setup")]
pub async fn casino_setup(
    ctx: Context<'_>,
    #[description = "Kanal für Casino-Befehle (leer = überall)"] kanal: Option<serenity::GuildChannel>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().unwrap();
    let ch_id    = kanal.as_ref().map(|c| c.id);
    crate::db::set_casino_channel(&ctx.data().db, guild_id, ch_id).await;

    let msg = match &kanal {
        Some(c) => format!("Casino-Kanal auf <#{}> gesetzt.", c.id),
        None    => "Casino-Befehle sind jetzt überall erlaubt.".to_string(),
    };
    ctx.send(poise::CreateReply::default()
        .embed(CreateEmbed::new().title("Casino-Setup").description(msg).color(0x57F287u32))
        .ephemeral(true),
    ).await?;
    Ok(())
}

/// Casino-Tresor-Kontostand anzeigen.
#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "casino-tresor")]
pub async fn casino_tresor(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().unwrap();
    let balance  = crate::db::casino_vault_get(&ctx.data().db, guild_id).await;
    ctx.send(poise::CreateReply::default()
        .embed(CreateEmbed::new()
            .title("🏦 Casino-Tresor")
            .description(format!("Aktueller Saldo: **{} Coins**", balance))
            .color(0x5865F2u32))
        .ephemeral(true),
    ).await?;
    Ok(())
}

/// Tägliches Verlustlimit pro Nutzer setzen (0 = kein Limit).
#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "casino-limit")]
pub async fn casino_limit(
    ctx: Context<'_>,
    #[description = "Maximaler Verlust pro Tag pro Nutzer (0 = kein Limit)"] limit: i64,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().unwrap();
    crate::db::set_casino_daily_limit(&ctx.data().db, guild_id, limit.max(0)).await;
    let msg = if limit <= 0 {
        "Kein tägliches Verlustlimit.".to_string()
    } else {
        format!("Tägliches Verlustlimit auf **{} Coins** gesetzt.", limit)
    };
    ctx.send(poise::CreateReply::default()
        .embed(CreateEmbed::new().title("Casino-Limit").description(msg).color(0x57F287u32))
        .ephemeral(true),
    ).await?;
    Ok(())
}

/// Lotto-Jackpot manuell erhöhen.
#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "casino-jackpot")]
pub async fn casino_jackpot(
    ctx: Context<'_>,
    #[description = "Coins zum Jackpot hinzufügen"] betrag: i64,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().unwrap();
    let pool     = &ctx.data().db;

    let drawing = crate::db::get_or_create_lotto_drawing(pool, guild_id, ctx.channel_id()).await;
    crate::db::add_to_lotto_jackpot(pool, drawing.id, betrag.max(0)).await;
    let updated = crate::db::get_active_lotto_drawing(pool, guild_id).await;
    let new_jp  = updated.map(|d| d.jackpot).unwrap_or(betrag);

    ctx.send(poise::CreateReply::default()
        .embed(CreateEmbed::new()
            .title("Jackpot erhöht")
            .description(format!("**+{} Coins** zum Jackpot hinzugefügt.\nAktueller Jackpot: **{} Coins**", betrag, new_jp))
            .color(0xFFD700u32))
        .ephemeral(true),
    ).await?;
    Ok(())
}
