use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn path_to_string_lossy(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

pub(super) fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) fn now_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

pub(super) fn new_local_user_id() -> String {
    format!("local-{:x}-{:x}", now_unix_nanos(), std::process::id())
}

pub(super) fn new_session_id() -> String {
    format!("session-{:x}", now_unix_nanos())
}
