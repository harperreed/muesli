// ABOUTME: Utility functions for slugging, timestamps, and helpers
// ABOUTME: Provides consistent filename generation and time formatting

use crate::model::TimestampValue;

pub fn slugify(text: &str) -> String {
    slug::slugify(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Q4 Planning!!!"), "q4-planning");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("Föö Bär"), "foo-bar");
        assert_eq!(slugify("Test@#$%123"), "test-123");
    }
}

pub fn normalize_timestamp(ts: &TimestampValue) -> Option<String> {
    match ts {
        TimestampValue::Seconds(secs) => {
            let total_secs = *secs as u64;
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            Some(format!("{:02}:{:02}:{:02}", hours, minutes, seconds))
        }
        TimestampValue::String(s) => {
            // Try to parse and normalize HH:MM:SS.sss -> HH:MM:SS
            if let Some(pos) = s.find('.') {
                Some(s[..pos].to_string())
            } else {
                Some(s.clone())
            }
        }
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;
    use crate::model::TimestampValue;

    #[test]
    fn test_normalize_timestamp_seconds() {
        let ts = TimestampValue::Seconds(3665.5);
        assert_eq!(normalize_timestamp(&ts), Some("01:01:05".into()));
    }

    #[test]
    fn test_normalize_timestamp_string() {
        let ts = TimestampValue::String("00:12:34.567".into());
        assert_eq!(normalize_timestamp(&ts), Some("00:12:34".into()));
    }

    #[test]
    fn test_normalize_timestamp_string_no_subseconds() {
        let ts = TimestampValue::String("00:05:10".into());
        assert_eq!(normalize_timestamp(&ts), Some("00:05:10".into()));
    }
}
