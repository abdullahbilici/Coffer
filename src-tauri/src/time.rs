use std::time::{SystemTime, UNIX_EPOCH};

use objc2_foundation::{NSDate, NSDateFormatter, NSLocale, NSString};

pub fn to_millis(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

pub fn now_millis() -> i64 {
    to_millis(SystemTime::now())
}

/// Today in local time, formatted with a fixed English Gregorian locale whatever the user's
/// region settings are (`yyyy-MM-dd`, or `EEEE, d MMMM yyyy` for "Friday, 23 October 2026").
pub fn local_today(format: &str) -> String {
    let formatter = NSDateFormatter::new();
    formatter.setLocale(Some(&NSLocale::localeWithLocaleIdentifier(
        &NSString::from_str("en_US_POSIX"),
    )));
    formatter.setDateFormat(Some(&NSString::from_str(format)));
    formatter.stringFromDate(&NSDate::now()).to_string()
}
