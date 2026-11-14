//! Applications, webhooks, audit entries, emoji and voice state.

use serde::{Deserialize, Serialize};
use super::*;

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
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub permissions: u64,
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
