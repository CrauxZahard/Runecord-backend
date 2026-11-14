//! Shared identifier and timestamp helpers.

/// Every persisted Runecord entity uses a signed 64-bit identifier.
pub type Id = i64;

/// Unix milliseconds, matching the JSON wire format.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
