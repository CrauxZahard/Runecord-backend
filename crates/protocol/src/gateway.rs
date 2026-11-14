//! Gateway events, commands and envelopes.

use serde::{Deserialize, Serialize};
use super::*;

// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", content = "d", rename_all = "snake_case")]
pub enum GatewayEvent {
    Ready(Box<Bootstrap>),
    MessageCreate(Box<Message>),
    MessageUpdate(Box<Message>),
    MessageDelete {
        channel_id: Id,
        message_id: Id,
    },
    ReactionUpdate(Box<Message>),
    TypingStart {
        channel_id: Id,
        user_id: Id,
        until_ms: i64,
    },
    PresenceUpdate {
        user_id: Id,
        status: Presence,
    },
    ChannelCreate(Box<Channel>),
    ChannelUpdate(Box<Channel>),
    ChannelDelete {
        channel_id: Id,
        guild_id: Option<Id>,
    },
    GuildCreate(Box<Guild>),
    GuildUpdate(Box<Guild>),
    GuildDelete {
        guild_id: Id,
    },
    RoleCreate(Box<Role>),
    RoleUpdate(Box<Role>),
    RoleDelete {
        guild_id: Id,
        role_id: Id,
    },
    MemberUpdate(Box<Member>),
    MemberRemove {
        guild_id: Id,
        user_id: Id,
    },
    MemberCountUpdate {
        guild_id: Id,
        count: u32,
    },
    DmCreate(Box<DmChannel>),
    DmUpdate(Box<DmChannel>),
    DmRemove {
        channel_id: Id,
        user_id: Id,
    },
    GuildMembers {
        guild_id: Id,
        members: Vec<Member>,
    },
    RelationshipUpdate(Box<Relationship>),
    RelationshipRemove {
        user_id: Id,
    },
    ReadUpdate {
        channel_id: Id,
        last_read_id: Option<Id>,
        mention_count: u32,
    },
    UserUpdate(Box<User>),
    VoiceStateUpdate(Box<VoiceState>),
    InteractionCreate {
        app_id: Id,
        payload: serde_json::Value,
    },
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
