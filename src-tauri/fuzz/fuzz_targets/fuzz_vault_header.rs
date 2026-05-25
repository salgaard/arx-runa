//! Fuzz `VaultHeader` JSON deserialization + structural validation.
//!
//! The vault header arrives as JSON from the cloud and is parsed before any
//! authentication takes place. Malformed or adversarially crafted JSON must
//! not cause panics or unsafe behaviour. This target exercises:
//!
//! - `serde_json` UTF-8 parsing and JSON deserialization
//! - `VaultHeader::validate_structure`: base64/hex decoding, length assertions,
//!   tier range check, schema version check, recovery-slot field validation

#![no_main]

use arx_runa_tauri_lib::storage::cloud::vault_header::VaultHeader;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize arbitrary bytes as a VaultHeader JSON blob.
    // On success, validate structural invariants. Neither step must panic.
    if let Ok(header) = serde_json::from_slice::<VaultHeader>(data) {
        let _ = header.validate_structure();
        let _ = header.validate();
    }
});
