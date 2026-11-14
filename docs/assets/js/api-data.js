/* Runecord native API surface (kept in sync with crates/server/src/api/mod.rs). */
window.RUNECORD_API = {
  base: "http://127.0.0.1:47800",
  groups: [
    {
      id: "system",
      name: "System & session",
      blurb: "Health, versioning, bootstrap and the login lifecycle.",
      endpoints: [
        { m: "GET", p: "/health", d: "Liveness probe. Returns the plain text body ok.", a: "Public" },
        { m: "GET", p: "/api/version", d: "Server name and version.", a: "Public" },
        { m: "GET", p: "/api/bootstrap", d: "Full client bootstrap: user, guilds, DMs, channels and read state.", a: "Session" },
        { m: "GET", p: "/api/cache", d: "In-process guild and thumbnail cache statistics.", a: "Session" },
        { m: "POST", p: "/api/register", d: "Create a local account. Returns a session token and user.", a: "Public" },
        { m: "POST", p: "/api/login", d: "Exchange username and password for a session token.", a: "Public" },
        { m: "POST", p: "/api/logout", d: "Revoke the current session token.", a: "Session" }
      ]
    },
    {
      id: "users",
      name: "Users & profiles",
      blurb: "Profiles, avatars, banners, activity artwork, privacy and read state.",
      endpoints: [
        { m: "PATCH", p: "/api/me", d: "Update display name, bio, custom status, colors and profile settings.", a: "Session" },
        { m: "POST", p: "/api/me/avatar", d: "Upload an avatar image.", a: "Session" },
        { m: "DELETE", p: "/api/me/avatar", d: "Remove the current avatar.", a: "Session" },
        { m: "POST", p: "/api/me/banner", d: "Upload a profile banner.", a: "Session" },
        { m: "DELETE", p: "/api/me/banner", d: "Remove the profile banner.", a: "Session" },
        { m: "POST", p: "/api/me/activity/artwork", d: "Upload artwork for the activity card.", a: "Session" },
        { m: "DELETE", p: "/api/me/activity/artwork", d: "Remove activity artwork.", a: "Session" },
        { m: "POST", p: "/api/me/password", d: "Change the account password.", a: "Session" },
        { m: "POST", p: "/api/me/privacy", d: "Update privacy and discoverability settings.", a: "Session" },
        { m: "GET", p: "/api/users/{id}", d: "Fetch a user profile by ID.", a: "Session / Bot" },
        { m: "GET", p: "/api/users/lookup?q=", d: "Look up users by name or handle.", a: "Session / Bot" },
        { m: "GET", p: "/api/users/{id}/mutuals", d: "List mutual guilds with another user.", a: "Session / Bot" },
        { m: "POST", p: "/api/users/batch", d: "Fetch several user profiles in one request.", a: "Session / Bot" },
        { m: "POST", p: "/api/read", d: "Update read state for a channel.", a: "Session" }
      ]
    },
    {
      id: "guilds",
      name: "Guilds",
      blurb: "Servers, discovery, boosts, icons, notification settings and permissions.",
      endpoints: [
        { m: "POST", p: "/api/guilds", d: "Create a guild and make the caller its owner.", a: "Session" },
        { m: "GET", p: "/api/guilds/{id}", d: "Guild detail: channels, roles, members and the caller's permissions.", a: "Session / Bot" },
        { m: "PATCH", p: "/api/guilds/{id}", d: "Update guild name, description and settings.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}", d: "Delete a guild the caller owns.", a: "Session" },
        { m: "GET", p: "/api/public-guilds", d: "Browse the public server directory.", a: "Session" },
        { m: "POST", p: "/api/public-guilds/{id}/join", d: "Join a listed public guild.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/channels", d: "Create a text, voice, forum or category channel.", a: "Session / Bot" },
        { m: "GET", p: "/api/guilds/{id}/commands", d: "List application commands installed in a guild.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/leave", d: "Leave a guild.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/boost", d: "Apply a guild boost.", a: "Session" },
        { m: "DELETE", p: "/api/guilds/{id}/boost", d: "Remove a guild boost.", a: "Session" },
        { m: "GET", p: "/api/guilds/{id}/notifications", d: "Read server notification settings and overrides.", a: "Session" },
        { m: "PATCH", p: "/api/guilds/{id}/notifications", d: "Update server notification settings.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/icon", d: "Upload a guild icon.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/icon", d: "Remove the guild icon.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/banner", d: "Upload a guild banner.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/banner", d: "Remove the guild banner.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/discoverable", d: "Toggle public discovery for a guild.", a: "Session" },
        { m: "GET", p: "/api/guilds/{id}/permissions", d: "The caller's computed base permissions in a guild.", a: "Session / Bot" },
        { m: "GET", p: "/api/guilds/{id}/audit-logs", d: "Read the guild audit log.", a: "Session" }
      ]
    },
    {
      id: "members",
      name: "Members & moderation",
      blurb: "Member pages, kicks, bans, timeouts, pruning, appeals and the inbox.",
      endpoints: [
        { m: "GET", p: "/api/guilds/{id}/members", d: "Paginated member list with role and presence data.", a: "Session / Bot" },
        { m: "GET", p: "/api/guilds/{id}/members/{user_id}", d: "Fetch one guild member.", a: "Session / Bot" },
        { m: "PATCH", p: "/api/guilds/{id}/members/{user_id}", d: "Update nicknames, roles or member flags.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/members/{user_id}/kick", d: "Kick a member.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/members/{user_id}/ban", d: "Ban a member.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/members/{user_id}/timeout", d: "Apply or clear a member timeout.", a: "Session / Bot" },
        { m: "GET", p: "/api/guilds/{id}/bans", d: "List banned users.", a: "Session" },
        { m: "DELETE", p: "/api/guilds/{id}/bans/{user_id}", d: "Unban a user.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/prune/preview", d: "Preview how many members a prune would remove.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/prune", d: "Prune inactive members.", a: "Session" },
        { m: "GET", p: "/api/guilds/{id}/appeals", d: "List ban appeals for a guild.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/appeals/{appeal}/accept", d: "Accept a ban appeal.", a: "Session" },
        { m: "POST", p: "/api/guilds/{id}/appeals/{appeal}/deny", d: "Deny a ban appeal.", a: "Session" },
        { m: "POST", p: "/api/invites/{code}/appeal", d: "Submit an appeal from a ban screen.", a: "Session / Bot" },
        { m: "GET", p: "/api/inbox", d: "Mentions, unreads and inbox items for the caller.", a: "Session" }
      ]
    },
    {
      id: "channels",
      name: "Channels & threads",
      blurb: "Channel settings, overwrites, threads, typing, uploads, pins and emoji.",
      endpoints: [
        { m: "PATCH", p: "/api/channels/{id}", d: "Update a channel's name, topic, position or flags.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/channels/{id}", d: "Delete a channel.", a: "Session / Bot" },
        { m: "GET", p: "/api/channels/{id}/permissions", d: "List channel permission overwrites.", a: "Session / Bot" },
        { m: "PUT", p: "/api/channels/{id}/permissions/{target}", d: "Create or replace an overwrite for a role or member.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/channels/{id}/permissions/{target}", d: "Clear a channel overwrite.", a: "Session / Bot" },
        { m: "PATCH", p: "/api/channels/{id}/thread", d: "Update thread state, title or archive status.", a: "Session / Bot" },
        { m: "GET", p: "/api/channels/{id}/threads", d: "List active threads in a channel.", a: "Session / Bot" },
        { m: "POST", p: "/api/channels/{id}/threads", d: "Create a thread from a channel or message.", a: "Session / Bot" },
        { m: "POST", p: "/api/channels/{id}/typing", d: "Broadcast a typing indicator.", a: "Session / Bot" },
        { m: "POST", p: "/api/channels/{id}/upload", d: "Upload an attachment to a channel.", a: "Session / Bot" },
        { m: "GET", p: "/cdn/{file}", d: "Fetch an encrypted CDN asset; supports thumbnail widths.", a: "Public" },
        { m: "GET", p: "/api/channels/{id}/pins", d: "List pinned messages.", a: "Session / Bot" },
        { m: "POST", p: "/api/channels/{id}/notifications", d: "Set the caller's notification level for a channel.", a: "Session" },
        { m: "GET", p: "/api/guilds/{id}/emojis", d: "List custom guild emoji.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/emojis", d: "Upload a custom guild emoji.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/emojis/{emoji}", d: "Delete a custom emoji.", a: "Session / Bot" }
      ]
    },
    {
      id: "messages",
      name: "Messages",
      blurb: "Send, edit, react, pin and search inside channels.",
      endpoints: [
        { m: "GET", p: "/api/channels/{id}/messages", d: "Fetch a message window with before/after/around pagination.", a: "Session / Bot" },
        { m: "POST", p: "/api/channels/{id}/messages", d: "Send a message, optionally as a reply or thread post.", a: "Session / Bot" },
        { m: "PATCH", p: "/api/messages/{id}", d: "Edit a message's content or embeds.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/messages/{id}", d: "Delete a message.", a: "Session / Bot" },
        { m: "POST", p: "/api/messages/{id}/reactions", d: "Toggle a reaction for the current user or bot.", a: "Session / Bot" },
        { m: "POST", p: "/api/messages/{id}/pin", d: "Toggle the pinned state of a message.", a: "Session / Bot" },
        { m: "GET", p: "/api/search", d: "Search messages the caller can see.", a: "Session" }
      ]
    },
    {
      id: "roles",
      name: "Roles & invites",
      blurb: "Role CRUD, icons, role links, invites and join flows.",
      endpoints: [
        { m: "GET", p: "/api/guilds/{id}/roles", d: "List guild roles in hierarchy order.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/roles", d: "Create a role.", a: "Session / Bot" },
        { m: "PATCH", p: "/api/guilds/{id}/roles/{role_id}", d: "Update role name, color, permissions or position.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/roles/{role_id}", d: "Delete a role.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/roles/{role_id}/icon", d: "Upload a role icon.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/roles/{role_id}/icon", d: "Remove a role icon.", a: "Session / Bot" },
        { m: "GET", p: "/api/guilds/{id}/roles/{role_id}/links", d: "List links attached to a role.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/roles/{role_id}/links", d: "Attach a link to a role.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/roles/{role_id}/links/{link}", d: "Remove a role link.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/invites", d: "Create an invite to a guild.", a: "Session / Bot" },
        { m: "GET", p: "/api/invites/{code}", d: "Preview an invite code.", a: "Public" },
        { m: "POST", p: "/api/invites/{code}/join", d: "Join a guild through an invite.", a: "Session / Bot" }
      ]
    },
    {
      id: "social",
      name: "Friends & DMs",
      blurb: "Friend requests, blocks, one-to-one DMs and group conversations.",
      endpoints: [
        { m: "POST", p: "/api/friends/{id}/request", d: "Send a friend request.", a: "Session" },
        { m: "POST", p: "/api/friends/{id}/accept", d: "Accept a pending friend request.", a: "Session" },
        { m: "POST", p: "/api/friends/{id}/remove", d: "Remove a friend.", a: "Session" },
        { m: "POST", p: "/api/friends/{id}/block", d: "Block a user.", a: "Session" },
        { m: "POST", p: "/api/dms/group", d: "Create a group DM.", a: "Session" },
        { m: "POST", p: "/api/dms/{id}", d: "Open or create a one-to-one DM.", a: "Session" },
        { m: "PATCH", p: "/api/dms/{id}", d: "Rename a group DM or update its settings.", a: "Session" },
        { m: "POST", p: "/api/dms/{id}/icon", d: "Upload a group DM icon.", a: "Session" },
        { m: "DELETE", p: "/api/dms/{id}/icon", d: "Remove a group DM icon.", a: "Session" },
        { m: "POST", p: "/api/dms/{id}/members", d: "Add a member to a group DM.", a: "Session" },
        { m: "DELETE", p: "/api/dms/{id}/members/{user}", d: "Remove a member from a group DM.", a: "Session" },
        { m: "POST", p: "/api/dms/{id}/leave", d: "Leave a group DM.", a: "Session" }
      ]
    },
    {
      id: "events",
      name: "Scheduled events",
      blurb: "Create and manage events on a server.",
      endpoints: [
        { m: "GET", p: "/api/guilds/{id}/events", d: "List scheduled events for a guild.", a: "Session / Bot" },
        { m: "POST", p: "/api/guilds/{id}/events", d: "Create a scheduled event.", a: "Session / Bot" },
        { m: "DELETE", p: "/api/guilds/{id}/events/{event}", d: "Delete a scheduled event.", a: "Session / Bot" }
      ]
    },
    {
      id: "bots",
      name: "Bots",
      blurb: "Bot accounts are users with is_bot set, owned by a human session.",
      endpoints: [
        { m: "GET", p: "/api/bots", d: "List bots owned by the caller.", a: "Session" },
        { m: "POST", p: "/api/bots", d: "Create a bot. Returns the bot user and a token shown once.", a: "Session" },
        { m: "DELETE", p: "/api/bots/{id}", d: "Revoke a bot token and remove the bot from its guilds.", a: "Session" }
      ]
    },
    {
      id: "apps",
      name: "Applications & interactions",
      blurb: "Apps, install links, application commands, tokens and interaction callbacks.",
      endpoints: [
        { m: "GET", p: "/api/applications", d: "List applications owned by the caller.", a: "Session" },
        { m: "POST", p: "/api/applications", d: "Create an application and its backing bot.", a: "Session" },
        { m: "DELETE", p: "/api/applications/{id}", d: "Delete an application.", a: "Session" },
        { m: "POST", p: "/api/applications/{id}/install-link", d: "Create a shareable install link.", a: "Session" },
        { m: "POST", p: "/api/applications/install/{code}", d: "Install an application into a guild.", a: "Session" },
        { m: "POST", p: "/api/applications/{id}/token", d: "Issue or rotate the application bot token.", a: "Session" },
        { m: "GET", p: "/api/applications/{id}/commands", d: "List an application's commands.", a: "Session" },
        { m: "POST", p: "/api/applications/{id}/commands", d: "Register an application command.", a: "Session" },
        { m: "DELETE", p: "/api/applications/{id}/commands/{command}", d: "Delete an application command.", a: "Session" },
        { m: "POST", p: "/api/interactions", d: "Invoke an application command from the client.", a: "Session / Bot" },
        { m: "POST", p: "/api/interactions/{id}/{token}/callback", d: "Respond to a pending interaction.", a: "App" }
      ]
    },
    {
      id: "webhooks",
      name: "Webhooks",
      blurb: "Channel webhooks and token-authenticated execution.",
      endpoints: [
        { m: "GET", p: "/api/channels/{id}/webhooks", d: "List webhooks in a channel.", a: "Session" },
        { m: "POST", p: "/api/channels/{id}/webhooks", d: "Create a webhook.", a: "Session" },
        { m: "DELETE", p: "/api/webhooks/{id}", d: "Delete a webhook.", a: "Session" },
        { m: "POST", p: "/api/webhooks/{id}/{token}", d: "Execute a webhook with its token. No bearer auth needed.", a: "Webhook token" }
      ]
    },
    {
      id: "gateway",
      name: "Gateway & voice",
      blurb: "Realtime sockets for events and voice signalling.",
      endpoints: [
        { m: "GET", p: "/gateway", d: "Native WebSocket gateway using the { op, d } envelope.", a: "Session / Bot" },
        { m: "GET", p: "/gateway/v1", d: "Discord-compatible gateway with HELLO, IDENTIFY and RESUME.", a: "Bot token" },
        { m: "GET", p: "/voice", d: "Voice signalling WebSocket.", a: "Session / Bot" },
        { m: "POST", p: "/api/voice", d: "Update the caller's voice state.", a: "Session / Bot" }
      ]
    }
  ]
};

/* Discord-shaped compatibility surface mounted at /api/v1. */
window.RUNECORD_V1 = [
  { m: "GET", p: "/api/v1/users/@me", d: "Current user with string IDs and ISO timestamps." },
  { m: "GET", p: "/api/v1/users/@me/guilds", d: "Guilds the current user belongs to." },
  { m: "GET", p: "/api/v1/users/{id}", d: "User profile." },
  { m: "GET", p: "/api/v1/guilds/{guild_id}", d: "Guild object." },
  { m: "GET", p: "/api/v1/guilds/{guild_id}/channels", d: "Guild channels." },
  { m: "GET", p: "/api/v1/guilds/{guild_id}/members", d: "Guild members." },
  { m: "PUT", p: "/api/v1/guilds/{guild_id}/members/{user_id}/roles/{role_id}", d: "Add a role to a member." },
  { m: "DELETE", p: "/api/v1/guilds/{guild_id}/members/{user_id}/roles/{role_id}", d: "Remove a role from a member." },
  { m: "GET", p: "/api/v1/channels/{channel_id}", d: "Channel object." },
  { m: "GET", p: "/api/v1/channels/{channel_id}/messages", d: "Message window with before, after, around and limit." },
  { m: "POST", p: "/api/v1/channels/{channel_id}/messages", d: "Send a message with content, nonce and message_reference." },
  { m: "GET", p: "/api/v1/channels/{channel_id}/messages/{message_id}", d: "Fetch one message." },
  { m: "PATCH", p: "/api/v1/channels/{channel_id}/messages/{message_id}", d: "Edit a message." },
  { m: "DELETE", p: "/api/v1/channels/{channel_id}/messages/{message_id}", d: "Delete a message." },
  { m: "PUT", p: "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me", d: "Add a reaction as the current user." },
  { m: "DELETE", p: "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me", d: "Remove the current user's reaction." },
  { m: "GET", p: "/api/v1/channels/{channel_id}/pins", d: "Pinned messages." },
  { m: "PUT", p: "/api/v1/channels/{channel_id}/pins/{message_id}", d: "Pin a message." },
  { m: "DELETE", p: "/api/v1/channels/{channel_id}/pins/{message_id}", d: "Unpin a message." },
  { m: "POST", p: "/api/v1/channels/{channel_id}/typing", d: "Typing indicator." },
  { m: "GET", p: "/api/v1/gateway", d: "Gateway URL for the current session." },
  { m: "GET", p: "/api/v1/gateway/bot", d: "Gateway URL for a bot." }
];

/* Curated response examples, keyed by "METHOD path". Anything not listed
   falls back to a representative shape for its resource group. */
(function () {
  var user = {
    id: 7,
    username: "maya",
    display_name: "Maya Chen",
    avatar_color: 6,
    bio: "building tiny UIs",
    custom_status: "shipping",
    status: "online",
    is_bot: false,
    created_at: 1794000000000
  };

  var guild = {
    id: 1,
    name: "Rune Lounge",
    icon_color: 5,
    owner_id: 1,
    description: "A home for interface obsessives.",
    member_count: 8,
    tag: "",
    permissions: 1024,
    unread_count: 0,
    mention_count: 0,
    created_at: 1794000000000,
    discoverable: true
  };

  var channel = {
    id: 12,
    guild_id: 1,
    name: "general",
    kind: "text",
    position: 2,
    parent_id: null,
    topic: "Talk about anything and everything",
    status: "",
    nsfw: false,
    slowmode: 0,
    last_message_id: 9081,
    owner_id: null,
    thread: null,
    my_permissions: 1024,
    icon_url: ""
  };

  var role = {
    id: 3,
    guild_id: 1,
    name: "Moderator",
    color: 3447003,
    permissions: 1024,
    position: 1,
    hoist: true,
    mentionable: true,
    icon_url: ""
  };

  var member = {
    guild_id: 1,
    user: user,
    nickname: "",
    roles: [3],
    joined_at: 1794000000000,
    timeout_until: 0
  };

  var message = {
    id: 9081,
    channel_id: 12,
    author_id: 7,
    content: "shipped it.",
    timestamp: 1794691200123,
    edited_at: null,
    attachments: [],
    reactions: [
      { emoji: "🔥", count: 3, me: false, user_ids: [1, 4, 8] }
    ],
    pinned: false,
    thread_id: null,
    mentions: [],
    mention_everyone: false
  };

  var invite = {
    code: "rune-lounge",
    guild_id: 1,
    guild_name: "Rune Lounge",
    guild_icon_color: 5,
    inviter_id: 1,
    uses: 0,
    created_at: 1794000000000,
    member_count: 8
  };

  var bot = {
    id: 42,
    username: "runebot",
    display_name: "Runebot",
    avatar_color: 4,
    bio: "",
    custom_status: "",
    status: "online",
    is_bot: true,
    created_at: 1794000000000
  };

  var application = {
    id: 3,
    owner_id: 1,
    name: "Runebot Tools",
    description: "Utilities for Rune Lounge",
    icon_color: 35071,
    bot: bot,
    scopes: ["bot", "applications.commands"],
    permissions: 0,
    created_at: 1794000000000
  };

  var command = {
    id: 18,
    app_id: 3,
    name: "ping",
    description: "Check the bot's latency",
    options: []
  };

  var webhook = {
    id: 7,
    guild_id: 1,
    channel_id: 12,
    name: "CI",
    avatar_color: 3447003,
    token: "rcwh.9a1f…"
  };

  var audit = {
    id: 91,
    guild_id: 1,
    actor_id: 1,
    action_type: 20,
    target_id: 7,
    changes: [{ key: "nickname", new_value: "maya" }],
    created_at: 1794691200123
  };

  var event = {
    id: 4,
    guild_id: 1,
    creator_id: 1,
    name: "Design review",
    description: "Walk through the next interface pass.",
    start_at: 1794777600000,
    channel_id: 12,
    created_at: 1794691200123
  };

  var inboxEntry = {
    channel_id: 12,
    guild_id: 1,
    message_id: 9081,
    guild_name: "Rune Lounge",
    channel_name: "general",
    author_id: 7,
    author_name: "Maya Chen",
    snippet: "shipped it.",
    timestamp: 1794691200123,
    unread: 0,
    mention: true
  };

  var dm = {
    channel: {
      id: 22,
      guild_id: null,
      name: "",
      kind: "dm",
      position: 0,
      parent_id: null,
      topic: "",
      status: "",
      nsfw: false,
      slowmode: 0,
      last_message_id: null,
      owner_id: 7,
      thread: null,
      my_permissions: 0,
      icon_url: ""
    },
    recipients: [user]
  };

  window.RUNECORD_RESPONSES = {
    "GET /health": { status: "200 OK", body: "ok" },
    "GET /api/version": { body: { name: "runecord", version: "0.1.0" } },
    "POST /api/register": { body: { token: "4f9c2a8e1c7d…", user: user } },
    "POST /api/login": { body: { token: "4f9c2a8e1c7d…", user: user } },
    "POST /api/logout": { body: { ok: true } },
    "GET /api/bootstrap": {
      note: "Abbreviated: real bootstraps include every guild, DM, read state and presence the user can see.",
      body: {
        user: user,
        guilds: [guild],
        dms: [],
        relationships: [],
        reads: [{ channel_id: 12, last_read_id: 9080, mention_count: 0 }],
        users: [user],
        presences: { "7": "online" },
        unread: { "12": 0 },
        emojis: [],
        voice_states: []
      }
    },
    "PATCH /api/me": { body: user },
    "GET /api/users/{id}": { body: user },
    "GET /api/users/lookup?q=": { body: [user] },
    "GET /api/users/{id}/mutuals": { body: [guild] },
    "POST /api/users/batch": { body: [user] },
    "POST /api/read": { body: { ok: true } },
    "POST /api/guilds": { body: guild },
    "GET /api/guilds/{id}": {
      body: {
        guild: guild,
        channels: [channel],
        roles: [role],
        overwrites: [],
        permissions: 1024
      }
    },
    "PATCH /api/guilds/{id}": { body: guild },
    "DELETE /api/guilds/{id}": { body: { ok: true } },
    "GET /api/public-guilds": { body: [guild] },
    "POST /api/public-guilds/{id}/join": { body: guild },
    "POST /api/guilds/{id}/channels": { body: channel },
    "GET /api/guilds/{id}/commands": { body: [command] },
    "POST /api/guilds/{id}/leave": { body: { ok: true } },
    "POST /api/guilds/{id}/boost": { body: guild },
    "DELETE /api/guilds/{id}/boost": { body: guild },
    "GET /api/guilds/{id}/permissions": {
      body: { permissions: 1024, channels: { "12": 1024, "13": 1024 } }
    },
    "GET /api/guilds/{id}/audit-logs": { body: [audit] },
    "GET /api/guilds/{id}/members": {
      body: { members: [member], total: 8, next_offset: null, presences: { "7": "online" } }
    },
    "GET /api/guilds/{id}/members/{user_id}": { body: member },
    "PATCH /api/guilds/{id}/members/{user_id}": { body: member },
    "POST /api/guilds/{id}/members/{user_id}/kick": { body: { ok: true } },
    "POST /api/guilds/{id}/members/{user_id}/ban": { body: { ok: true } },
    "POST /api/guilds/{id}/members/{user_id}/timeout": { body: member },
    "GET /api/guilds/{id}/bans": {
      body: [{ user: user, reason: "spam", created_at: 1794691200123 }]
    },
    "DELETE /api/guilds/{id}/bans/{user_id}": { body: { ok: true } },
    "POST /api/guilds/{id}/prune/preview": { body: { count: 3 } },
    "POST /api/guilds/{id}/prune": { body: { pruned: 3 } },
    "GET /api/inbox": { body: { mentions: [inboxEntry], unreads: [inboxEntry] } },
    "PATCH /api/channels/{id}": { body: channel },
    "DELETE /api/channels/{id}": { body: { ok: true } },
    "GET /api/channels/{id}/permissions": {
      body: [{ target_id: 3, target_type: "role", allow: 1024, deny: 0 }]
    },
    "GET /api/channels/{id}/threads": { body: [channel] },
    "POST /api/channels/{id}/threads": { body: channel },
    "POST /api/channels/{id}/typing": { body: { ok: true } },
    "POST /api/channels/{id}/upload": {
      body: {
        attachment: {
          id: 55,
          filename: "screenshot.png",
          size: 184320,
          content_type: "image/png",
          url: "/cdn/ab12cd34.png",
          width: 1920,
          height: 1080
        }
      }
    },
    "GET /cdn/{file}": {
      status: "200 OK",
      body: "<binary asset, optionally resized with ?w=480>"
    },
    "GET /api/channels/{id}/pins": { body: [message] },
    "GET /api/channels/{id}/webhooks": { body: [webhook] },
    "POST /api/channels/{id}/webhooks": { body: webhook },
    "GET /api/guilds/{id}/emojis": {
      body: [{ id: 4, guild_id: 1, name: "rune", sticker: false, url: "/cdn/rune.png" }]
    },
    "GET /api/channels/{id}/messages": { body: [message] },
    "POST /api/channels/{id}/messages": { body: message },
    "PATCH /api/messages/{id}": { body: message },
    "DELETE /api/messages/{id}": { body: { ok: true } },
    "POST /api/messages/{id}/reactions": { body: message },
    "POST /api/messages/{id}/pin": { body: message },
    "GET /api/search": {
      body: [
        {
          message: message,
          channel_id: 12,
          channel_name: "general",
          guild_id: 1,
          guild_name: "Rune Lounge",
          author: user
        }
      ]
    },
    "GET /api/guilds/{id}/roles": { body: [role] },
    "POST /api/guilds/{id}/roles": { body: role },
    "PATCH /api/guilds/{id}/roles/{role_id}": { body: role },
    "DELETE /api/guilds/{id}/roles/{role_id}": { body: { ok: true } },
    "POST /api/guilds/{id}/invites": { body: invite },
    "GET /api/invites/{code}": { body: invite },
    "POST /api/invites/{code}/join": { body: guild },
    "POST /api/friends/{id}/request": { body: { ok: true } },
    "POST /api/friends/{id}/accept": { body: { ok: true } },
    "POST /api/friends/{id}/remove": { body: { ok: true } },
    "POST /api/friends/{id}/block": { body: { ok: true } },
    "POST /api/dms/group": { body: dm },
    "POST /api/dms/{id}": { body: dm },
    "PATCH /api/dms/{id}": { body: dm },
    "GET /api/guilds/{id}/events": { body: [event] },
    "POST /api/guilds/{id}/events": { body: event },
    "DELETE /api/guilds/{id}/events/{event}": { body: { ok: true } },
    "GET /api/bots": { body: [bot] },
    "POST /api/bots": { body: { bot: bot, token: "rcbot.9f2c…" } },
    "DELETE /api/bots/{id}": { body: { ok: true } },
    "GET /api/applications": { body: [application] },
    "POST /api/applications": { body: { application: application, token: "rcbot.9f2c…" } },
    "DELETE /api/applications/{id}": { body: { ok: true } },
    "POST /api/applications/{id}/token": { body: { token: "rcbot.9f2c…" } },
    "GET /api/applications/{id}/commands": { body: [command] },
    "POST /api/applications/{id}/commands": { body: command },
    "DELETE /api/applications/{id}/commands/{command}": { body: { ok: true } },
    "POST /api/interactions": { body: { id: "9001", token: "8b1d…" } },
    "POST /api/interactions/{id}/{token}/callback": { body: { ok: true } },
    "DELETE /api/webhooks/{id}": { body: { ok: true } },
    "POST /api/webhooks/{id}/{token}": { body: message },
    "POST /api/voice": {
      body: { user_id: 7, guild_id: 1, channel_id: 3, self_mute: false, self_deaf: false }
    },
    "GET /gateway": {
      status: "101 SWITCHING PROTOCOLS",
      body: "Upgrade: websocket\nConnection: Upgrade\n\n{\"op\":\"ready\",\"d\":{\"user\":{\"id\":7,...}}}"
    },
    "GET /gateway/v1": {
      status: "101 SWITCHING PROTOCOLS",
      body: "{\"op\":10,\"d\":{\"heartbeat_interval\":41250}}"
    },
    "GET /voice": {
      status: "101 SWITCHING PROTOCOLS",
      body: "{\"op\":\"voice_ready\",\"d\":{\"channel_id\":3,\"participants\":[]}}"
    }
  };
})();
