//! Timestamp formatting utilities for relative time display.

/// Formats an ISO-8601 timestamp into a relative human-readable string.
///
/// Examples: "just now", "5 minutes ago", "2 hours ago", "1 day ago"
pub fn format_relative_time(iso_timestamp: &str) -> String {
    // Parse the ISO timestamp using js_sys::Date
    let then = js_sys::Date::new(&iso_timestamp.into());
    let now = js_sys::Date::new_0();

    // Get milliseconds since epoch
    let then_ms = then.get_time();
    let now_ms = now.get_time();

    // Calculate seconds difference
    let seconds_diff = ((now_ms - then_ms) / 1000.0) as i64;

    if seconds_diff < 60 {
        "just now".to_string()
    } else if seconds_diff < 3600 {
        let minutes = seconds_diff / 60;
        format!(
            "{} minute{} ago",
            minutes,
            if minutes == 1 { "" } else { "s" }
        )
    } else if seconds_diff < 86400 {
        let hours = seconds_diff / 3600;
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else if seconds_diff < 604800 {
        let days = seconds_diff / 86400;
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else {
        let weeks = seconds_diff / 604800;
        format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_format_relative_time_recent() {
        // This is hard to test without a real timestamp, so we skip for now
        // Real tests would use mock time
    }
}
