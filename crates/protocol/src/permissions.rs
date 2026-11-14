//! Permission bit flags and checks.

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
    pub const MODERATE_MEMBERS: u64 = 1 << 40;

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
