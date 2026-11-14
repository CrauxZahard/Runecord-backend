//! Guilds, roles, members and guild events.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GuildSummary {
    pub id: Id,
    pub name: String,
    pub icon_color: u32,
    pub owner_id: Id,
    pub description: String,
    pub member_count: u32,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub permissions: u64,
    #[serde(default)]
    pub unread_count: u32,
    #[serde(default)]
    pub mention_count: u32,
    pub created_at: i64,
    #[serde(default)]
    pub discoverable: bool,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub banner_url: String,
    #[serde(default)]
    pub boosts: u32,
    #[serde(default)]
    pub boost_level: u32,
    #[serde(default)]
    pub boosted_by_me: bool,
    #[serde(default)]
    pub upload_limit_bytes: u64,
    #[serde(default)]
    pub voice_kbps: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicGuild {
    pub id: Id,
    pub name: String,
    pub icon_color: u32,
    pub description: String,
    pub member_count: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ban {
    pub user: User,
    pub reason: String,
    pub actor_id: Id,
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BanAppeal {
    pub id: Id,
    pub guild_id: Id,
    pub user: User,
    pub message: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GuildDetail {
    pub guild: GuildSummary,
    pub channels: Vec<Channel>,
    pub roles: Vec<Role>,
    pub overwrites: Vec<ChannelOverwrites>,
    pub permissions: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MemberPage {
    pub members: Vec<Member>,
    pub total: u32,
    pub next_offset: Option<u32>,
    pub presences: BTreeMap<Id, Presence>,
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
    #[serde(default)]
    pub permissions: u64,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub banner_url: String,
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
    #[serde(default)]
    pub icon_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoleLink {
    pub id: Id,
    pub guild_id: Id,
    pub role_id: Id,
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuildEvent {
    pub id: Id,
    pub guild_id: Id,
    pub creator_id: Id,
    pub name: String,
    pub description: String,
    pub start_at: i64,
    pub channel_id: Option<Id>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGuildEvent {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub start_at: i64,
    #[serde(default)]
    pub channel_id: Option<Id>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InboxEntry {
    pub channel_id: Id,
    pub guild_id: Option<Id>,
    pub message_id: Id,
    pub guild_name: String,
    pub channel_name: String,
    pub author_id: Id,
    pub author_name: String,
    pub snippet: String,
    pub timestamp: i64,
    pub unread: u32,
    pub mention: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inbox {
    pub mentions: Vec<InboxEntry>,
    pub unreads: Vec<InboxEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateRoleLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Member {
    pub guild_id: Id,
    pub user: User,
    pub nickname: String,
    pub roles: Vec<Id>,
    pub joined_at: i64,
    #[serde(default)]
    pub timeout_until: i64,
}
