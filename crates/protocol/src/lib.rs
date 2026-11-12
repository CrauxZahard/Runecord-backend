//! Shared domain and wire types for Runecord's local server and Rune client.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Id = i64;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Users and presence
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Online,
    Idle,
    #[default]
    Offline,
    Dnd,
}

impl Presence {
    pub fn label(self) -> &'static str {
        match self {
            Presence::Online => "Online",
            Presence::Idle => "Idle",
            Presence::Dnd => "Do Not Disturb",
            Presence::Offline => "Offline",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct User {
    pub id: Id,
    pub username: String,
    pub display_name: String,
    /// sRGB accent colour for the generated avatar.
    pub avatar_color: u32,
    #[serde(default)]
    pub avatar_border: u32,
    #[serde(default)]
    pub banner_color: u32,
    pub bio: String,
    pub custom_status: String,
    pub status: Presence,
    #[serde(default)]
    pub is_bot: bool,
    pub created_at: i64,
}

impl User {
    pub fn initials(&self) -> String {
        self.display_name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }

    pub fn handle(&self) -> String {
        format!("@{}", self.username)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user: User,
}

// ---------------------------------------------------------------------------
// Guilds, channels, roles
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    #[default]
    Text,
    Voice,
    Category,
    Announcement,
    Forum,
    Thread,
    Dm,
    GroupDm,
}

impl ChannelKind {
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum | ChannelKind::Thread | ChannelKind::Dm | ChannelKind::GroupDm
        )
    }

    pub fn is_guild_text(self) -> bool {
        matches!(self, ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum | ChannelKind::Thread)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub parent_id: Id,
    pub name: String,
    pub message_count: u32,
    pub archived: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Channel {
    pub id: Id,
    pub guild_id: Option<Id>,
    pub name: String,
    pub kind: ChannelKind,
    pub position: i32,
    pub parent_id: Option<Id>,
    pub topic: String,
    pub nsfw: bool,
    pub slowmode: u32,
    pub last_message_id: Option<Id>,
    pub owner_id: Option<Id>,
    pub thread: Option<ThreadInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwriteKind {
    Role,
    Member,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionOverwrite {
    pub target_id: Id,
    pub kind: OverwriteKind,
    pub allow: u64,
    pub deny: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Guild {
    pub id: Id,
    pub name: String,
    pub icon_color: u32,
    pub owner_id: Id,
    pub description: String,
    pub member_count: u32,
    #[serde(default)]
    pub tag: String,
    pub created_at: i64,
    pub channels: Vec<Channel>,
    pub roles: Vec<Role>,
    pub members: Vec<Member>,
    pub overwrites: Vec<ChannelOverwrites>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelOverwrites {
    pub channel_id: Id,
    pub overwrites: Vec<PermissionOverwrite>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Role {
    pub id: Id,
    pub guild_id: Id,
    pub name: String,
    pub color: u32,
    pub permissions: u64,
    pub position: i32,
    pub hoist: bool,
    pub mentionable: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Member {
    pub guild_id: Id,
    pub user: User,
    pub nickname: String,
    pub roles: Vec<Id>,
    pub joined_at: i64,
}

pub mod perm {
    pub const CREATE_INVITE: u64 = 1 << 0;
    pub const KICK_MEMBERS: u64 = 1 << 1;
    pub const BAN_MEMBERS: u64 = 1 << 2;
    pub const ADMINISTRATOR: u64 = 1 << 3;
    pub const MANAGE_CHANNELS: u64 = 1 << 4;
    pub const MANAGE_GUILD: u64 = 1 << 5;
    pub const ADD_REACTIONS: u64 = 1 << 6;
    pub const VIEW_AUDIT_LOG: u64 = 1 << 7;
    pub const PRIORITY_SPEAKER: u64 = 1 << 8;
    pub const STREAM: u64 = 1 << 9;
    pub const VIEW_CHANNEL: u64 = 1 << 10;
    pub const SEND_MESSAGES: u64 = 1 << 11;
    pub const MANAGE_MESSAGES: u64 = 1 << 13;
    pub const EMBED_LINKS: u64 = 1 << 14;
    pub const ATTACH_FILES: u64 = 1 << 15;
    pub const READ_MESSAGE_HISTORY: u64 = 1 << 16;
    pub const MENTION_EVERYONE: u64 = 1 << 17;
    pub const USE_EXTERNAL_EMOJIS: u64 = 1 << 18;
    pub const CONNECT: u64 = 1 << 20;
    pub const SPEAK: u64 = 1 << 21;
    pub const MUTE_MEMBERS: u64 = 1 << 22;
    pub const DEAFEN_MEMBERS: u64 = 1 << 23;
    pub const MOVE_MEMBERS: u64 = 1 << 24;
    pub const MANAGE_NICKNAMES: u64 = 1 << 27;
    pub const MANAGE_ROLES: u64 = 1 << 28;
    pub const MANAGE_WEBHOOKS: u64 = 1 << 29;
    pub const MANAGE_THREADS: u64 = 1 << 34;
    pub const CREATE_THREADS: u64 = 1 << 35;
    pub const SEND_MESSAGES_IN_THREADS: u64 = 1 << 38;

    pub const EVERYONE: u64 = VIEW_CHANNEL
        | SEND_MESSAGES
        | ADD_REACTIONS
        | EMBED_LINKS
        | ATTACH_FILES
        | READ_MESSAGE_HISTORY
        | USE_EXTERNAL_EMOJIS
        | CONNECT
        | SPEAK
        | STREAM
        | CREATE_THREADS
        | SEND_MESSAGES_IN_THREADS
        | CREATE_INVITE;

    pub const MANAGE: u64 = ADMINISTRATOR
        | MANAGE_CHANNELS
        | MANAGE_GUILD
        | MANAGE_MESSAGES
        | MANAGE_ROLES
        | MANAGE_THREADS
        | KICK_MEMBERS;
}

pub fn has(permissions: u64, bit: u64) -> bool {
    permissions & perm::ADMINISTRATOR != 0 || permissions & bit != 0
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    #[default]
    Default,
    Reply,
    ThreadCreated,
    SystemJoin,
    SystemPin,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Id,
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub count: u32,
    pub me: bool,
    pub user_ids: Vec<Id>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MessageReply {
    pub message_id: Id,
    pub author_id: Id,
    pub author_name: String,
    pub content: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Message {
    pub id: Id,
    pub channel_id: Id,
    pub author_id: Id,
    pub content: String,
    pub timestamp: i64,
    pub edited_at: Option<i64>,
    pub kind: MessageKind,
    pub reply_to: Option<MessageReply>,
    pub attachments: Vec<Attachment>,
    pub reactions: Vec<Reaction>,
    pub pinned: bool,
    pub webhook: Option<WebhookAuthor>,
    pub thread_id: Option<Id>,
    pub mentions: Vec<Id>,
    pub mention_everyone: bool,
    /// Client-only: optimistic send in flight.
    #[serde(default)]
    pub pending: bool,
}

impl Message {
    pub fn preview(&self) -> String {
        if !self.content.is_empty() {
            self.content.replace('\n', " ")
        } else if let Some(a) = self.attachments.first() {
            format!("[{}]", a.filename)
        } else {
            String::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Relationships and reads
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Friend,
    Incoming,
    Outgoing,
    Blocked,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Relationship {
    pub kind: RelationshipKind,
    pub user: User,
    pub since: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReadState {
    pub channel_id: Id,
    pub last_read_id: Option<Id>,
    pub mention_count: u32,
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WebhookAuthor {
    pub id: Id,
    pub name: String,
    pub color: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DmChannel {
    pub channel: Channel,
    pub recipients: Vec<User>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    pub user: User,
    pub guilds: Vec<Guild>,
    pub dms: Vec<DmChannel>,
    pub relationships: Vec<Relationship>,
    pub reads: Vec<ReadState>,
    pub users: Vec<User>,
    pub presences: BTreeMap<Id, Presence>,
    pub unread: BTreeMap<Id, u32>,
    pub emojis: Vec<Emoji>,
    pub voice_states: Vec<VoiceState>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Emoji {
    pub id: Id,
    pub guild_id: Id,
    pub name: String,
    pub sticker: bool,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Application {
    pub id: Id,
    pub owner_id: Id,
    pub name: String,
    pub description: String,
    pub icon_color: u32,
    pub bot: User,
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationCommand {
    pub id: Id,
    pub app_id: Id,
    pub name: String,
    pub description: String,
    pub options: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WebhookInfo {
    pub id: Id,
    pub guild_id: Id,
    pub channel_id: Id,
    pub name: String,
    pub avatar_color: u32,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Id,
    pub guild_id: Id,
    pub actor_id: Id,
    pub action_type: i32,
    pub target_id: Option<Id>,
    pub changes: serde_json::Value,
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VoiceState {
    pub user_id: Id,
    pub guild_id: Id,
    pub channel_id: Option<Id>,
    pub self_mute: bool,
    pub self_deaf: bool,
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterRequest {
    Register { username: String, display_name: String, password: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SendMessage {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reply_to: Option<Id>,
    #[serde(default)]
    pub attachment_ids: Vec<Id>,
    #[serde(default)]
    pub nonce: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditMessage {
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionToggle {
    pub emoji: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGuild {
    pub name: String,
    pub icon_color: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateChannel {
    pub name: String,
    pub kind: ChannelKind,
    pub parent_id: Option<Id>,
    pub topic: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateGuild {
    pub name: Option<String>,
    pub icon_color: Option<u32>,
    pub description: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateMessageThread {
    pub name: String,
    pub message_id: Option<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateInvite {
    pub max_uses: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invite {
    pub code: String,
    pub guild_id: Id,
    pub guild_name: String,
    pub guild_icon_color: u32,
    pub inviter_id: Id,
    pub uses: u32,
    pub created_at: i64,
    pub member_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub message: Message,
    pub channel_id: Id,
    pub channel_name: String,
    pub guild_id: Option<Id>,
    pub guild_name: Option<String>,
    pub author: User,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResults {
    pub total: u32,
    pub hits: Vec<SearchHit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateProfile {
    pub display_name: Option<String>,
    pub avatar_color: Option<u32>,
    pub bio: Option<String>,
    pub custom_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadUpdate {
    pub channel_id: Id,
    pub last_read_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub attachment: Attachment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadQuery {
    pub name: String,
    pub content_type: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberUpdate {
    pub user_id: Id,
    pub nickname: Option<String>,
    pub role_ids: Option<Vec<Id>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoleUpdate {
    pub name: Option<String>,
    pub color: Option<u32>,
    pub permissions: Option<u64>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateRole {
    pub name: String,
    pub color: u32,
    pub permissions: u64,
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", content = "d", rename_all = "snake_case")]
pub enum GatewayEvent {
    Ready(Box<Bootstrap>),
    MessageCreate(Box<Message>),
    MessageUpdate(Box<Message>),
    MessageDelete { channel_id: Id, message_id: Id },
    ReactionUpdate(Box<Message>),
    TypingStart { channel_id: Id, user_id: Id, until_ms: i64 },
    PresenceUpdate { user_id: Id, status: Presence },
    ChannelCreate(Box<Channel>),
    ChannelUpdate(Box<Channel>),
    ChannelDelete { channel_id: Id, guild_id: Option<Id> },
    GuildCreate(Box<Guild>),
    GuildUpdate(Box<Guild>),
    GuildDelete { guild_id: Id },
    MemberUpdate(Box<Member>),
    GuildMembers { guild_id: Id, members: Vec<Member> },
    RelationshipUpdate(Box<Relationship>),
    RelationshipRemove { user_id: Id },
    ReadUpdate { channel_id: Id, last_read_id: Option<Id>, mention_count: u32 },
    UserUpdate(Box<User>),
    VoiceStateUpdate(Box<VoiceState>),
    InteractionCreate { app_id: Id, payload: serde_json::Value },
    ReadyCheck,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", content = "d", rename_all = "snake_case")]
pub enum GatewayCommand {
    Subscribe { channel_id: Id },
    Typing { channel_id: Id },
    Heartbeat,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayEnvelope<T> {
    pub op: u8,
    pub d: T,
}
