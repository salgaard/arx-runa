//! Fuzz `parse_chunk_size_bytes` string-to-u64 validation.
//!
//! `chunk_size_bytes` is stored as a string in `manifest_meta` and parsed
//! back to a `u64` with range validation on every vault open. The value
//! originates from a cloud-synced SQLCipher backup (untrusted after sync).
//!
//! Valid range: 131 072 (128 KiB) to 67 108 864 (64 MiB). Out-of-range
//! integers and non-numeric strings must return errors without panicking.

#![no_main]

use arx_runa_tauri_lib::fuzz_api::parse_chunk_size_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only valid UTF-8 strings reach `parse_chunk_size_bytes` in production;
    // filter here so libFuzzer learns to generate valid strings faster.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse_chunk_size_bytes(s);
    }
});
