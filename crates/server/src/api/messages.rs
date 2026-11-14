//! messages.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct MessageQuery {
    before: Option<Id>,
    limit: Option<u32>,
}

pub(crate) async fn list_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Query(q): Query<MessageQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let messages = store::list_messages_window(&conn, channel, me, q.before, None, None, limit);
    Ok(Json(messages))
}

pub(crate) async fn send_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Json(body): Json<SendMessage>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_send(&conn, channel, me) {
        return Err(forbidden());
    }
    let content = body.content.trim_end().to_string();
    if content.is_empty() && body.attachment_ids.is_empty() {
        return Err(bad("Message can't be empty."));
    }
    let target = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    if let Some(guild) = target.guild_id {
        let until = store::member_timeout(&conn, guild, me);
        if until > now_ms() {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "You are timed out and cannot send messages right now.".into(),
            ));
        }
    }
    if target.slowmode > 0 {
        let perms = store::channel_permissions(&conn, channel, me);
        if !has(perms, perm::MANAGE_MESSAGES)
            && !has(perms, perm::MANAGE_CHANNELS)
            && let Some(last) = store::last_message_timestamp(&conn, channel, me)
        {
            let wait = last + target.slowmode as i64 * 1000 - now_ms();
            if wait > 0 {
                return Err(ApiError(
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "Slowmode: wait {}s before posting again.",
                        (wait + 999) / 1000
                    ),
                ));
            }
        }
    }
    let forum_title = (target.kind == ChannelKind::Forum).then(|| thread_title(&content));
    let (msg, new_thread) = store::create_message(
        &conn,
        me,
        &target,
        content,
        body.reply_to,
        &body.attachment_ids,
        forum_title.as_deref(),
    )
    .map_err(internal)?;
    drop(conn);
    if let Some(thread) = new_thread {
        let _ = state.tx.send(GatewayEvent::ChannelCreate(Box::new(thread)));
    }
    let _ = state
        .tx
        .send(GatewayEvent::MessageCreate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn edit_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(body): Json<EditMessage>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let Some((author, channel)) = store::message_location(&conn, id) else {
        return Err(not_found());
    };
    if author != me
        && !has(
            store::channel_permissions(&conn, channel, me),
            perm::MANAGE_MESSAGES,
        )
    {
        return Err(forbidden());
    }
    store::set_message_content(&conn, id, body.content.trim_end(), now_ms()).map_err(internal)?;
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::MessageUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn delete_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let Some((author, channel)) = store::message_location(&conn, id) else {
        return Err(not_found());
    };
    if author != me
        && !has(
            store::channel_permissions(&conn, channel, me),
            perm::MANAGE_MESSAGES,
        )
    {
        return Err(forbidden());
    }
    store::delete_message(&conn, id, channel).map_err(internal)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageDelete {
        channel_id: channel,
        message_id: id,
    });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn toggle_reaction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(body): Json<ReactionToggle>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let Some((_, channel)) = store::message_location(&conn, id) else {
        return Err(not_found());
    };
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let emoji = body.emoji.chars().take(8).collect::<String>();
    let msg = store::toggle_reaction(&conn, id, me, &emoji).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ReactionUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let msg = store::toggle_pin(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::MessageUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn list_pins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let out = store::pinned_ids(&conn, channel)
        .into_iter()
        .filter_map(|id| store::message_by_id(&conn, id, me))
        .collect();
    Ok(Json(out))
}

pub(crate) async fn typing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    drop(conn);
    let _ = state.tx.send(GatewayEvent::TypingStart {
        channel_id: channel,
        user_id: me,
        until_ms: now_ms() + 8000,
    });
    Ok(Json(json!({ "ok": true })))
}

fn thread_title(content: &str) -> String {
    let line = content.lines().next().unwrap_or("New thread");
    let cleaned: String = line
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '~' | '|' | '>' | '#'))
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = collapsed.chars().take(60).collect::<String>();
    if name.trim().is_empty() {
        "New thread".to_string()
    } else {
        name
    }
}

pub(crate) async fn list_threads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    Ok(Json(
        store::list_threads(&conn, channel, me)
            .into_iter()
            .map(|thread| channel_for(&conn, thread, me))
            .collect::<Vec<_>>(),
    ))
}

pub(crate) async fn create_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Json(body): Json<CreateMessageThread>,
) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let parent = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let name = if body.name.trim().is_empty() {
        body.message_id
            .and_then(|mid| store::message_content(&conn, mid))
            .map(|content| thread_title(&content))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "New thread".to_string())
    } else {
        body.name.trim().chars().take(80).collect()
    };
    let thread =
        store::create_thread(&conn, me, &parent, &name, body.message_id).map_err(internal)?;
    let thread = channel_for(&conn, thread, me);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ChannelCreate(Box::new(thread.clone())));
    Ok(Json(thread))
}

// ---------------------------------------------------------------------------
// Uploads
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

#[derive(Default)]
struct SearchFilters {
    text: String,
    from: Option<String>,
    in_channel: Option<String>,
    mentions: Option<String>,
    has_link: bool,
    before: Option<i64>,
    after: Option<i64>,
}

fn parse_search(raw: &str) -> SearchFilters {
    let mut filters = SearchFilters::default();
    let mut words = Vec::new();
    for token in raw.split_whitespace() {
        let lower = token.to_lowercase();
        if let Some(value) = lower.strip_prefix("from:")
            && !value.is_empty()
        {
            filters.from = Some(value.to_string());
            continue;
        }
        if let Some(value) = lower.strip_prefix("in:")
            && !value.is_empty()
        {
            filters.in_channel = Some(value.trim_start_matches('#').to_string());
            continue;
        }
        if let Some(value) = lower.strip_prefix("mentions:")
            && !value.is_empty()
        {
            filters.mentions = Some(value.trim_start_matches('@').to_string());
            continue;
        }
        if lower == "has:link" {
            filters.has_link = true;
            continue;
        }
        if let Some(value) = lower.strip_prefix("before:") {
            filters.before = parse_day(value);
            if filters.before.is_some() {
                continue;
            }
        }
        if let Some(value) = lower.strip_prefix("after:") {
            filters.after = parse_day(value);
            if filters.after.is_some() {
                continue;
            }
        }
        words.push(token);
    }
    filters.text = words.join(" ");
    filters
}

fn parse_day(value: &str) -> Option<i64> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)
        .map(|date| date.and_utc().timestamp_millis())
}

pub(crate) async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResults>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let filters = parse_search(&q.q);
    let term = if filters.text.trim().is_empty() {
        String::new()
    } else {
        format!("%{}%", filters.text.trim())
    };
    if q.q.trim().is_empty() {
        return Ok(Json(SearchResults {
            total: 0,
            hits: Vec::new(),
        }));
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let rows: Vec<(Id, Id)> = store::search_message_ids(&conn, &term, 800)
        .into_iter()
        .filter(|(_, channel)| store::can_view(&conn, *channel, me))
        .collect();
    let mentions_name = filters.mentions.clone();
    let mut hits = Vec::new();
    for (id, ch) in rows {
        if hits.len() as u32 >= limit {
            break;
        }
        let Some(message) = store::message_by_id(&conn, id, me) else {
            continue;
        };
        let Some(channel) = store::channel_by_id(&conn, ch) else {
            continue;
        };
        let author = store::user_by_id(&conn, message.author_id).unwrap_or_default();
        if let Some(from) = &filters.from {
            let matches = author.username.to_lowercase().contains(from)
                || author.display_name.to_lowercase().contains(from);
            if !matches {
                continue;
            }
        }
        if let Some(name) = &filters.in_channel
            && !channel.name.to_lowercase().contains(name)
        {
            continue;
        }
        if filters.has_link {
            let content = message.content.to_lowercase();
            if !(content.contains("http://")
                || content.contains("https://")
                || content.contains("www."))
            {
                continue;
            }
        }
        if let Some(before) = filters.before
            && message.timestamp >= before
        {
            continue;
        }
        if let Some(after) = filters.after
            && message.timestamp < after
        {
            continue;
        }
        if let Some(mention) = &mentions_name {
            let mut matched = false;
            for user_id in &message.mentions {
                if let Some(user) = store::user_by_id(&conn, *user_id)
                    && (user.username.to_lowercase().contains(mention)
                        || user.display_name.to_lowercase().contains(mention))
                {
                    matched = true;
                    break;
                }
            }
            if !matched {
                continue;
            }
        }
        let guild = channel
            .guild_id
            .and_then(|guild_id| store::guild_name(&conn, guild_id));
        hits.push(SearchHit {
            message,
            channel_id: ch,
            channel_name: channel.name,
            guild_id: channel.guild_id,
            guild_name: guild,
            author,
        });
    }
    Ok(Json(SearchResults {
        total: hits.len() as u32,
        hits,
    }))
}

// ---------------------------------------------------------------------------
// Friends and DMs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_parses_discord_filters() {
        let filters = parse_search("fire from:maya in:general has:link after:2026-01-01");
        assert_eq!(filters.text, "fire");
        assert_eq!(filters.from.as_deref(), Some("maya"));
        assert_eq!(filters.in_channel.as_deref(), Some("general"));
        assert!(filters.has_link);
        assert!(filters.after.is_some());
        assert!(filters.before.is_none());
    }

    #[test]
    fn search_keeps_unknown_filters_as_text() {
        let filters = parse_search("hello before:not-a-date");
        assert_eq!(filters.text, "hello before:not-a-date");
        assert!(filters.before.is_none());
    }
}
