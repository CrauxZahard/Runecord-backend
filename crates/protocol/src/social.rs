//! Relationships, read state, DMs and notifications.

use serde::{Deserialize, Serialize};
use super::*;

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

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    #[default]
    All,
    Mentions,
    None,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuildNotificationSettings {
    #[serde(default)]
    pub level: NotificationLevel,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub suppress_everyone: bool,
    #[serde(default)]
    pub suppress_roles: bool,
    #[serde(default)]
    pub suppress_highlights: bool,
    #[serde(default)]
    pub mute_new_events: bool,
    #[serde(default)]
    pub mobile_push: bool,
}
