use rand::Rng;
use chrono::Timelike;

use poise::serenity_prelude as serenity;
use serenity::{CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter};

use crate::commands::moderation::info;
use crate::{Context, Error};

const ARBEIT_COOLDOWN_SECS: i64 = 3600;

struct WorkTier {
    name:      &'static str,
    min_coins: i64,
    max_coins: i64,
    xp_reward: u64,
    jobs:      &'static [(&'static str, &'static str)],
}

static TIERS: &[WorkTier] = &[
    WorkTier {
        name: "Einsteiger",
        min_coins: 40,
        max_coins: 80,
        xp_reward: 15,
        jobs: &[
            ("Zeitungsausträger", "Du hast um 5 Uhr morgens Zeitungen ausgetragen. Drei Hunde haben gebellt, keiner gebissen. **+{} Coins.**"),
            ("Tellerwäscher",     "4 Stunden Teller waschen. Das Wasser war kalt, der Chef war kälter. Aber der Lohn ist da. **+{} Coins.**"),
            ("Parkwächter",       "Im Parkhaus gestanden und Stempel gegeben. Einer hat 'danke' gesagt. Das erste Mal heute. **+{} Coins.**"),
            ("Kassierer",         "Schicht am Supermarkt-Kassierer. 300 Kunden, 2 Beschwerden, 1 Storno. Das Geld ist da. **+{} Coins.**"),
            ("Harz IV",           "Nach 4 Stunden Wartezeit und 23 Formularen hat der Sachbearbeiter alles genehmigt. **+{} Coins.**"),
            ("Betteln",           "Du hast einen Pappbecher hingestellt, bist kurz Döner holen gegangen - und als du wiederkamst, war er voll. **+{} Coins.**"),
        ],
    },
    WorkTier {
        name: "Fachkraft",
        min_coins: 70,
        max_coins: 130,
        xp_reward: 20,
        jobs: &[
            ("Elektriker",   "Du hast eine Steckdose repariert. Der Kunde hat die ganze Zeit Fragen gestellt. Trotzdem: **+{} Coins.**"),
            ("Mechaniker",   "Ölwechsel, Reifenwechsel, eine Inspektion. Hände schwarz, Konto heller. **+{} Coins.**"),
            ("Koch",         "Das Mittagsservice war chaotisch, aber die Gäste waren satt. Chef hat nichts gesagt - das ist Lob. **+{} Coins.**"),
            ("Buchhalter",   "Monatsabschluss, Steuererklärung, drei Ausreden für das Finanzamt. Alles läuft. **+{} Coins.**"),
            ("Programmierer","Feature gebaut, Fehler gefixt, PR gemergt. Niemandem aufgefallen. Gehalt trotzdem überwiesen. **+{} Coins.**"),
            ("Fahrer",       "Fünf Stunden auf der Autobahn. GPS hat dreimal die Route geändert. Du bist trotzdem pünktlich. **+{} Coins.**"),
        ],
    },
    WorkTier {
        name: "Experte",
        min_coins: 110,
        max_coins: 200,
        xp_reward: 30,
        jobs: &[
            ("Arzt",       "12 Patienten behandelt. Keiner war ernsthaft krank, aber alle haben sich unwohl gefühlt. Abgerechnet. **+{} Coins.**"),
            ("Anwalt",     "Schriftverkehr, Verhandlung, Vergleich. Mandant zufrieden, Gericht entlastet, du bezahlt. **+{} Coins.**"),
            ("Ingenieur",  "Die Brücke hält. Die Berechnungen stimmten. Das Modell hat bestätigt was du wusstest. **+{} Coins.**"),
            ("Architekt",  "Pläne genehmigt. Nur 6 Änderungen, kein Einspruch. Persönlicher Rekord. **+{} Coins.**"),
            ("Analytiker", "Drei Dashboards, ein Bericht, zwei Präsentationen. Management hat genickt. **+{} Coins.**"),
            ("Forscher",   "Experiment abgeschlossen, Daten ausgewertet, Paper eingereicht. Reviewer haben diesmal nichts zu meckern. **+{} Coins.**"),
        ],
    },
    WorkTier {
        name: "Elite",
        min_coins: 180,
        max_coins: 350,
        xp_reward: 50,
        jobs: &[
            ("CEO",           "Drei Meetings, eine Strategiepräsentation und zwei Telefonkonferenzen überlebt. Quartalsgehalt ausgezahlt. **+{} Coins.**"),
            ("Börsenmakler",  "Guter Tag an der Börse. Du warst früher drin und früher raus als alle anderen. **+{} Coins.**"),
            ("Politiker",     "Rede gehalten, Hände geschüttelt, nichts versprochen. Diäten überwiesen. **+{} Coins.**"),
            ("Unternehmer",   "Drei Verträge unterschrieben, ein Startup gepitcht, zwei VCs überzeugt. Gewinnanteil ausgezahlt. **+{} Coins.**"),
            ("Hedgefonds-Mgr","Quartalsrendite über Ziel. Kunden sind zufrieden. Bonus wird überwiesen. **+{} Coins.**"),
            ("Berater",       "Wochenlang nichts getan außer PowerPoints erstellt. Der Kunde zahlt trotzdem eine sechsstellige Rechnung. **+{} Coins.**"),
        ],
    },
];

// ── legacy flavor text (unused, kept for reference) ─────────────────────────
static _ARBEIT_LEGACY: &[(&str, &[&str], &[&str])] = &[
    (
        "Harz IV beantragen",
        &[
            "Nach nur 4 Stunden Wartezeit und 23 Formularen hat der Sachbearbeiter deinen Antrag genehmigt -inklusive einer Fördermaßnahme, die du gar nicht beantragt hast. **+{} Coins.**",
            "Der Sachbearbeiter war heute gut drauf, weil seine Kollegin endlich in Rente gegangen ist. Er hat dir einfach alles genehmigt. **+{} Coins.**",
        ],
        &[
            "Das Amt hat deinen Antrag wegen eines Tippfehlers abgelehnt. Du warst *Simon Müller*, nicht *Simon Müler*. Bearbeitungsgebühr trotzdem fällig. **{} Coins.**",
            "Du hast das falsche Formular ausgefüllt. Der Sachbearbeiter hat dich wortlos rausgeworfen. Nächster Termin: in 6 Wochen. **{} Coins.**",
        ],
    ),
    (
        "Kong Strong kaufen",
        &[
            "Du hast Kong Strong auf dem Flohmarkt für 30 Cent gekauft und für 4€ weiterverkauft. Nennst du das jetzt Unternehmertum? **+{} Coins.**",
            "Nach drei Kong Strong warst du so hyper, dass du aus Versehen produktiv warst. Drei Aufgaben erledigt, **+{} Coins** verdient. Dein Herz schlägt noch.",
        ],
        &[
            "Du hast aus Versehen 'Kong Weak' erwischt -die Discounter-Eigenmarke. Schmeckt nach Enttäuschung und Pfandbon. **{} Coins** weg.",
            "Die Dose war aufgeblasen. Du hast sie trotzdem getrunken. Der Arzt hat **{} Coins** gekostet. Es war nicht wert.",
        ],
    ),
    (
        "In der Fußgängerzone betteln",
        &[
            "Eine Rentnerin dachte, du wärst ihr Enkelsohn, und hat dir ihren kompletten Geldbeutel in die Hand gedrückt. Du hast kurz überlegt, ob du es zurückgibst. Hast du nicht. **+{} Coins.**",
            "Du hast einen Pappbecher hingestellt, bist kurz Döner holen gegangen, und als du wiederkamst, war er voll. Menschen sind manchmal nett. Manchmal. **+{} Coins.**",
        ],
        &[
            "Ein Streetworker hat dich aus dem Revier gescheucht. Jemand hat außerdem einen Kaugummi in deinen Becher geworfen. **{} Coins** verloren, Würde auch.",
            "Du hast 2 Stunden gebettelt. Eine Frau hat dir einen Zettel in die Hand gedrückt -ihr Einkaufszettel. **{} Coins** für die Mühe.",
        ],
    ),
    (
        "Gelben Schein beantragen",
        &[
            "Der Arzt hat dich einen Blick lang angeschaut und gesagt: 'Sie sehen krank aus.' Du hattest nur Augenringe. Gelber Schein für 3 Wochen. **+{} Coins** Krankengeld.",
            "Du hast Rückenschmerzen vorgetäuscht. Der Arzt hatte selbst Rückenschmerzen und wollte früher Feierabend. Er hat sofort unterschrieben. **+{} Coins.**",
        ],
        &[
            "Der Arzt hat dich durch einen Fitnesstest gejagt. Du warst zu fit. Kein gelber Schein, aber Praxisgebühr trotzdem fällig. **{} Coins** weg.",
            "Du hast online einen Symptom-Generator benutzt und versehentlich angegeben, schwanger zu sein. Das Gespräch war unangenehm. **{} Coins** Strafgebühr.",
        ],
    ),
    (
        "Ausbildung abbrechen",
        &[
            "Dein Ausbilder war so froh, dich loszuwerden, dass er dir spontan eine inoffizielle 'Abgangsprämie' gezahlt hat. **+{} Coins.** Auf Nimmerwiedersehen.",
            "Du hast die Ausbildung abgebrochen und direkt ein TikTok darüber gepostet. Drei Sponsorenanfragen in 24 Stunden. **+{} Coins.**",
        ],
        &[
            "Du hast die Ausbildung abgebrochen und erfährst jetzt, dass du die Vergütung der letzten zwei Monate zurückzahlen musst. Kleines rechtliches Erwachen. **{} Coins** weg.",
            "Du hast abgebrochen, aber vergessen, dich beim Jobcenter zu melden. Die haben dich jetzt zu einer Maßnahme eingeladen: 'Bewerbungstraining für Abbrecher'. Kosten: **{} Coins.**",
        ],
    ),
    (
        "Studium abbrechen",
        &[
            "Du hast das Studium hingeschmissen, eine App-Idee gehabt und Investoren gefunden, die nicht wussten, was die App macht. **+{} Coins.** Klassiker.",
            "Dein Professor war so genervt von dir, dass er dich persönlich rausgeworfen hat -und dabei aus Versehen deine Abschlussarbeit eines anderen Studenten mitgegeben hat. Verkauft. **+{} Coins.**",
        ],
        &[
            "Du hast das Studium abgebrochen, aber die Semestergebühren sind bereits gebucht. Und die Mensa-Karte läuft diese Woche ab. **{} Coins** weg.",
            "Du hast abgebrochen und deinen Eltern noch nicht gesagt. Die haben heute angerufen und gefragt, wie die Prüfungen laufen. **{} Coins** psychologischer Schaden.",
        ],
    ),
    (
        "Kündigen",
        &[
            "Du hast gekündigt und dein Chef war so überrascht, dass er dir eine Abfindung angeboten hat, nur damit du sofort gehst. Er bereut es jetzt. Du nicht. **+{} Coins.**",
            "Du hast per Post gekündigt. Der Brief ist nie angekommen. Du bekommst weiter Gehalt, niemand fragt. **+{} Coins.** Bis es jemand merkt.",
        ],
        &[
            "Du hast während der Probezeit gekündigt und dabei aus Versehen den Firmenlaptop mitgenommen. Rückgabe plus Strafe: **{} Coins** weg.",
            "Dein Chef hat nach deiner Kündigung angefangen zu weinen. Du hast sie aus Mitleid zurückgenommen. Jetzt hast du weder Abfindung noch Freiheit. **{} Coins** Verlust.",
        ],
    ),
    (
        "Arbeitsunfall vortäuschen",
        &[
            "Deine Schauspielerei war so überzeugend, dass der Betriebsarzt selbst kurz gezweifelt hat, ob er dir glauben soll. Die Berufsgenossenschaft hat sofort gezahlt. **+{} Coins.**",
            "Du hast einen Arbeitsunfall vorgetäuscht, dabei aber aus Versehen wirklich einen kleinen Unfall gebaut. Die Versicherung hat sogar mehr gezahlt als geplant. **+{} Coins.**",
        ],
        &[
            "Der Betriebsarzt hat dich in 11 Sekunden durchschaut. Jetzt gibt es eine Abmahnung, eine Untersuchungsgebühr und einen sehr unangenehmen HR-Termin. **{} Coins** weg.",
            "Du hast den Unfall zu gut geplant. Die Polizei wurde eingeschaltet. Es war sehr aufwändig, das aufzuklären. **{} Coins** für den Anwalt.",
        ],
    ),
];

const KLAUEN_COOLDOWN_SECS: i64 = 1800; // 30 Minuten

// {target} = mention des Opfers, {coins} = Betrag
static KLAUEN_ERFOLG: &[&str] = &[
    "Du hast {target} kurz abgelenkt und dabei unbemerkt das Portemonnaie geleert. Er hat es nicht mal gemerkt. **+{coins} Coins.**",
    "Wahrend {target} auf sein Handy geschaut hat, hast du schnell zugegriffen. Sauber und professionell. **+{coins} Coins.**",
    "Du hast {target} im Gedrange der Fußgängerzone angerempelt und dabei die Geldborse gezogen. 'Entschuldigung!' hast du noch gerufen. **+{coins} Coins.**",
    "{target} hat kurz die Tasche abgestellt. Du hast kurzer abgestellt. **+{coins} Coins.**",
    "Du hast {target} mit einer erfundenen Frage abgelenkt, wahrend deine andere Hand bereits aktiv war. Oscar-reife Ablenkung. **+{coins} Coins.**",
    "{target} hat das Kleingeld aus der Tasche fallen lassen. Du hast es aufgehoben. Fur dich. **+{coins} Coins.**",
];

static KLAUEN_ERWISCHT: &[&str] = &[
    "{target} hat sich umgedreht genau in dem Moment. Er hat direkt die Polizei gerufen. Strafe: **{coins} Coins.**",
    "Du hast dich so ungeschickt angestellt, dass {target} dich schon von weitem beobachtet hat. Die Polizei stand in zwei Minuten da. Strafe: **{coins} Coins.**",
    "{target} hatte die Geldborse mit einem Stahlseil befestigt. Du hast daran gezogen. Es hat sich nicht bewegt. Alle haben geguckt. Strafe: **{coins} Coins.**",
    "Ein Freund von {target} hat alles gesehen und sofort eingegriffen. Der Polizist um die Ecke auch. Strafe: **{coins} Coins.**",
    "Dein Klingelton ist losgegangen genau als du die Hand in {target}s Tasche gesteckt hast. 'Baby Shark'. Strafe: **{coins} Coins.**",
    "{target} hat sich als Zivilpolizist entpuppt. Er hatte extra gewartet, bis du es versuchst. Strafe: **{coins} Coins.**",
];

/// Arbeite und verdiene Coins - einmal pro Stunde (Belohnung steigt mit deinem Level)
#[poise::command(slash_command, guild_only)]
pub async fn arbeit(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user = ctx.author();

    if is_economy_jailed(ctx).await? { return Ok(()); }

    // ── cooldown check ────────────────────────────────────────────────────────
    let now = chrono::Utc::now().timestamp();
    if let Some(last) = crate::db::get_arbeit_cooldown(&ctx.data().db, guild_id, user.id).await {
        let elapsed = now - last;
        if elapsed < ARBEIT_COOLDOWN_SECS {
            let remaining = ARBEIT_COOLDOWN_SECS - elapsed;
            let mins = remaining / 60;
            let secs = remaining % 60;
            ctx.send(
                poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .description(format!(
                            "⏳ **Du bist noch erschöpft!**\nNoch **{}m {}s** bis du wieder arbeiten kannst.",
                            mins, secs
                        ))
                        .color(0xED4245u32),
                ),
            )
            .await?;
            return Ok(());
        }
    }

    // ── determine tier from level ─────────────────────────────────────────────
    let total_xp = crate::db::get_xp(&ctx.data().db, guild_id, user.id).await;
    let level = crate::xp::level_from_xp(total_xp);
    let tier = match level {
        0..=4  => &TIERS[0],
        5..=9  => &TIERS[1],
        10..=19 => &TIERS[2],
        _      => &TIERS[3],
    };

    // ── roll coins and pick job (rng dropped before any await) ───────────────
    let (label, coins, response) = {
        let mut rng = rand::thread_rng();
        let coins: i64 = rng.gen_range(tier.min_coins..=tier.max_coins);
        let (job_name, template) = tier.jobs[rng.gen_range(0..tier.jobs.len())];
        let text = template.replace("{}", &coins.to_string());
        (job_name, coins, text)
    };

    // ── XP: apply booster if active ───────────────────────────────────────────
    let has_booster = crate::db::has_active_shop_item(&ctx.data().db, guild_id, user.id, "xp_booster").await;
    let xp_gain = if has_booster { tier.xp_reward * 2 } else { tier.xp_reward };

    let old_xp  = total_xp;
    let new_xp  = crate::db::add_xp(&ctx.data().db, guild_id, user.id, xp_gain).await;
    let old_lvl = crate::xp::level_from_xp(old_xp);
    let new_lvl = crate::xp::level_from_xp(new_xp);

    // ── apply coins + feed 10% to bank + update cooldown ─────────────────────
    let new_balance = crate::db::add_coins(&ctx.data().db, guild_id, user.id, coins).await;
    crate::db::add_to_bank(&ctx.data().db, guild_id, coins / 10).await;
    crate::db::set_arbeit_cooldown(&ctx.data().db, guild_id, user.id, now).await;

    let tier_label = format!("{} (Level {})", tier.name, level);

    let mut embed = CreateEmbed::new()
        .author(serenity::CreateEmbedAuthor::new(user.tag()).icon_url(user.face()))
        .title(format!("💼 {} — {}", label, tier_label))
        .description(response)
        .color(0x57F287u32)
        .field(
            "Ergebnis",
            format!("+{} Coins → Kontostand: **{} Coins**", coins, new_balance),
            true,
        )
        .field(
            "XP",
            format!("+{} XP{}", xp_gain, if has_booster { " 🚀" } else { "" }),
            true,
        )
        .footer(CreateEmbedFooter::new("Nächster Einsatz in 1 Stunde"));

    // level up notification inside embed
    if new_lvl > old_lvl && old_lvl < 50 {
        embed = embed.field(
            "🎉 Level Up!",
            format!("Level **{}** → **{}** (+{} Coins!)", old_lvl, new_lvl, new_lvl * 100),
            false,
        );
    }

    let ready_at = now + ARBEIT_COOLDOWN_SECS;
    let remind_btn = CreateButton::new(format!("remind_arbeit_{}_{}", user.id, ready_at))
        .label("🔔 In 1 Std erinnern")
        .style(serenity::ButtonStyle::Secondary);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![CreateActionRow::Buttons(vec![remind_btn])]),
    ).await?;

    // ── level-up: grant coins and announce ────────────────────────────────────
    if new_lvl > old_lvl && old_lvl < 50 {
        let reward = (new_lvl * 100) as i64;
        crate::db::add_coins(&ctx.data().db, guild_id, user.id, reward).await;
        crate::db::set_credited_level(&ctx.data().db, guild_id, user.id, new_lvl).await;
        let bot_ch = crate::db::get_bot_channel(&ctx.data().db, guild_id).await;
        if let Some(ch) = bot_ch {
            let _ = ch.send_message(
                ctx.serenity_context(),
                crate::commands::levels::level_up_embed(user.id, new_lvl),
            ).await;
        }
    }

    crate::events::send_bot_log(ctx.serenity_context(), ctx.data(), guild_id,
        serenity::CreateEmbed::new()
            .title(format!("💼 Arbeit: {}", label))
            .color(0x57F287u32)
            .field("Nutzer", format!("<@{}>", user.id), true)
            .field("Ergebnis", format!("+{} Coins", coins), true)
            .field("Kontostand", format!("{} Coins", new_balance), true)
            .timestamp(serenity::Timestamp::now()),
    ).await;

    Ok(())
}

/// Bestiehl einen anderen Nutzer - Erwischungsrisiko steigt am Tag
#[poise::command(slash_command, guild_only)]
pub async fn klauen(
    ctx: Context<'_>,
    #[description = "Wen willst du bestehlen?"] opfer: serenity::User,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let dieb = ctx.author();

    if is_economy_jailed(ctx).await? { return Ok(()); }

    if opfer.bot {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("Du kannst keinen Bot bestehlen. Der hat keine Coins.")
                .color(0xED4245u32),
        )).await?;
        return Ok(());
    }
    if opfer.id == dieb.id {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("Du kannst dich nicht selbst bestehlen. Das nennt man Verlust.")
                .color(0xED4245u32),
        )).await?;
        return Ok(());
    }

    // ── cooldown ──────────────────────────────────────────────────────────────
    let now = chrono::Utc::now().timestamp();
    if let Some(last) = crate::db::get_klauen_cooldown(&ctx.data().db, guild_id, dieb.id).await {
        let elapsed = now - last;
        if elapsed < KLAUEN_COOLDOWN_SECS {
            let remaining = KLAUEN_COOLDOWN_SECS - elapsed;
            let mins = remaining / 60;
            let secs = remaining % 60;
            ctx.send(poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .description(format!(
                        "Du bist noch zu auffällig. Warte noch **{}m {}s** bevor du wieder klaust.",
                        mins, secs
                    ))
                    .color(0xED4245u32),
            )).await?;
            return Ok(());
        }
    }

    // ── time-based catch probability ──────────────────────────────────────────
    let hour = chrono::Local::now().hour();
    let catch_pct: u32 = if hour >= 22 || hour < 6 {
        40 // Nacht
    } else if (6..10).contains(&hour) || (18..22).contains(&hour) {
        45 // Morgen / Abend
    } else {
        60 // Tag
    };

    let (caught, stolen, fine) = {
        let mut rng = rand::thread_rng();
        let caught = rng.gen_range(0..100) < catch_pct;
        let stolen: i64 = rng.gen_range(0..=15);
        let fine: i64 = rng.gen_range(0..=20);
        (caught, stolen, fine)
    };

    crate::db::set_klauen_cooldown(&ctx.data().db, guild_id, dieb.id, now).await;

    let opfer_mention = format!("<@{}>", opfer.id);

    if caught {
        let new_balance = crate::db::add_coins(&ctx.data().db, guild_id, dieb.id, -fine).await;
        crate::db::add_to_bank(&ctx.data().db, guild_id, fine).await;
        let template = {
            let mut rng = rand::thread_rng();
            KLAUEN_ERWISCHT[rng.gen_range(0..KLAUEN_ERWISCHT.len())]
        };
        let text = template
            .replace("{target}", &opfer_mention)
            .replace("{coins}", &fine.to_string());

        let time_label = if hour >= 22 || hour < 6 { "Nacht (40%)" }
            else if (6..10).contains(&hour) || (18..22).contains(&hour) { "Morgen/Abend (45%)" }
            else { "Tag (60%)" };

        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .author(serenity::CreateEmbedAuthor::new(dieb.tag()).icon_url(dieb.face()))
                .title("🚔 Erwischt!")
                .description(text)
                .color(0xED4245u32)
                .field("Kontostand", format!("**{} Coins**", new_balance), true)
                .field("Tageszeit", time_label, true),
        )).await?;
        crate::events::send_bot_log(ctx.serenity_context(), ctx.data(), guild_id,
            serenity::CreateEmbed::new()
                .title("🚔 Klauen: Erwischt")
                .color(0xED4245u32)
                .field("Dieb", format!("<@{}>", dieb.id), true)
                .field("Opfer", format!("<@{}>", opfer.id), true)
                .field("Strafe", format!("-{} Coins", fine), true)
                .timestamp(serenity::Timestamp::now()),
        ).await;
    } else {
        let opfer_balance = crate::db::get_coins(&ctx.data().db, guild_id, opfer.id).await;
        let actual_stolen = stolen.min(opfer_balance).max(0);
        crate::db::add_coins(&ctx.data().db, guild_id, opfer.id, -actual_stolen).await;
        let new_balance = crate::db::add_coins(&ctx.data().db, guild_id, dieb.id, actual_stolen).await;

        let template = {
            let mut rng = rand::thread_rng();
            KLAUEN_ERFOLG[rng.gen_range(0..KLAUEN_ERFOLG.len())]
        };
        let text = template
            .replace("{target}", &opfer_mention)
            .replace("{coins}", &actual_stolen.to_string());

        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .author(serenity::CreateEmbedAuthor::new(dieb.tag()).icon_url(dieb.face()))
                .title("💸 Erfolgreich geklaut!")
                .description(text)
                .color(0x57F287u32)
                .field("Kontostand", format!("**{} Coins**", new_balance), true),
        )).await?;
        crate::events::send_bot_log(ctx.serenity_context(), ctx.data(), guild_id,
            serenity::CreateEmbed::new()
                .title("💸 Klauen: Erfolgreich")
                .color(0x57F287u32)
                .field("Dieb", format!("<@{}>", dieb.id), true)
                .field("Opfer", format!("<@{}>", opfer.id), true)
                .field("Gestohlen", format!("+{} Coins", actual_stolen), true)
                .timestamp(serenity::Timestamp::now()),
        ).await;
    }

    Ok(())
}

// ── jail check helper ─────────────────────────────────────────────────────────

/// Returns true and sends a jail message if the user is currently jailed.
async fn is_economy_jailed(ctx: Context<'_>) -> Result<bool, Error> {
    let guild_id = ctx.guild_id().unwrap();
    let now = chrono::Utc::now().timestamp();
    if let Some(until) = crate::db::get_jail_until(&ctx.data().db, guild_id, ctx.author().id).await {
        if until > now {
            let remaining = until - now;
            let hours = remaining / 3600;
            let mins = (remaining % 3600) / 60;
            ctx.send(poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🔒 Du sitzt im Knast!")
                    .description(format!(
                        "Das Gericht hat entschieden. Du kommst in **{}h {}min** frei.\n\
                         Einladungen funktionieren trotzdem.",
                        hours, mins
                    ))
                    .color(0xED4245u32),
            )).await?;
            return Ok(true);
        }
    }
    Ok(false)
}

// ── /banküberfall ─────────────────────────────────────────────────────────────

static BANKRAUB_ERFOLG: &[&str] = &[
    "Du und deine Crew haben die Bank in 4 Minuten leer geräumt. Der Sicherheitsdienst war beim Mittagessen. **+{coins} Coins** aus dem Tresor.",
    "Drei Ablenkungsmanöver, ein fingierter Stromausfall und eine sehr überzeugende Uniform. Der Tresor ist leer. **+{coins} Coins.**",
    "Der Tresor hatte als Passwort '1234'. Du stehst immer noch fassungslos davor. **+{coins} Coins.**",
    "Der Bankdirektor hat dich für einen Wirtschaftsprüfer gehalten und dich persönlich in den Tresorraum geführt. **+{coins} Coins.**",
    "Du hast alle Kameras vorher mit Post-its abgeklebt. Der Wachmann hat es nicht bemerkt, weil er selbst am Schlafen war. **+{coins} Coins.**",
];

static BANKRAUB_ERWISCHT: &[&str] = &[
    "Das SEK war bereits im Tresorraum und hat auf dich gewartet. Jemand hat dich verpfiffen. Das Gericht verurteilt dich zu **{hours} Stunden** Knast.",
    "Du hast die falsche Adresse aufgeschrieben. Es war eine Polizeiwache. Mit einer Geldkassette davor. **{hours} Stunden** Knast.",
    "Die Fluchtroute war perfekt geplant. Der Fluchtwagen stand bereit. Du hast den Schlüssel innen gelassen. **{hours} Stunden** nach Urteilsverkündung.",
    "Ein Passant hat deinen Einbruch in Echtzeit auf TikTok gestreamt. 40.000 Zuschauer. Darunter drei Polizisten. **{hours} Stunden** Knast.",
    "Der Tunnel hat drei Stunden funktioniert. Dann bist du in die falsche Richtung gebohrt und direkt im Keller der Polizeiwache gelandet. **{hours} Stunden** Urteil.",
    "Du hast aus Nervosität laut 'Das ist ein Überfall!' gerufen, bevor du überhaupt drin warst. Der Alarm lief sofort. **{hours} Stunden** Knast.",
];

/// Beraube die Bank - einmal täglich, hohes Risiko, alles oder nichts
#[poise::command(slash_command, guild_only, rename = "bankueberfall")]
pub async fn bankueberfall(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let user = ctx.author();
    let now = chrono::Utc::now().timestamp();

    if is_economy_jailed(ctx).await? { return Ok(()); }

    // ── cooldown: once per 24h ────────────────────────────────────────────────
    if let Some(last) = crate::db::get_bankraub_cooldown(&ctx.data().db, guild_id, user.id).await {
        let elapsed = now - last;
        if elapsed < 86400 {
            let remaining = 86400 - elapsed;
            let hours = remaining / 3600;
            let mins = (remaining % 3600) / 60;
            ctx.send(poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .description(format!(
                        "Du wartest noch auf den richtigen Moment. Noch **{}h {}min** bis zum nächsten Versuch.",
                        hours, mins
                    ))
                    .color(0xED4245u32),
            )).await?;
            return Ok(());
        }
    }

    let bank_balance = crate::db::get_bank(&ctx.data().db, guild_id).await;

    if bank_balance == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🏦 Die Bank ist leer")
                .description("Kein Geld in der Bank. Warte bis jemand arbeitet oder Strafen zahlt.")
                .color(0xFEE75Cu32),
        )).await?;
        return Ok(());
    }

    // ── time-based catch probability ──────────────────────────────────────────
    let hour = chrono::Local::now().hour();
    let catch_pct: u32 = if hour >= 22 || hour < 6 { 55 }
        else if (6..10).contains(&hour) || (18..22).contains(&hour) { 65 }
        else { 75 };

    let (caught, jail_hours) = {
        let mut rng = rand::thread_rng();
        let caught = rng.gen_range(0..100) < catch_pct;
        let jail_hours: i64 = rng.gen_range(8..=12);
        (caught, jail_hours)
    };

    crate::db::set_bankraub_cooldown(&ctx.data().db, guild_id, user.id, now).await;

    let time_label = if hour >= 22 || hour < 6 { "Nacht (55%)" }
        else if (6..10).contains(&hour) || (18..22).contains(&hour) { "Morgen/Abend (65%)" }
        else { "Tag (75%)" };

    if caught {
        let jail_until = now + jail_hours * 3600;
        crate::db::set_jail_until(&ctx.data().db, guild_id, user.id, jail_until).await;

        let wallet_before = crate::db::get_coins(&ctx.data().db, guild_id, user.id).await;
        let fine = wallet_before / 5; // 20%
        let wallet_after = crate::db::add_coins(&ctx.data().db, guild_id, user.id, -fine).await;

        let template = {
            let mut rng = rand::thread_rng();
            BANKRAUB_ERWISCHT[rng.gen_range(0..BANKRAUB_ERWISCHT.len())]
        };
        let text = template.replace("{hours}", &jail_hours.to_string());

        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .author(serenity::CreateEmbedAuthor::new(user.tag()).icon_url(user.face()))
                .title("🚔 Festgenommen!")
                .description(text)
                .color(0xED4245u32)
                .field("Geldstrafe", format!("-{} Coins (20% des Kontos)", fine), true)
                .field("Kontostand", format!("{} Coins", wallet_after), true)
                .field("Freilassung", format!("<t:{}:R>", jail_until), true),
        )).await?;
        crate::events::send_bot_log(ctx.serenity_context(), ctx.data(), guild_id,
            serenity::CreateEmbed::new()
                .title("🚔 Bankraub: Erwischt")
                .color(0xED4245u32)
                .field("Nutzer", format!("<@{}>", user.id), true)
                .field("Knast", format!("{} Stunden", jail_hours), true)
                .field("Strafe", format!("-{} Coins", fine), true)
                .field("Freilassung", format!("<t:{}:R>", jail_until), false)
                .timestamp(serenity::Timestamp::now()),
        ).await;
    } else {
        let stolen = crate::db::drain_bank(&ctx.data().db, guild_id).await;
        let new_balance = crate::db::add_coins(&ctx.data().db, guild_id, user.id, stolen).await;

        let template = {
            let mut rng = rand::thread_rng();
            BANKRAUB_ERFOLG[rng.gen_range(0..BANKRAUB_ERFOLG.len())]
        };
        let text = template.replace("{coins}", &stolen.to_string());

        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .author(serenity::CreateEmbedAuthor::new(user.tag()).icon_url(user.face()))
                .title("💰 Bankraub erfolgreich!")
                .description(text)
                .color(0x57F287u32)
                .field("Erbeutet", format!("**{} Coins**", stolen), true)
                .field("Neuer Kontostand", format!("**{} Coins**", new_balance), true)
                .field("Tageszeit", time_label, true),
        )).await?;
        crate::events::send_bot_log(ctx.serenity_context(), ctx.data(), guild_id,
            serenity::CreateEmbed::new()
                .title("💰 Bankraub: Erfolgreich")
                .color(0x57F287u32)
                .field("Nutzer", format!("<@{}>", user.id), true)
                .field("Erbeutet", format!("{} Coins", stolen), true)
                .field("Neuer Kontostand", format!("{} Coins", new_balance), true)
                .timestamp(serenity::Timestamp::now()),
        ).await;
    }

    Ok(())
}

/// Dein Kontostand oder der eines anderen Nutzers
#[poise::command(slash_command, guild_only)]
pub async fn coins(
    ctx: Context<'_>,
    #[description = "Nutzer (Standard: du selbst)"] user: Option<serenity::User>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let target = user.as_ref().unwrap_or_else(|| ctx.author());
    let guild_id = ctx.guild_id().unwrap();

    if target.bot {
        ctx.send(
            poise::CreateReply::default()
                .embed(info("Ungültig", "Bots haben kein Konto.")),
        )
        .await?;
        return Ok(());
    }

    let balance = crate::db::get_coins(&ctx.data().db, guild_id, target.id).await;
    let invites = crate::db::get_invites(&ctx.data().db, guild_id, target.id).await;

    let embed = CreateEmbed::new()
        .author(serenity::CreateEmbedAuthor::new(target.tag()).icon_url(target.face()))
        .color(0xF1C40Fu32)
        .field("💰 Kontostand", format!("**{} Coins**", balance), true)
        .field("🎟️ Einladungen", format!("**{}**", invites), true);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// Top 10 Nutzer nach Coins
#[poise::command(slash_command, guild_only)]
pub async fn coins_leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let entries = crate::db::get_coins_leaderboard(&ctx.data().db, guild_id, 10).await;

    if entries.is_empty() {
        ctx.send(
            poise::CreateReply::default()
                .embed(info("Keine Daten", "Noch niemand hat Coins verdient. Lade Leute ein!")),
        )
        .await?;
        return Ok(());
    }

    let medals = ["🥇", "🥈", "🥉"];
    let mut lines = Vec::new();
    for (i, (user_id, coins)) in entries.iter().enumerate() {
        let prefix = medals.get(i).copied().unwrap_or("🔹");
        lines.push(format!("{} **#{}** <@{}> -**{}** Coins", prefix, i + 1, user_id, coins));
    }

    let guild_icon = ctx.guild().and_then(|g| g.icon_url()).unwrap_or_default();
    let guild_name = ctx.guild().map(|g| g.name.clone()).unwrap_or_default();

    let embed = CreateEmbed::new()
        .title("💰 Coin-Bestenliste")
        .description(lines.join("\n"))
        .color(0xF1C40Fu32)
        .thumbnail(guild_icon)
        .footer(CreateEmbedFooter::new(format!("Top 10 auf {}", guild_name)));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// Coins an einen anderen Nutzer überweisen
#[poise::command(slash_command, guild_only, rename = "ueberweisung")]
pub async fn ueberweisung(
    ctx: Context<'_>,
    #[description = "Empfänger"] empfaenger: serenity::User,
    #[description = "Betrag (min. 1)"] betrag: i64,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let sender   = ctx.author();

    if empfaenger.bot {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("Bots haben kein Konto. Überweisung abgelehnt.")
                .color(0xED4245u32),
        )).await?;
        return Ok(());
    }

    if empfaenger.id == sender.id {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("Du kannst keine Coins an dich selbst überweisen.")
                .color(0xED4245u32),
        )).await?;
        return Ok(());
    }

    if betrag < 1 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("Mindestbetrag ist **1 Coin**.")
                .color(0xED4245u32),
        )).await?;
        return Ok(());
    }

    let balance = crate::db::get_coins(&ctx.data().db, guild_id, sender.id).await;
    if balance < betrag {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description(format!(
                    "Nicht genug Coins. Du hast **{} Coins**, brauchst aber **{}**.",
                    balance, betrag
                ))
                .color(0xED4245u32),
        )).await?;
        return Ok(());
    }

    crate::db::add_coins(&ctx.data().db, guild_id, sender.id, -betrag).await;
    let new_receiver = crate::db::add_coins(&ctx.data().db, guild_id, empfaenger.id, betrag).await;
    let new_sender   = crate::db::get_coins(&ctx.data().db, guild_id, sender.id).await;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .author(serenity::CreateEmbedAuthor::new(sender.tag()).icon_url(sender.face()))
            .title("💸 Überweisung erfolgreich")
            .description(format!(
                "**{} Coins** wurden an <@{}> überwiesen.",
                betrag, empfaenger.id
            ))
            .color(0x57F287u32)
            .field("Dein neuer Kontostand", format!("**{} Coins**", new_sender), true)
            .field(
                format!("Kontostand von {}", empfaenger.name),
                format!("**{} Coins**", new_receiver),
                true,
            ),
    )).await?;

    crate::events::send_bot_log(ctx.serenity_context(), ctx.data(), guild_id,
        serenity::CreateEmbed::new()
            .title("💸 Überweisung")
            .color(0x57F287u32)
            .field("Von",    format!("<@{}>", sender.id),     true)
            .field("An",     format!("<@{}>", empfaenger.id), true)
            .field("Betrag", format!("{} Coins", betrag),     true)
            .timestamp(serenity::Timestamp::now()),
    ).await;

    Ok(())
}
