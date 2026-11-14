//! Mutation request bodies.

use super::*;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub slowmode: u32,
    #[serde(default)]
    pub nsfw: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateGuild {
    pub name: Option<String>,
    pub icon_color: Option<u32>,
    pub description: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub discoverable: Option<bool>,
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
    #[serde(default)]
    pub pronouns: Option<String>,
    #[serde(default)]
    pub badges: Option<Vec<String>>,
    #[serde(default)]
    pub avatar_decoration: Option<String>,
    pub avatar_color: Option<u32>,
    #[serde(default)]
    pub avatar_border: Option<u32>,
    #[serde(default)]
    pub banner_color: Option<u32>,
    pub bio: Option<String>,
    pub custom_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdatePrivacy {
    pub dm_privacy: PrivacyLevel,
    pub friend_privacy: PrivacyLevel,
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
    #[serde(default)]
    pub position: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateRole {
    pub name: String,
    pub color: u32,
    pub permissions: u64,
}

// ---------------------------------------------------------------------------
// Gateway
