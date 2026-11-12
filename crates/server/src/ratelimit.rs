//! Discord-style rate limiting: per-actor route buckets, global bucket, and `X-RateLimit-*`
//! headers on every response.

use crate::AppState;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Bucket {
    limit: u32,
    remaining: u32,
    reset: Instant,
}

pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
    global: Mutex<HashMap<String, Bucket>>,
}

pub struct LimitInfo {
    pub limit: u32,
    pub remaining: u32,
    pub reset_after: f64,
    pub bucket: String,
    pub global: bool,
}

impl LimitInfo {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() + self.reset_after;
        let put = |headers: &mut HeaderMap, name: &'static str, value: String| {
            if let Ok(v) = HeaderValue::from_str(&value) {
                headers.insert(name, v);
            }
        };
        put(&mut headers, "x-ratelimit-limit", self.limit.to_string());
        put(&mut headers, "x-ratelimit-remaining", self.remaining.to_string());
        put(&mut headers, "x-ratelimit-reset", format!("{epoch:.3}"));
        put(&mut headers, "x-ratelimit-reset-after", format!("{:.3}", self.reset_after));
        put(&mut headers, "x-ratelimit-bucket", self.bucket.clone());
        if self.global {
            put(&mut headers, "x-ratelimit-global", "true".into());
        }
        headers
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self { buckets: Mutex::new(HashMap::new()), global: Mutex::new(HashMap::new()) }
    }

    fn take(bucket: &mut Bucket, limit: u32, window: Duration) -> (u32, f64) {
        let now = Instant::now();
        if now >= bucket.reset {
            bucket.limit = limit;
            bucket.remaining = limit;
            bucket.reset = now + window;
        }
        if bucket.remaining > 0 {
            bucket.remaining -= 1;
        }
        (bucket.remaining, bucket.reset.saturating_duration_since(now).as_secs_f64())
    }

    pub fn check(&self, actor: &str, route: &str, method: &Method, path: &str) -> Result<LimitInfo, LimitInfo> {
        let (limit, window) = policy(route, method, path);
        let global_key = actor.to_string();
        let mut global = self.global.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let g = global.entry(global_key).or_insert(Bucket { limit: 50, remaining: 50, reset: Instant::now() });
        let (g_remaining, g_reset) = Self::take(g, 50, Duration::from_secs(1));
        if g_remaining == 0 {
            return Err(LimitInfo { limit: 50, remaining: 0, reset_after: g_reset, bucket: "global".into(), global: true });
        }
        drop(global);

        let bucket_key = format!("{actor}:{method}:{route}:{path}");
        let mut buckets = self.buckets.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let b = buckets.entry(bucket_key).or_insert(Bucket { limit, remaining: limit, reset: Instant::now() + window });
        let (remaining, reset_after) = Self::take(b, limit, window);
        let info = LimitInfo {
            limit,
            remaining,
            reset_after,
            bucket: short_hash(route),
            global: false,
        };
        if remaining == 0 {
            Err(info)
        } else {
            Ok(info)
        }
    }
}

fn policy(route: &str, method: &Method, path: &str) -> (u32, Duration) {
    if route.contains("/login") || route.contains("/register") {
        return (5, Duration::from_secs(60));
    }
    if path.ends_with("/upload") {
        return (10, Duration::from_secs(60));
    }
    if route == "/api/bots" && method == Method::POST {
        return (2, Duration::from_secs(60));
    }
    if route.contains("reactions") {
        return (5, Duration::from_secs(5));
    }
    if route.ends_with("/messages") && method == Method::POST {
        return (5, Duration::from_secs(5));
    }
    if route.contains("/messages") && (method == Method::PATCH || method == Method::DELETE) {
        return (5, Duration::from_secs(5));
    }
    if method == Method::GET {
        return (60, Duration::from_secs(60));
    }
    (30, Duration::from_secs(60))
}

fn short_hash(value: &str) -> String {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn actor_key(headers: &HeaderMap) -> String {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");
    short_hash(token)
}

fn major(path: &str) -> &str {
    let parts: Vec<&str> = path.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if matches!(*part, "channels" | "guilds" | "webhooks") {
            if let Some(next) = parts.get(i + 1) {
                return next;
            }
        }
    }
    ""
}

pub async fn middleware(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let actor = actor_key(req.headers());
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let method = req.method().clone();
    let path = major(req.uri().path()).to_string();
    match state.limits.check(&actor, &route, &method, &path) {
        Ok(info) => {
            let headers = info.headers();
            let mut response = next.run(req).await;
            response.headers_mut().extend(headers);
            response
        }
        Err(info) => {
            let retry_after = info.reset_after.max(0.001);
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({
                    "code": 0,
                    "message": "You are being rate limited.",
                    "retry_after": retry_after,
                    "global": info.global
                })),
            )
                .into_response();
            response.headers_mut().extend(info.headers());
            if let Ok(v) = HeaderValue::from_str(&format!("{:.3}", retry_after)) {
                response.headers_mut().insert(header::RETRY_AFTER, v);
            }
            response
        }
    }
}
