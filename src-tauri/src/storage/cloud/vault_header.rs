//! Vault header types and validation. I/O operations are in [`vault_header_io`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCHEMA_VERSION: u32 = 1;
const ARGON2_MIN_MEMORY_COST_KIB: u32 = 19_456;
const ARGON2_MIN_TIME_COST: u32 = 2;
const ARGON2_MIN_PARALLELISM: u32 = 1;

/// Argon2id parameters as serialised inside the vault header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Argon2ParametersJson {
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
    pub argon2_params: Argon2ParametersJson,
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
    pub argon2_params: Argon2ParametersJson,
    /// BLAKE3 hex digest of the USB key file; local-only hint, never uploaded to cloud.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_file_blake3: Option<String>,
    /// Recovery slots; empty until `setup_recovery` runs.
    #[serde(default)]
    pub recovery_slots: Vec<RecoverySlot>,
    /// Optional human-readable vault name set at creation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Trusted local vault-header anchor used for existing-device trust checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedVaultHeaderAnchor {
    /// Expected vault identifier.
    pub vault_id: String,
    /// Expected primary Argon2id salt.
    pub argon2_salt: String,
    /// Expected primary Argon2id parameters.
    pub argon2_params: Argon2ParametersJson,
}

/// Trust-policy mode applied after structural vault-header validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultHeaderTrustPolicy<'a> {
    /// Bootstrap-mode policy for new devices.
    Bootstrap,
    /// Existing-device policy requiring exact match with trusted local anchor.
    ExistingDevice {
        /// Locally trusted anchor for downgrade resistance.
        trusted_anchor: &'a TrustedVaultHeaderAnchor,
    },
}

impl VaultHeader {
    /// Current schema version emitted by Phase 2.4 ceremonies.
    pub const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    /// Validates both structural invariants and bootstrap trust-policy floors.
    pub fn validate(&self) -> Result<(), VaultHeaderError> {
        self.validate_structure()?;
        self.validate_trust_policy(VaultHeaderTrustPolicy::Bootstrap)?;
        Ok(())
    }

    /// Validates header field shape/encoding invariants independent of trust mode.
    pub fn validate_structure(&self) -> Result<(), VaultHeaderError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(VaultHeaderError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        match self.tier {
            1 | 2 => {
                // key_file_blake3 is a local-only hint field stripped before cloud upload;
                // validate format only when present.
                if let Some(hex_digest) = &self.key_file_blake3 {
                    let digest = hex::decode(hex_digest)
                        .map_err(|_| VaultHeaderError::KeyFileBlake3DecodeFailed)?;
                    if digest.len() != 32 {
                        return Err(VaultHeaderError::KeyFileBlake3WrongLength);
                    }
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

    /// Validates trust-policy requirements and enforces structural validation.
    pub fn validate_trust_policy(
        &self,
        policy: VaultHeaderTrustPolicy<'_>,
    ) -> Result<(), VaultHeaderError> {
        self.validate_structure()?;
        validate_argon2_parameters(&self.argon2_params)
            .map_err(|_| VaultHeaderError::Argon2ParamsBelowMinimum)?;
        for slot in &self.recovery_slots {
            validate_argon2_parameters(&slot.argon2_params)
                .map_err(|_| VaultHeaderError::RecoverySlotArgon2ParamsBelowMinimum)?;
        }
        match policy {
            VaultHeaderTrustPolicy::Bootstrap => Ok(()),
            VaultHeaderTrustPolicy::ExistingDevice { trusted_anchor } => {
                if self.vault_id != trusted_anchor.vault_id {
                    return Err(VaultHeaderError::TrustedVaultIdMismatch);
                }
                if self.argon2_salt != trusted_anchor.argon2_salt {
                    return Err(VaultHeaderError::TrustedArgon2SaltMismatch);
                }
                if self.argon2_params != trusted_anchor.argon2_params {
                    return Err(VaultHeaderError::TrustedArgon2ParamsMismatch);
                }
                Ok(())
            }
        }
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
    /// The `key_file_blake3` field is not valid hex.
    #[error("key_file_blake3 failed hex decode")]
    KeyFileBlake3DecodeFailed,
    /// The `key_file_blake3` field did not decode to 32 bytes.
    #[error("key_file_blake3 must decode to 32 bytes")]
    KeyFileBlake3WrongLength,
    /// The primary `argon2_salt` field failed base64 decoding.
    #[error("argon2_salt failed base64 decode")]
    SaltDecodeFailed,
    /// The primary `argon2_salt` field did not decode to 32 bytes.
    #[error("argon2_salt must decode to 32 bytes")]
    SaltWrongLength,
    /// The primary `argon2_params` fields are below minimum floor values.
    #[error("argon2_params are below minimum floor values")]
    Argon2ParamsBelowMinimum,
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
    /// A recovery slot `argon2_params` fields are below minimum floor values.
    #[error("recovery slot argon2_params are below minimum floor values")]
    RecoverySlotArgon2ParamsBelowMinimum,
    /// Existing-device trust anchor mismatch on vault identifier.
    #[error("vault_id does not match trusted local anchor")]
    TrustedVaultIdMismatch,
    /// Existing-device trust anchor mismatch on primary Argon2 salt.
    #[error("argon2_salt does not match trusted local anchor")]
    TrustedArgon2SaltMismatch,
    /// Existing-device trust anchor mismatch on primary Argon2 parameters.
    #[error("argon2_params do not match trusted local anchor")]
    TrustedArgon2ParamsMismatch,
}

/// Decodes a standard base64 string into raw bytes.
fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| ())
}

/// Checks whether Argon2 parameters meet the minimum accepted floor.
fn validate_argon2_parameters(parameters: &Argon2ParametersJson) -> Result<(), ()> {
    if parameters.memory_cost < ARGON2_MIN_MEMORY_COST_KIB
        || parameters.time_cost < ARGON2_MIN_TIME_COST
        || parameters.parallelism < ARGON2_MIN_PARALLELISM
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn encode_base64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn valid_argon2_parameters() -> Argon2ParametersJson {
        Argon2ParametersJson {
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
            argon2_params: valid_argon2_parameters(),
            key_file_blake3: None,
            recovery_slots: Vec::new(),
            name: None,
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
            argon2_params: valid_argon2_parameters(),
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
    fn test_vault_header_validate_rejects_key_file_blake3_wrong_length() {
        let mut header = valid_tier2_header();
        header.key_file_blake3 = Some("ab".repeat(31));

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::KeyFileBlake3WrongLength)
        ));
    }

    #[test]
    fn test_vault_header_validate_rejects_key_file_blake3_non_hex() {
        let mut header = valid_tier2_header();
        header.key_file_blake3 = Some("g".repeat(64));

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::KeyFileBlake3DecodeFailed)
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
    fn test_vault_header_validate_rejects_primary_argon2_params_below_minimum_floor() {
        let mut header = valid_tier1_header();
        header.argon2_params.memory_cost = ARGON2_MIN_MEMORY_COST_KIB - 1;

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::Argon2ParamsBelowMinimum)
        ));
    }

    #[test]
    fn test_vault_header_validate_structure_accepts_primary_argon2_below_floor() {
        let mut header = valid_tier1_header();
        header.argon2_params.memory_cost = ARGON2_MIN_MEMORY_COST_KIB - 1;

        assert!(header.validate_structure().is_ok());
    }

    #[test]
    fn test_vault_header_validate_accepts_valid_recovery_slot() {
        let mut header = valid_tier1_header();
        header.recovery_slots.push(valid_recovery_slot());

        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vault_header_validate_rejects_recovery_slot_argon2_params_below_minimum_floor() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.argon2_params.time_cost = ARGON2_MIN_TIME_COST - 1;
        header.recovery_slots.push(slot);

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::RecoverySlotArgon2ParamsBelowMinimum)
        ));
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
    fn test_vault_header_validate_rejects_unknown_recovery_method_with_invalid_structure() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.method = "future-method".into();
        slot.argon2_salt = "invalid".into();
        slot.wrapped_master_key = "invalid".into();
        header.recovery_slots.push(slot);

        let result = header.validate();

        assert!(matches!(
            result,
            Err(VaultHeaderError::RecoverySlotSaltDecodeFailed)
        ));
    }

    #[test]
    fn test_vault_header_validate_accepts_unknown_recovery_method_with_valid_structure() {
        let mut header = valid_tier1_header();
        let mut slot = valid_recovery_slot();
        slot.method = "future-method".into();
        header.recovery_slots.push(slot);

        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vault_header_validate_trust_policy_rejects_vault_id_anchor_mismatch() {
        let header = valid_tier1_header();
        let trusted_anchor = TrustedVaultHeaderAnchor {
            vault_id: "aaaaaaaa-2222-3333-4444-555555555555".into(),
            argon2_salt: header.argon2_salt.clone(),
            argon2_params: header.argon2_params.clone(),
        };

        let result = header.validate_trust_policy(VaultHeaderTrustPolicy::ExistingDevice {
            trusted_anchor: &trusted_anchor,
        });

        assert!(matches!(
            result,
            Err(VaultHeaderError::TrustedVaultIdMismatch)
        ));
    }

    #[test]
    fn test_vault_header_validate_trust_policy_rejects_invalid_structure_before_policy_checks() {
        let mut header = valid_tier1_header();
        header.argon2_salt = "not base64 !!!".into();

        let result = header.validate_trust_policy(VaultHeaderTrustPolicy::Bootstrap);

        assert!(matches!(result, Err(VaultHeaderError::SaltDecodeFailed)));
    }

    #[test]
    fn test_vault_header_validate_trust_policy_rejects_argon2_salt_anchor_mismatch() {
        let header = valid_tier1_header();
        let trusted_anchor = TrustedVaultHeaderAnchor {
            vault_id: header.vault_id.clone(),
            argon2_salt: encode_base64(&[0x44u8; 32]),
            argon2_params: header.argon2_params.clone(),
        };

        let result = header.validate_trust_policy(VaultHeaderTrustPolicy::ExistingDevice {
            trusted_anchor: &trusted_anchor,
        });

        assert!(matches!(
            result,
            Err(VaultHeaderError::TrustedArgon2SaltMismatch)
        ));
    }

    #[test]
    fn test_vault_header_validate_trust_policy_rejects_argon2_params_anchor_mismatch() {
        let header = valid_tier1_header();
        let trusted_anchor = TrustedVaultHeaderAnchor {
            vault_id: header.vault_id.clone(),
            argon2_salt: header.argon2_salt.clone(),
            argon2_params: Argon2ParametersJson {
                memory_cost: 12345,
                time_cost: header.argon2_params.time_cost,
                parallelism: header.argon2_params.parallelism,
            },
        };

        let result = header.validate_trust_policy(VaultHeaderTrustPolicy::ExistingDevice {
            trusted_anchor: &trusted_anchor,
        });

        assert!(matches!(
            result,
            Err(VaultHeaderError::TrustedArgon2ParamsMismatch)
        ));
    }

    #[test]
    fn test_vault_header_serde_round_trip_preserves_fields() {
        let mut header = valid_tier2_header();
        header.recovery_slots.push(valid_recovery_slot());

        let json = serde_json::to_string(&header).expect("serialize must succeed");
        let decoded: VaultHeader = serde_json::from_str(&json).expect("deserialize must succeed");

        assert_eq!(decoded, header);
    }

    #[test]
    fn test_vault_header_serde_missing_recovery_slots_defaults_to_empty() {
        let header_json = format!(
            r#"{{
  "vault_id": "11111111-2222-3333-4444-555555555555",
  "schema_version": {},
  "tier": 1,
  "argon2_salt": "{}",
  "argon2_params": {{
    "memory_cost": 65536,
    "time_cost": 3,
    "parallelism": 4
  }},
  "key_file_blake3": null
}}"#,
            VaultHeader::SCHEMA_VERSION,
            encode_base64(&[0x11u8; 32]),
        );

        let decoded: VaultHeader =
            serde_json::from_str(&header_json).expect("deserialize must succeed");

        assert!(decoded.recovery_slots.is_empty());
    }
}
