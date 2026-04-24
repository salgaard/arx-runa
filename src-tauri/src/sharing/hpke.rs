//! HPKE Base-mode seal/open for Arx Runa share packages.
//!
//! Ciphersuite: `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305`.
//!
//! Wire format: `[32B enc | ciphertext | 32B CTX tag]` (minimum 64 bytes).
//!
//! This module implements RFC 9180 Base-mode manually rather than using the
//! `hpke` crate, because the CTX-ChaCha20-Poly1305 committing construction
//! uses a 32-byte BLAKE3 tag and a 24-byte XChaCha20 nonce which are not
//! compatible with the sealed AEAD trait in `hpke` v0.13. DHKEM uses
//! `x25519-dalek` directly to avoid a `rand_core` version conflict between
//! `hpke` 0.13 (`rand_core 0.9`) and the project's `rand 0.10`
//! (`rand_core 0.10`).
//!
//! IANA identifiers in `suite_id`:
//! - KEM: `0x0020` — DHKEM(X25519, HKDF-SHA256)
//! - KDF: `0x0001` — HKDF-SHA256
//! - AEAD: `0x0003` — ChaCha20-Poly1305 (CTX is wire-equivalent; tag only)

use hkdf::{Hkdf, HkdfExtract};
use rand::Rng;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey as DalekPublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::sharing::ctx_aead::{ctx_open, ctx_seal};
use crate::sharing::error::SharingError;
use crate::sharing::types::X25519PublicKey;

/// HPKE info string for share-package encryption.
pub(crate) const HPKE_SHARE_INFO: &[u8] = b"arx-runa-share";

/// HPKE version label per RFC 9180 §4.
const VERSION_LABEL: &[u8] = b"HPKE-v1";

/// KEM suite ID: `"KEM" || I2OSP(0x0020, 2)` per RFC 9180 §4.1.
const KEM_SUITE_ID: &[u8] = &[b'K', b'E', b'M', 0x00, 0x20];

/// Full HPKE suite ID per RFC 9180 §4:
/// `"HPKE" || I2OSP(0x0020, 2) || I2OSP(0x0001, 2) || I2OSP(0x0003, 2)`.
///
/// AEAD ID `0x0003` is registered with `Nn=12`, but this implementation
/// expands `base_nonce` to 24 bytes for XChaCha20. The `I2OSP(L, 2)` length
/// prefix in `LabeledExpand` provides domain separation. This suite ID is
/// intentionally non-interoperable with standard HPKE implementations.
const SUITE_ID: &[u8] = &[b'H', b'P', b'K', b'E', 0x00, 0x20, 0x00, 0x01, 0x00, 0x03];

/// Encapsulated key length for X25519 (32 bytes).
const ENC_LEN: usize = 32;

/// CTX commitment tag length (32 bytes).
const CTX_TAG_LEN: usize = 32;

/// Minimum valid wire length: `enc (32) + tag (32)` = 64 bytes.
const MIN_WIRE_LEN: usize = ENC_LEN + CTX_TAG_LEN;

/// XChaCha20 nonce length (24 bytes).
const NONCE_LEN: usize = 24;

/// Key-and-nonce pair produced by the HPKE key schedule.
type KeyScheduleOutput = (Zeroizing<[u8; KEY_LEN]>, Zeroizing<[u8; NONCE_LEN]>);

/// ChaCha20 key length (32 bytes).
const KEY_LEN: usize = 32;

/// RFC 9180 §4 `LabeledExtract(salt, label, ikm)`:
/// `Extract(salt, "HPKE-v1" || suite_id || label || ikm)`.
///
/// Returns the PRK bytes and an `Hkdf` context suitable for `labeled_expand`.
fn labeled_extract(
    suite_id: &[u8],
    salt: &[u8],
    label: &[u8],
    ikm: &[u8],
) -> (Zeroizing<[u8; 32]>, Hkdf<Sha256>) {
    let mut extract = HkdfExtract::<Sha256>::new(Some(salt));
    extract.input_ikm(VERSION_LABEL);
    extract.input_ikm(suite_id);
    extract.input_ikm(label);
    extract.input_ikm(ikm);
    let (prk_output, hkdf) = extract.finalize();
    let mut prk_bytes = Zeroizing::new([0u8; 32]);
    prk_bytes.copy_from_slice(&prk_output);
    (prk_bytes, hkdf)
}

/// RFC 9180 §4 `LabeledExpand(prk, label, info, L)`:
/// `Expand(prk, I2OSP(L, 2) || "HPKE-v1" || suite_id || label || info, L)`.
///
/// Writes the expansion result into `output`; the length `L` is `output.len()`.
fn labeled_expand(
    hkdf: &Hkdf<Sha256>,
    suite_id: &[u8],
    label: &[u8],
    info: &[u8],
    output: &mut [u8],
) -> Result<(), SharingError> {
    let length_bytes = (output.len() as u16).to_be_bytes();
    hkdf.expand_multi_info(
        &[&length_bytes, VERSION_LABEL, suite_id, label, info],
        output,
    )
    .map_err(|_| SharingError::Backend("HKDF expand length exceeded".to_owned()))
}

/// RFC 9180 §4.1 DHKEM `ExtractAndExpand(dh, kem_context)` using `KEM_SUITE_ID`.
fn extract_and_expand(
    diffie_hellman_bytes: &[u8; 32],
    kem_context: &[u8; 64],
) -> Result<Zeroizing<[u8; 32]>, SharingError> {
    let (_, eae_hkdf) = labeled_extract(KEM_SUITE_ID, &[], b"eae_prk", diffie_hellman_bytes);
    let mut shared_secret = Zeroizing::new([0u8; 32]);
    labeled_expand(
        &eae_hkdf,
        KEM_SUITE_ID,
        b"shared_secret",
        kem_context,
        &mut *shared_secret,
    )?;
    Ok(shared_secret)
}

/// DHKEM(X25519) Encap: generates an ephemeral keypair, performs DH, and
/// derives the shared secret per RFC 9180 §4.1.
///
/// Returns `(shared_secret, enc)` where `enc` is the 32-byte ephemeral public key.
fn kem_encap(
    recipient_public_key: &DalekPublicKey,
) -> Result<(Zeroizing<[u8; 32]>, [u8; 32]), SharingError> {
    let mut ephemeral_bytes = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(ephemeral_bytes.as_mut_slice());

    let ephemeral_secret = StaticSecret::from(*ephemeral_bytes);
    let ephemeral_public_key = DalekPublicKey::from(&ephemeral_secret);
    let diffie_hellman = ephemeral_secret.diffie_hellman(recipient_public_key);

    if diffie_hellman.as_bytes().ct_eq(&[0u8; 32]).into() {
        return Err(SharingError::AuthenticationFailed);
    }

    let enc = *ephemeral_public_key.as_bytes();
    let mut kem_context = [0u8; 64];
    kem_context[..32].copy_from_slice(&enc);
    kem_context[32..].copy_from_slice(recipient_public_key.as_bytes());

    let shared_secret = extract_and_expand(diffie_hellman.as_bytes(), &kem_context)?;
    Ok((shared_secret, enc))
}

/// DHKEM(X25519) Decap: performs DH with the recipient's static key and the
/// encapsulated ephemeral public key, then derives the shared secret.
fn kem_decap(
    recipient_private_key_bytes: &[u8; 32],
    enc: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, SharingError> {
    let recipient_secret = StaticSecret::from(*recipient_private_key_bytes);
    let recipient_public_key = DalekPublicKey::from(&recipient_secret);
    let enc_public_key = DalekPublicKey::from(*enc);
    let diffie_hellman = recipient_secret.diffie_hellman(&enc_public_key);

    if diffie_hellman.as_bytes().ct_eq(&[0u8; 32]).into() {
        return Err(SharingError::AuthenticationFailed);
    }

    let mut kem_context = [0u8; 64];
    kem_context[..32].copy_from_slice(enc);
    kem_context[32..].copy_from_slice(recipient_public_key.as_bytes());

    extract_and_expand(diffie_hellman.as_bytes(), &kem_context)
}

/// RFC 9180 §5.1 HPKE key schedule for Base mode (`mode = 0x00`).
///
/// Derives a 32-byte encryption key and a 24-byte nonce from the KEM shared
/// secret. The nonce is 24 bytes (XChaCha20) rather than the standard 12
/// bytes, matching our CTX-ChaCha20-Poly1305 construction.
fn key_schedule(shared_secret: &[u8; 32]) -> Result<KeyScheduleOutput, SharingError> {
    let (psk_id_hash, _) = labeled_extract(SUITE_ID, &[], b"psk_id_hash", &[]);
    let (info_hash, _) = labeled_extract(SUITE_ID, &[], b"info_hash", HPKE_SHARE_INFO);

    let mut key_schedule_context = [0u8; 65];
    key_schedule_context[0] = 0x00;
    key_schedule_context[1..33].copy_from_slice(psk_id_hash.as_ref());
    key_schedule_context[33..65].copy_from_slice(info_hash.as_ref());

    let (_, secret_hkdf) = labeled_extract(SUITE_ID, shared_secret, b"secret", &[]);

    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    labeled_expand(
        &secret_hkdf,
        SUITE_ID,
        b"key",
        &key_schedule_context,
        &mut *key,
    )?;

    let mut base_nonce = Zeroizing::new([0u8; NONCE_LEN]);
    labeled_expand(
        &secret_hkdf,
        SUITE_ID,
        b"base_nonce",
        &key_schedule_context,
        &mut *base_nonce,
    )?;

    Ok((key, base_nonce))
}

/// Encrypts `plaintext` for `recipient_public_key` using HPKE Base mode.
///
/// Returns the `.vgshare` wire bytes: `[32B enc | ciphertext | 32B CTX tag]`.
pub(crate) fn seal(
    recipient_public_key: &X25519PublicKey,
    plaintext: &[u8],
) -> Result<Vec<u8>, SharingError> {
    let dalek_public_key = DalekPublicKey::from(*recipient_public_key.as_bytes());
    let (shared_secret, enc) = kem_encap(&dalek_public_key)?;
    let (key, base_nonce) = key_schedule(&shared_secret)?;

    let mut buffer = Zeroizing::new(plaintext.to_vec());
    let tag = ctx_seal(&key, &base_nonce, &mut buffer)?;

    let mut wire = Vec::with_capacity(ENC_LEN + buffer.len() + CTX_TAG_LEN);
    wire.extend_from_slice(&enc);
    wire.extend_from_slice(&buffer);
    wire.extend_from_slice(&tag);
    Ok(wire)
}

/// Decrypts a `.vgshare` wire blob using the recipient's private key.
///
/// Returns the decrypted plaintext wrapped in `Zeroizing` for zeroize-on-drop.
/// All authentication failures (wrong key, corrupted enc, corrupted ciphertext,
/// wrong CTX tag) return `SharingError::AuthenticationFailed` with no context.
pub(crate) fn open(
    recipient_private_key_bytes: &[u8; 32],
    wire: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SharingError> {
    if wire.len() < MIN_WIRE_LEN {
        return Err(SharingError::MalformedSharePackage(
            "wire length < 64".to_owned(),
        ));
    }

    let enc: [u8; ENC_LEN] = wire[..ENC_LEN]
        .try_into()
        .map_err(|_| SharingError::MalformedSharePackage("enc length mismatch".to_owned()))?;
    let tag: &[u8; CTX_TAG_LEN] = wire[wire.len() - CTX_TAG_LEN..]
        .try_into()
        .map_err(|_| SharingError::MalformedSharePackage("tag length mismatch".to_owned()))?;
    let ciphertext = &wire[ENC_LEN..wire.len() - CTX_TAG_LEN];

    let shared_secret = kem_decap(recipient_private_key_bytes, &enc)?;
    let (key, base_nonce) = key_schedule(&shared_secret)?;

    let mut buffer = Zeroizing::new(ciphertext.to_vec());
    ctx_open(&key, &base_nonce, &mut buffer, tag)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generates a test X25519 keypair from deterministic seed bytes.
    fn test_keypair(seed: u8) -> ([u8; 32], X25519PublicKey) {
        let private_bytes = [seed; 32];
        let secret = StaticSecret::from(private_bytes);
        let public_key = DalekPublicKey::from(&secret);
        (private_bytes, X25519PublicKey::new(*public_key.as_bytes()))
    }

    /// Verifies seal/open round-trip recovers the original plaintext.
    #[test]
    fn test_hpke_seal_open_round_trip_recovers_plaintext() {
        let (private_key, public_key) = test_keypair(0xAA);
        let plaintext = b"hello from HPKE seal";

        let wire = seal(&public_key, plaintext).expect("seal should succeed");
        assert!(wire.len() >= MIN_WIRE_LEN);

        let decrypted = open(&private_key, &wire).expect("open should succeed");
        assert_eq!(&*decrypted, plaintext.as_slice());
    }

    /// Verifies a different recipient cannot decrypt the share package.
    #[test]
    fn test_hpke_wrong_recipient_rejected_with_authentication_failed() {
        let (_sender_sk, recipient_pk) = test_keypair(0xBB);
        let (wrong_sk, _wrong_pk) = test_keypair(0xCC);

        let wire = seal(&recipient_pk, b"secret payload").expect("seal should succeed");

        let result = open(&wrong_sk, &wire);
        assert!(matches!(result, Err(SharingError::AuthenticationFailed)));
    }

    /// Verifies a single-byte flip in the enc field produces `AuthenticationFailed`.
    #[test]
    fn test_hpke_corrupted_enc_rejected_with_authentication_failed() {
        let (private_key, public_key) = test_keypair(0xDD);

        let mut wire = seal(&public_key, b"test data").expect("seal should succeed");
        wire[0] ^= 0x01;

        let result = open(&private_key, &wire);
        assert!(matches!(result, Err(SharingError::AuthenticationFailed)));
    }

    /// Verifies a wire blob shorter than 64 bytes produces `MalformedSharePackage`.
    #[test]
    fn test_hpke_short_wire_rejected_with_malformed_share_package() {
        let (private_key, _public_key) = test_keypair(0xEE);
        let short_wire = vec![0u8; 63];

        let result = open(&private_key, &short_wire);
        assert!(matches!(
            result,
            Err(SharingError::MalformedSharePackage(ref message)) if message == "wire length < 64"
        ));
    }

    /// Verifies corrupted ciphertext (between enc and tag) is rejected.
    #[test]
    fn test_hpke_corrupted_ciphertext_rejected_with_authentication_failed() {
        let (private_key, public_key) = test_keypair(0xFF);
        let plaintext = b"data to corrupt in transit";

        let mut wire = seal(&public_key, plaintext).expect("seal should succeed");
        let ciphertext_start = ENC_LEN;
        wire[ciphertext_start] ^= 0x01;

        let result = open(&private_key, &wire);
        assert!(matches!(result, Err(SharingError::AuthenticationFailed)));
    }
}
