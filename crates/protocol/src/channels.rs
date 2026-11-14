//! Channels, threads and permission overwrites.

use serde::{Deserialize, Serialize};
use super::*;

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
            ChannelKind::Text
                | ChannelKind::Announcement
                | ChannelKind::Forum
                | ChannelKind::Thread
                | ChannelKind::Dm
                | ChannelKind::GroupDm
        )
    }

    pub fn is_guild_text(self) -> bool {
        matches!(
            self,
            ChannelKind::Text
                | ChannelKind::Announcement
                | ChannelKind::Forum
                | ChannelKind::Thread
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub parent_id: Id,
    pub name: String,
    pub message_count: u32,
    pub archived: bool,
    #[serde(default)]
    pub preview: String,
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
    #[serde(default)]
    pub status: String,
    pub nsfw: bool,
    pub slowmode: u32,
    pub last_message_id: Option<Id>,
    pub owner_id: Option<Id>,
    pub thread: Option<ThreadInfo>,
    #[serde(default)]
    pub my_permissions: u64,
    #[serde(default)]
    pub icon_url: String,
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
