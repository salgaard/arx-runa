//! Fuzz `decrypt_manifest_backup` wire-format parsing.
//!
//! Manifest backup blobs are downloaded from cloud storage (untrusted source)
//! and parsed before the manifest key is even verified. The wire format is:
//!
//!   `[24-byte nonce | ciphertext | 16-byte tag]`
//!
//! Malformed inputs (too short, wrong lengths, garbage slices) must not
//! cause panics or unsafe behaviour. Only the `CryptoError::DecryptionFailed`
//! variant is expected on adversarial input; everything else is a bug.

#![no_main]

use arx_runa_tauri_lib::crypto::types::VaultId;
use arx_runa_tauri_lib::fuzz_api::decrypt_manifest_backup;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fixed key and vault_id: we are testing the parsing path, not
    // the cryptographic correctness. Decryption will always fail on random
    // input, but it must fail gracefully without panicking.
    let manifest_key = [0u8; 32];
    let vault_id = VaultId::new([0u8; 16]);
    let _ = decrypt_manifest_backup(data, &manifest_key, &vault_id);
});
