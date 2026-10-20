use std::time::{SystemTime, UNIX_EPOCH};

pub fn to_millis(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

pub fn now_millis() -> i64 {
    to_millis(SystemTime::now())
}
