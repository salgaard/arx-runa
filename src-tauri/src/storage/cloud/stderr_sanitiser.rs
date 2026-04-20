//! Rclone stderr redaction helper.

const SENSITIVE_KEYWORDS: [&str; 6] = ["token", "key", "secret", "password", "credential", "auth"];

/// Drops stderr lines containing sensitive credential-like keywords.
pub fn sanitise_stderr(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let filtered: Vec<&str> = raw
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !SENSITIVE_KEYWORDS
                .iter()
                .any(|keyword| lower.contains(keyword))
        })
        .collect();

    if filtered.is_empty() {
        "<credentials scrubbed>".to_owned()
    } else {
        filtered.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::sanitise_stderr;

    #[test]
    fn test_sanitise_stderr_drops_token_lines() {
        assert_eq!(sanitise_stderr("prefix token=123\nsafe"), "safe");
    }

    #[test]
    fn test_sanitise_stderr_is_case_insensitive() {
        assert_eq!(sanitise_stderr("AUTH failed\nsafe"), "safe");
    }

    #[test]
    fn test_sanitise_stderr_preserves_line_granularity() {
        assert_eq!(
            sanitise_stderr("safe one\nsecret line\nsafe two"),
            "safe one\nsafe two"
        );
    }

    #[test]
    fn test_sanitise_stderr_returns_placeholder_when_everything_scrubbed() {
        assert_eq!(sanitise_stderr("password bad"), "<credentials scrubbed>");
    }

    #[test]
    fn test_sanitise_stderr_keeps_empty_input() {
        assert_eq!(sanitise_stderr(""), "");
    }
}
