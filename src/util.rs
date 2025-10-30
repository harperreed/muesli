// ABOUTME: Utility functions for slugging, timestamps, and helpers
// ABOUTME: Provides consistent filename generation and time formatting

use crate::model::TimestampValue;
use chrono::{DateTime, Utc};

pub fn slugify(text: &str) -> String {
    let slug = slug::slugify(text);
    // Handle empty slugs (happens when title is only special chars)
    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Q4 Planning!!!"), "q4-planning");
        assert_eq!(slugify(""), "untitled");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("Föö Bär"), "foo-bar");
        assert_eq!(slugify("Test@#$%123"), "test-123");
        assert_eq!(slugify("!!!@@@###"), "untitled"); // Only special chars
    }
}

pub fn normalize_timestamp(ts: &str) -> Option<String> {
    // Try to parse as ISO 8601 datetime
    if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
        // Extract time portion and format as HH:MM:SS
        return Some(dt.format("%H:%M:%S").to_string());
    }

    // Fallback: try to parse as HH:MM:SS or HH:MM:SS.sss
    if let Some(pos) = ts.find('.') {
        Some(ts[..pos].to_string())
    } else if ts.contains(':') {
        Some(ts.to_string())
    } else {
        None
    }
}

// Legacy function for backward compatibility with old TimestampValue
pub fn normalize_timestamp_legacy(ts: &TimestampValue) -> Option<String> {
    match ts {
        TimestampValue::Seconds(secs) => {
            let total_secs = *secs as u64;
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            Some(format!("{:02}:{:02}:{:02}", hours, minutes, seconds))
        }
        TimestampValue::String(s) => normalize_timestamp(s),
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;
    use crate::model::TimestampValue;

    #[test]
    fn test_normalize_timestamp_iso8601() {
        assert_eq!(
            normalize_timestamp("2025-10-01T21:35:24.568Z"),
            Some("21:35:24".into())
        );
        assert_eq!(
            normalize_timestamp("2025-10-01T09:05:10.000Z"),
            Some("09:05:10".into())
        );
    }

    #[test]
    fn test_normalize_timestamp_hms() {
        assert_eq!(normalize_timestamp("00:12:34.567"), Some("00:12:34".into()));
        assert_eq!(normalize_timestamp("00:05:10"), Some("00:05:10".into()));
    }

    #[test]
    fn test_normalize_timestamp_legacy_seconds() {
        let ts = TimestampValue::Seconds(3665.5);
        assert_eq!(normalize_timestamp_legacy(&ts), Some("01:01:05".into()));
    }

    #[test]
    fn test_normalize_timestamp_legacy_string() {
        let ts = TimestampValue::String("00:12:34.567".into());
        assert_eq!(normalize_timestamp_legacy(&ts), Some("00:12:34".into()));
    }
}
