//! XChaCha20-Poly1305 recovery-slot wrapping for `MasterKey`.
//!
//! Recovery-slot ciphertext is stored in the vault header (plaintext JSON in
//! the cloud), so it must be bound to vault identity. The AAD is
//! `b"arx-runa recovery v1" || vault_id_bytes`, preventing cross-vault
//! transplant and cross-slot confusion with `wrap_file_key` blobs (which use
//! empty AAD).
//!
//! Wire format matches `WrappedFileKey`:
//! `[24-byte nonce | 32-byte ciphertext | 16-byte tag] = 72 bytes`.

use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use secrecy::SecretBox;
use zeroize::Zeroizing;

use crate::crypto::error::CryptoError;
use crate::crypto::nonce::generate_nonce;
use crate::crypto::types::{MasterKey, RecoveryKey, VaultId, WrappedMasterKey};

const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const WRAPPED_LEN: usize = NONCE_LEN + KEY_LEN + TAG_LEN;
const AAD_PREFIX: &[u8] = b"arx-runa recovery v1";
const AAD_LEN: usize = 20 + 16;

/// Builds the recovery-slot AAD `b"arx-runa recovery v1" || vault_id_bytes`.
fn build_aad(vault_id: &VaultId) -> [u8; AAD_LEN] {
    let mut aad = [0u8; AAD_LEN];
    aad[..20].copy_from_slice(AAD_PREFIX);
    aad[20..].copy_from_slice(vault_id.as_bytes());
    aad
}

/// Wraps `master_key` for storage in a vault-header recovery slot.
///
/// Uses XChaCha20-Poly1305 with a fresh CSPRNG nonce and non-empty AAD
/// binding the ciphertext to `vault_id`. The plaintext is copied into a
/// `Zeroizing` buffer so the in-place encryption target is zeroed on drop
/// even if the function returns early via `?`.
///
/// # Errors
/// Returns `CryptoError::KeyWrapFailed` if the underlying AEAD call fails.
/// For a 32-byte plaintext with XChaCha20-Poly1305 this is unreachable in
/// practice, but the fallible surface lets callers propagate unexpected
/// failures instead of panicking.
pub fn wrap_master_key_for_recovery(
    master_key: &MasterKey,
    recovery_key: &RecoveryKey,
    vault_id: &VaultId,
) -> Result<WrappedMasterKey, CryptoError> {
    let nonce_bytes = generate_nonce();
    let mut ciphertext: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new([0u8; KEY_LEN]);
    ciphertext.copy_from_slice(master_key.expose());

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(recovery_key.expose()));
    let nonce = GenericArray::from_slice(&nonce_bytes);
    let aad = build_aad(vault_id);

    let tag = cipher
        .encrypt_in_place_detached(nonce, &aad, ciphertext.as_mut_slice())
        .map_err(|_| CryptoError::KeyWrapFailed)?;

    let mut wire = [0u8; WRAPPED_LEN];
    wire[..NONCE_LEN].copy_from_slice(&nonce_bytes);
    wire[NONCE_LEN..NONCE_LEN + KEY_LEN].copy_from_slice(ciphertext.as_slice());
    wire[NONCE_LEN + KEY_LEN..].copy_from_slice(tag.as_slice());

    Ok(WrappedMasterKey(wire))
}

/// Unwraps a `WrappedMasterKey`, returning a fresh `MasterKey`.
///
/// Decryption runs inside a `SecretBox<[u8; 32]>` via `init_with_mut`, so on
/// authentication failure the partial-keystream buffer is zeroized by the
/// `SecretBox`'s `Drop` rather than lingering on the stack.
///
/// # Errors
/// Returns `CryptoError::DecryptionFailed` if the authentication tag does
/// not verify. This includes: wrong `recovery_key`, wrong `vault_id` (AAD
/// mismatch — blocks cross-vault transplant), and tampered wire bytes.
pub fn unwrap_master_key_from_recovery(
    wrapped: &WrappedMasterKey,
    recovery_key: &RecoveryKey,
    vault_id: &VaultId,
) -> Result<MasterKey, CryptoError> {
    let nonce_slice = &wrapped.0[..NONCE_LEN];
    let ciphertext_slice = &wrapped.0[NONCE_LEN..NONCE_LEN + KEY_LEN];
    let tag_slice = &wrapped.0[NONCE_LEN + KEY_LEN..];

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(recovery_key.expose()));
    let nonce = GenericArray::from_slice(nonce_slice);
    let tag = GenericArray::from_slice(tag_slice);
    let aad = build_aad(vault_id);

    let mut decrypt_result: Result<(), chacha20poly1305::Error> = Ok(());
    let master_key_secret_box = SecretBox::<[u8; KEY_LEN]>::init_with_mut(|buffer| {
        buffer.copy_from_slice(ciphertext_slice);
        decrypt_result = cipher.decrypt_in_place_detached(nonce, &aad, buffer.as_mut_slice(), tag);
    });

    match decrypt_result {
        Ok(()) => Ok(MasterKey::from_secret_box(master_key_secret_box)),
        Err(_) => Err(CryptoError::DecryptionFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        WRAPPED_LEN, unwrap_master_key_from_recovery, wrap_master_key_for_recovery,
    };
    use crate::crypto::error::CryptoError;
    use crate::crypto::types::{MasterKey, RecoveryKey, VaultId, WrappedMasterKey};

    fn make_master_key(byte: u8) -> MasterKey {
        MasterKey::from_bytes([byte; 32])
    }

    fn make_recovery_key(byte: u8) -> RecoveryKey {
        RecoveryKey::from_bytes([byte; 32])
    }

    fn make_vault_id(byte: u8) -> VaultId {
        VaultId::new([byte; 16])
    }

    #[test]
    fn test_wrap_unwrap_recovery_round_trip_returns_original_master_key() {
        let master_key = make_master_key(0xA1);
        let recovery_key = make_recovery_key(0xB2);
        let vault_id = make_vault_id(0x33);
        let original_bytes = *master_key.expose();

        let wrapped = wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
            .expect("wrap must succeed");
        let recovered = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id)
            .expect("round trip must succeed");

        assert_eq!(*recovered.expose(), original_bytes);
    }

    #[test]
    fn test_wrap_recovery_two_calls_produce_distinct_wrapped_blobs() {
        let master_key = make_master_key(0xCD);
        let recovery_key = make_recovery_key(0xEF);
        let vault_id = make_vault_id(0x44);

        let first = wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
            .expect("wrap must succeed");
        let second = wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
            .expect("wrap must succeed");

        assert_ne!(
            first.0, second.0,
            "random nonce must make wrapped blobs differ"
        );
        assert_ne!(first.0[..24], second.0[..24], "nonce prefix must differ");
    }

    #[test]
    fn test_unwrap_recovery_wrong_recovery_key_fails_with_decryption_failed() {
        let master_key = make_master_key(0x11);
        let vault_id = make_vault_id(0x22);
        let wrapped = wrap_master_key_for_recovery(&master_key, &make_recovery_key(0x33), &vault_id)
            .expect("wrap must succeed");

        let result = unwrap_master_key_from_recovery(&wrapped, &make_recovery_key(0x44), &vault_id);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_recovery_wrong_vault_id_fails_with_decryption_failed() {
        let master_key = make_master_key(0x11);
        let recovery_key = make_recovery_key(0x22);
        let wrapped_for_vault_a =
            wrap_master_key_for_recovery(&master_key, &recovery_key, &make_vault_id(0xAA))
                .expect("wrap must succeed");

        let result = unwrap_master_key_from_recovery(
            &wrapped_for_vault_a,
            &recovery_key,
            &make_vault_id(0xBB),
        );

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_recovery_corrupted_nonce_fails_with_decryption_failed() {
        let master_key = make_master_key(0x11);
        let recovery_key = make_recovery_key(0x22);
        let vault_id = make_vault_id(0x33);
        let mut wrapped = wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
            .expect("wrap must succeed");

        wrapped.0[0] ^= 0x01;

        let result = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_recovery_corrupted_ciphertext_fails_with_decryption_failed() {
        let master_key = make_master_key(0x11);
        let recovery_key = make_recovery_key(0x22);
        let vault_id = make_vault_id(0x33);
        let mut wrapped = wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
            .expect("wrap must succeed");

        wrapped.0[24 + 5] ^= 0x01;

        let result = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_recovery_corrupted_tag_fails_with_decryption_failed() {
        let master_key = make_master_key(0x11);
        let recovery_key = make_recovery_key(0x22);
        let vault_id = make_vault_id(0x33);
        let mut wrapped = wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
            .expect("wrap must succeed");

        let tag_index = wrapped.0.len() - 1;
        wrapped.0[tag_index] ^= 0x01;

        let result = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_wrap_recovery_wire_format_is_seventy_two_bytes() {
        let wrapped = wrap_master_key_for_recovery(
            &make_master_key(0xAA),
            &make_recovery_key(0xBB),
            &make_vault_id(0xCC),
        )
        .expect("wrap must succeed");

        assert_eq!(wrapped.0.len(), WRAPPED_LEN);
        assert_eq!(WRAPPED_LEN, 72);
    }

    #[test]
    fn test_unwrap_recovery_all_zero_wrapped_blob_fails_with_decryption_failed() {
        let recovery_key = make_recovery_key(0x22);
        let vault_id = make_vault_id(0x33);
        let wrapped = WrappedMasterKey([0u8; 72]);

        let result = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_wrap_recovery_uses_non_empty_aad_scope_separation_from_file_key() {
        let key_bytes = [0x55u8; 32];
        let master_key = MasterKey::from_bytes(key_bytes);
        let recovery_key = RecoveryKey::from_bytes([0x77u8; 32]);
        let vault_id = make_vault_id(0x88);

        let wrapped_recovery =
            wrap_master_key_for_recovery(&master_key, &recovery_key, &vault_id)
                .expect("wrap must succeed");

        // A recovery-wrapped blob with vault id A and AAD must not decrypt
        // under the no-AAD file-key wrap path even with the same key bytes.
        use crate::crypto::types::{FileKey, KeyEncryptionKey, WrappedFileKey};
        use crate::crypto::wrap_key::unwrap_file_key;

        let pretend_file_wrapped = WrappedFileKey(wrapped_recovery.0);
        let kek_same_bytes = KeyEncryptionKey::from_bytes([0x77u8; 32]);
        let result: Result<FileKey, _> = unwrap_file_key(&pretend_file_wrapped, &kek_same_bytes);

        assert!(
            matches!(result, Err(CryptoError::DecryptionFailed)),
            "recovery-AAD wrap must not be decryptable via empty-AAD file-key unwrap"
        );
    }
}
