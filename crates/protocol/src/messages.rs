//! Messages, attachments, reactions and replies.

use serde::{Deserialize, Serialize};
use super::*;

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
