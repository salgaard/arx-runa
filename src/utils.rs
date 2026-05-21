//! Timestamp formatting utilities for relative time display.

use base64::Engine;
use sha2::{Digest, Sha256};

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

/// Calculates the fingerprint from a base64-encoded 32-byte public key.
///
/// The fingerprint is the first 8 bytes of SHA-256(public_key), rendered as 16 lowercase hex characters.
/// This follows the sharing module contract and enables users to verify contact identity out-of-band.
///
/// # Arguments
/// * `public_key_b64` - Base64-encoded 32-byte X25519 public key
///
/// # Returns
/// 16 lowercase hex characters (e.g., `0a1b2c3d4e5f6789`), or the input if decoding fails.
pub fn format_fingerprint(public_key_b64: &str) -> String {
    match base64::engine::general_purpose::STANDARD.decode(public_key_b64) {
        Ok(public_key_bytes) if public_key_bytes.len() == 32 => {
            let mut hasher = Sha256::new();
            hasher.update(&public_key_bytes);
            let hash = hasher.finalize();
            // Take the first 8 bytes and format as 16 hex chars
            let fingerprint_bytes = &hash[..8];
            fingerprint_bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        }
        _ => {
            // Invalid key encoding; return empty string
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that fingerprint formatting produces correct 16-char hex output.
    #[test]
    fn test_format_fingerprint_produces_16_hex_chars() {
        // Create a test 32-byte key (all zeros)
        let test_key = [0u8; 32];
        let test_key_b64 = base64::engine::general_purpose::STANDARD.encode(test_key);

        let fingerprint = format_fingerprint(&test_key_b64);

        // Should be exactly 16 characters (2 hex digits per byte × 8 bytes)
        assert_eq!(fingerprint.len(), 16);

        // Should be lowercase hex characters only
        assert!(
            fingerprint.chars().all(|c| "0123456789abcdef".contains(c)),
            "Got fingerprint: {}",
            fingerprint
        );
    }

    /// Test that fingerprint is unique for different public keys.
    #[test]
    fn test_format_fingerprint_unique_for_different_keys() {
        let mut key1 = [0u8; 32];
        key1[0] = 1;
        let key1_b64 = base64::engine::general_purpose::STANDARD.encode(key1);

        let mut key2 = [0u8; 32];
        key2[0] = 2;
        let key2_b64 = base64::engine::general_purpose::STANDARD.encode(key2);

        let fingerprint1 = format_fingerprint(&key1_b64);
        let fingerprint2 = format_fingerprint(&key2_b64);

        assert_ne!(fingerprint1, fingerprint2);
    }

    /// Test that invalid base64 returns empty string.
    #[test]
    fn test_format_fingerprint_invalid_base64() {
        let fingerprint = format_fingerprint("not-valid-base64!!!!");
        assert_eq!(fingerprint, "");
    }

    /// Test that wrong-sized key returns empty string.
    #[test]
    fn test_format_fingerprint_wrong_size_key() {
        // Base64 encode a 31-byte key instead of 32
        let test_key = [0u8; 31];
        let test_key_b64 = base64::engine::general_purpose::STANDARD.encode(test_key);

        let fingerprint = format_fingerprint(&test_key_b64);
        assert_eq!(fingerprint, "");
    }
}
