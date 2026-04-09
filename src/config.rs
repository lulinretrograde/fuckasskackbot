use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use poise::serenity_prelude::{ChannelId, GuildId, MessageId, RoleId, UserId};
use tokio::sync::Mutex;

// ── log channel config ────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct LogConfig {
    pub voice: Option<ChannelId>,
    pub messages: Option<ChannelId>,
    pub join_leave: Option<ChannelId>,
    pub server: Option<ChannelId>,
    pub members: Option<ChannelId>,
    pub welcome: Option<ChannelId>,
    pub mod_log: Option<ChannelId>,
    pub bot_log: Option<ChannelId>,
    pub jail_role: Option<RoleId>,
    pub jail_channel: Option<ChannelId>,
    pub base_role: Option<RoleId>,
}

pub type LogConfigs = Arc<Mutex<HashMap<GuildId, LogConfig>>>;

// ── raid detection ────────────────────────────────────────────────────────────

pub type JoinTracker = Arc<Mutex<HashMap<GuildId, VecDeque<Instant>>>>;

pub const RAID_JOINS: usize = 5;
pub const RAID_WINDOW_SECS: u64 = 30;

// ── message cache (for delete logs) ──────────────────────────────────────────

#[derive(Clone)]
pub struct CachedMessage {
    pub author_id: UserId,
    pub author_tag: String,
    pub content: String,
    pub channel_id: ChannelId,
}

/// (message_id → data, insertion-order deque for FIFO eviction)
pub type MessageCache = Arc<Mutex<(HashMap<MessageId, CachedMessage>, VecDeque<MessageId>)>>;

pub const MESSAGE_CACHE_LIMIT: usize = 2000;

// ── xp cooldowns (ephemeral) ──────────────────────────────────────────────────

pub type XpCooldowns = Arc<Mutex<HashMap<(GuildId, UserId), Instant>>>;

// ── invite cache (invite code → use count, ephemeral) ────────────────────────

pub type InviteCache = Arc<Mutex<HashMap<GuildId, HashMap<String, u64>>>>;

// ── voice sessions (guild+user → join Instant, ephemeral) ────────────────────

pub type VoiceSessions = Arc<Mutex<HashMap<(GuildId, UserId), Instant>>>;

// ── anti-nuke ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ActionKind {
    ChannelDelete,
    ChannelCreate,
    RoleDelete,
    RoleCreate,
    Ban,
    WebhookCreate,
}

pub type NukeCounters = Arc<Mutex<HashMap<(GuildId, UserId, ActionKind), VecDeque<Instant>>>>;
pub type RaidCounters = Arc<Mutex<HashMap<GuildId, VecDeque<(Instant, UserId)>>>>;
pub type LockdownState = Arc<Mutex<HashMap<GuildId, Instant>>>;

// ── bug report cooldowns (user → last report time) ────────────────────────────

/// One report per user per 10 minutes.
pub const BUG_COOLDOWN_SECS: u64 = 600;
pub type BugCooldowns = Arc<Mutex<HashMap<UserId, Instant>>>;

// ── ticket reply awaiting state ───────────────────────────────────────────────

#[derive(Clone)]
pub enum TicketAction {
    Resolve,
    Decline,
    ChannelCloseResolve,
}

/// Owner user ID → (ticket_id, action) waiting for a DM reply to use as message body.
pub type AwaitingTicketReply = Arc<Mutex<HashMap<UserId, (i64, TicketAction)>>>;
