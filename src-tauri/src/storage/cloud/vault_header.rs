//! Vault header schema forward declaration for Phase 4.1 / Phase 4.3.
//!
//! Phase 2.4 defines the serialisation shape required by vault ceremonies.
//! Phase 4.3 will adopt this struct as-is, add richer validation, and wire
//! the startup retry path for `pending-vault-header.json`. Phase 2.4
//! ceremonies uphold the `MasterKey` containment rule: this module must not
//! gain any field that stores a `MasterKey` (serialised or in memory).

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCHEMA_VERSION: u32 = 1;

/// Argon2id parameters as serialised inside the vault header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Argon2ParamsJson {
    /// Argon2id memory cost in KiB.
    pub memory_cost: u32,
    /// Argon2id time cost (iterations).
    pub time_cost: u32,
    /// Argon2id parallelism (lanes).
    pub parallelism: u32,
}

/// A recovery slot carrying a recovery-key-wrapped `master_key` blob.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoverySlot {
    /// Recovery method tag; Phase 2.4 only emits `"bip39"`.
    pub method: String,
    /// Base64-encoded 32-byte Argon2id salt for the recovery key derivation.
    pub argon2_salt: String,
    /// Argon2id parameters for the recovery key derivation.
    pub argon2_params: Argon2ParamsJson,
    /// Base64-encoded 72-byte `WrappedMasterKey` wire blob.
    pub wrapped_master_key: String,
}

/// The plaintext vault header uploaded to the cloud at `vault-header.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultHeader {
    /// Hyphenated UUID v4 string identifying the vault.
    pub vault_id: String,
    /// Schema version; Phase 2.4 uses [`VaultHeader::SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Authentication tier: `1` (password only) or `2` (password + key file).
    pub tier: u8,
    /// Base64-encoded 32-byte Argon2id salt for the primary slot.
    pub argon2_salt: String,
    /// Argon2id parameters for the primary slot.
    pub argon2_params: Argon2ParamsJson,
    /// BLAKE3 hex digest of the USB key file; `None` for tier 1.
    pub key_file_blake3: Option<String>,
    /// Recovery slots; empty until `setup_recovery` runs.
    pub recovery_slots: Vec<RecoverySlot>,
}

impl VaultHeader {
    /// Current schema version emitted by Phase 2.4 ceremonies.
    pub const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    /// Validates the structural invariants documented in
    /// `cloud-synchronisation/sub-phases/4.3-vault-header.md` deliverable 6.
    pub fn validate(&self) -> Result<(), VaultHeaderError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(VaultHeaderError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        match self.tier {
            1 => {
                if self.key_file_blake3.is_some() {
                    return Err(VaultHeaderError::Tier1WithKeyFileBlake3);
                }
            }
            2 => {
                let hex_digest = self
                    .key_file_blake3
                    .as_ref()
                    .ok_or(VaultHeaderError::Tier2MissingKeyFileBlake3)?;
                if hex_digest.len() != 64 {
                    return Err(VaultHeaderError::KeyFileBlake3WrongLength);
                }
            }
            other => return Err(VaultHeaderError::UnsupportedTier(other)),
        }
        let salt_bytes =
            base64_decode(&self.argon2_salt).map_err(|_| VaultHeaderError::SaltDecodeFailed)?;
        if salt_bytes.len() != 32 {
            return Err(VaultHeaderError::SaltWrongLength);
        }
        for slot in &self.recovery_slots {
            if slot.method != "bip39" {
                continue;
            }
            let slot_salt = base64_decode(&slot.argon2_salt)
                .map_err(|_| VaultHeaderError::RecoverySlotSaltDecodeFailed)?;
            if slot_salt.len() != 32 {
                return Err(VaultHeaderError::RecoverySlotSaltWrongLength);
            }
            let wrapped = base64_decode(&slot.wrapped_master_key)
                .map_err(|_| VaultHeaderError::RecoverySlotBlobDecodeFailed)?;
            if wrapped.len() != 72 {
                return Err(VaultHeaderError::RecoverySlotBlobWrongLength);
            }
        }
        Ok(())
    }
}

/// Errors produced while validating or parsing a [`VaultHeader`].
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum VaultHeaderError {
    /// Schema version field does not match [`VaultHeader::SCHEMA_VERSION`].
    #[error("unsupported vault header schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    /// Tier field is neither `1` nor `2`.
    #[error("unsupported tier: {0}")]
    UnsupportedTier(u8),
    /// A tier 1 header incorrectly carries a `key_file_blake3` value.
    #[error("tier 1 vault must not carry a key_file_blake3 field")]
    Tier1WithKeyFileBlake3,
    /// A tier 2 header is missing its mandatory `key_file_blake3` value.
    #[error("tier 2 vault missing key_file_blake3 field")]
    Tier2MissingKeyFileBlake3,
    /// The `key_file_blake3` field is not a 64-character hex string.
    #[error("key_file_blake3 must be 64 hex characters")]
    KeyFileBlake3WrongLength,
    /// The primary `argon2_salt` field failed base64 decoding.
    #[error("argon2_salt failed base64 decode")]
    SaltDecodeFailed,
    /// The primary `argon2_salt` field did not decode to 32 bytes.
    #[error("argon2_salt must decode to 32 bytes")]
    SaltWrongLength,
    /// A recovery slot `argon2_salt` field failed base64 decoding.
    #[error("recovery slot argon2_salt failed base64 decode")]
    RecoverySlotSaltDecodeFailed,
    /// A recovery slot `argon2_salt` field did not decode to 32 bytes.
    #[error("recovery slot argon2_salt must decode to 32 bytes")]
    RecoverySlotSaltWrongLength,
    /// A recovery slot `wrapped_master_key` field failed base64 decoding.
    #[error("recovery slot wrapped_master_key failed base64 decode")]
    RecoverySlotBlobDecodeFailed,
    /// A recovery slot `wrapped_master_key` field did not decode to 72 bytes.
    #[error("recovery slot wrapped_master_key must decode to 72 bytes")]
    RecoverySlotBlobWrongLength,
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn encode_base64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn valid_argon2_params() -> Argon2ParamsJson {
        Argon2ParamsJson {
            memory_cost: 65536,
            time_cost: 3,
            parallelism: 4,
        }
    }

    fn valid_tier1_header() -> VaultHeader {
        VaultHeader {
            vault_id: "11111111-2222-3333-4444-555555555555".into(),
            schema_version: VaultHeader::SCHEMA_VERSION,
            tier: 1,
            argon2_salt: encode_base64(&[0x11u8; 32]),
            argon2_params: valid_argon2_params(),
            key_file_blake3: None,
            recovery_slots: Vec::new(),
        }
    }

    fn valid_tier2_header() -> VaultHeader {
        let mut header = valid_tier1_header();
        header.tier = 2;
        header.key_file_blake3 = Some("a".repeat(64));
        header
    }

    fn valid_recovery_slot() -> RecoverySlot {
        RecoverySlot {
            method: "bip39".into(),
            argon2_salt: encode_base64(&[0x22u8; 32]),
            argon2_params: valid_argon2_params(),
            wrapped_master_key: encode_base64(&[0x33u8; 72]),
        }
    }

    #[test]
    fn test_vault_header_validate_tier1_without_key_file_blake3_succeeds() {
        let header = valid_tier1_header();
        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vault_header_validate_tier2_with_key_file_blake3_succeeds() {
        let header = valid_tier2_header();
        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vault_header_validate_rejects_schema_version_mismatch() {
        let mut header = valid_tier1_header();
        header.schema_version = 999;

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::UnsupportedSchemaVersion(999))
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_tier1_with_key_file_blake3() {
        let mut header = valid_tier1_header();
        header.key_file_blake3 = Some("f".repeat(64));

        let result = header.validate();

        assert!(matches!(result, Err(VaultHeaderError::Tier1WithKeyFileBlake3)));
    }

    #[test]
    fn test_vault_header_validate_rejects_tier2_missing_key_file_blake3() {
        let mut header = valid_tier2_header();
        header.key_file_blake3 = None;

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::Tier2MissingKeyFileBlake3)
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_key_file_blake3_wrong_length() {
        let mut header = valid_tier2_header();
        header.key_file_blake3 = Some("abc".into());

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::KeyFileBlake3WrongLength)
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_unsupported_tier() {
        let mut header = valid_tier1_header();
        header.tier = 5;

        let result = header.validate();

        assert!(matches!(result, Err(VaultHeaderError::UnsupportedTier(5))));
    }

    #[test]
    fn test_vault_header_validate_rejects_salt_decode_failure() {
        let mut header = valid_tier1_header();
        header.argon2_salt = "not base64 !!!".into();

        let result = header.validate();

        assert!(matches!(result, Err(VaultHeaderError::SaltDecodeFailed)));
    }

    #[test]
    fn test_vault_header_validate_rejects_salt_wrong_length() {
        let mut header = valid_tier1_header();
        header.argon2_salt = encode_base64(&[0u8; 16]);

        let result = header.validate();

        assert!(matches!(result, Err(VaultHeaderError::SaltWrongLength)));
    }

    #[test]
    fn test_vault_header_validate_accepts_valid_recovery_slot() {
        let mut header = valid_tier1_header();
        header.recovery_slots.push(valid_recovery_slot());

        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vault_header_validate_rejects_recovery_slot_salt_decode_failure() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.argon2_salt = "###".into();
        header.recovery_slots.push(slot);

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::RecoverySlotSaltDecodeFailed)
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_recovery_slot_salt_wrong_length() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.argon2_salt = encode_base64(&[0u8; 20]);
        header.recovery_slots.push(slot);

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::RecoverySlotSaltWrongLength)
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_recovery_slot_blob_decode_failure() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.wrapped_master_key = "###".into();
        header.recovery_slots.push(slot);

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::RecoverySlotBlobDecodeFailed)
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_recovery_slot_blob_wrong_length() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.wrapped_master_key = encode_base64(&[0u8; 40]);
        header.recovery_slots.push(slot);

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::RecoverySlotBlobWrongLength)
        ));
    }

    #[test]
    fn test_vault_header_validate_skips_unknown_recovery_method() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.method = "future-method".into();
        slot.argon2_salt = "invalid".into();
        slot.wrapped_master_key = "invalid".into();
        header.recovery_slots.push(slot);

        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vault_header_serde_round_trip_preserves_fields() {
        let mut header = valid_tier2_header();
        header.recovery_slots.push(valid_recovery_slot());

        let json = serde_json::to_string(&header).expect("serialize must succeed");
        let decoded: VaultHeader =
            serde_json::from_str(&json).expect("deserialize must succeed");

        assert_eq!(decoded, header);
    }
}
