//! The initial client bootstrap payload.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    pub user: User,
    pub guilds: Vec<GuildSummary>,
    pub dms: Vec<DmChannel>,
    pub relationships: Vec<Relationship>,
    pub reads: Vec<ReadState>,
    pub users: Vec<User>,
    pub presences: BTreeMap<Id, Presence>,
    pub unread: BTreeMap<Id, u32>,
    pub emojis: Vec<Emoji>,
    pub voice_states: Vec<VoiceState>,
    #[serde(default)]
    pub channel_notifications: BTreeMap<Id, NotificationLevel>,
    #[serde(default)]
    pub channel_suppress: std::collections::BTreeSet<Id>,
    #[serde(default)]
    pub guild_notifications: std::collections::BTreeMap<Id, GuildNotificationSettings>,
}
