//! Users, presence, profiles and sessions.

use serde::{Deserialize, Serialize};
use super::*;

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

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    #[default]
    Everyone,
    ServerMembers,
    Nobody,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileActivity {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub started_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileConnection {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
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
    #[serde(default)]
    pub pronouns: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub banner_url: String,
    #[serde(default)]
    pub badges: Vec<String>,
    #[serde(default)]
    pub avatar_decoration: String,
    #[serde(default)]
    pub profile_effect: String,
    #[serde(default)]
    pub nameplate: String,
    #[serde(default)]
    pub profile_widget: String,
    #[serde(default)]
    pub activity: Option<ProfileActivity>,
    #[serde(default)]
    pub connections: Vec<ProfileConnection>,
    pub status: Presence,
    #[serde(default)]
    pub is_bot: bool,
    pub created_at: i64,
    #[serde(default)]
    pub dm_privacy: PrivacyLevel,
    #[serde(default)]
    pub friend_privacy: PrivacyLevel,
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
