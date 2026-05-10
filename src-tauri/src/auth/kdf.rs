//! Argon2id KDF wrapper for Arx Runa authentication.
//!
//! Converts `(password, optional key file, salt, Argon2 params)` into a
//! 32-byte `master_key` written directly into a caller-provided buffer.

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;

/// Argon2id cost parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2Params {
    /// Memory cost in KiB.
    pub memory_cost_kib: u32,
    /// Time cost (iterations).
    pub time_cost: u32,
    /// Parallelism degree.
    pub parallelism: u32,
}

impl Argon2Params {
    /// Arx Runa default Argon2id parameters.
    pub const DEFAULT: Self = Self {
        memory_cost_kib: 65536,
        time_cost: 3,
        parallelism: 4,
    };
}

const MASTER_KEY_LENGTH_BYTES: usize = 32;
const KEY_FILE_LENGTH_BYTES: usize = 32;

/// Derives a 32-byte `master_key` into `output`.
///
/// - Tier 1: `argon2_input = password_utf8_bytes`.
/// - Tier 2: `argon2_input = password_utf8_bytes || key_file_bytes`.
///
/// Argon2 parameter validation is delegated to `argon2::Params::new`; this
/// wrapper does not clamp, default, or bootstrap-validate.
pub(crate) fn derive_master_key_into(
    password_utf8_bytes: &[u8],
    key_file_bytes: Option<&[u8; KEY_FILE_LENGTH_BYTES]>,
    salt: &[u8; 32],
    params: &Argon2Params,
    output: &mut [u8; MASTER_KEY_LENGTH_BYTES],
) -> Result<(), AuthenticationError> {
    let combined_input_length =
        password_utf8_bytes.len() + key_file_bytes.map_or(0, |_| KEY_FILE_LENGTH_BYTES);
    let mut combined_input: Zeroizing<Vec<u8>> =
        Zeroizing::new(Vec::with_capacity(combined_input_length));
    combined_input.extend_from_slice(password_utf8_bytes);
    if let Some(bytes) = key_file_bytes {
        combined_input.extend_from_slice(bytes);
    }

    let argon2_params = Params::new(
        params.memory_cost_kib,
        params.time_cost,
        params.parallelism,
        Some(MASTER_KEY_LENGTH_BYTES),
    )
    .map_err(|_| AuthenticationError::InvalidCredentials)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);
    argon2
        .hash_password_into(&combined_input, salt, output)
        .map_err(|_| AuthenticationError::InvalidCredentials)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Argon2Params, derive_master_key_into};

    const TEST_PARAMS: Argon2Params = Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    };

    const TEST_SALT: [u8; 32] = [0x11u8; 32];

    #[test]
    fn test_derive_master_key_tier1_produces_expected_length() {
        let mut output = [0u8; 32];
        derive_master_key_into(
            b"correct horse battery staple",
            None,
            &TEST_SALT,
            &TEST_PARAMS,
            &mut output,
        )
        .expect("tier 1 derivation must succeed");
        assert_ne!(output, [0u8; 32]);
    }

    #[test]
    fn test_derive_master_key_tier2_produces_expected_length() {
        let key_file = [0x22u8; 32];
        let mut output = [0u8; 32];
        derive_master_key_into(
            b"correct horse battery staple",
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut output,
        )
        .expect("tier 2 derivation must succeed");
        assert_ne!(output, [0u8; 32]);
    }

    #[test]
    fn test_derive_master_key_is_deterministic_for_same_inputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        derive_master_key_into(b"password", None, &TEST_SALT, &TEST_PARAMS, &mut first).unwrap();
        derive_master_key_into(b"password", None, &TEST_SALT, &TEST_PARAMS, &mut second).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_derive_master_key_different_passwords_produce_different_outputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        derive_master_key_into(b"password-a", None, &TEST_SALT, &TEST_PARAMS, &mut first).unwrap();
        derive_master_key_into(b"password-b", None, &TEST_SALT, &TEST_PARAMS, &mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn test_derive_master_key_different_key_files_produce_different_outputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        derive_master_key_into(
            b"password",
            Some(&[0x01u8; 32]),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut first,
        )
        .unwrap();
        derive_master_key_into(
            b"password",
            Some(&[0x02u8; 32]),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut second,
        )
        .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn test_derive_master_key_tier1_and_tier2_differ_for_same_password() {
        let key_file = [0x33u8; 32];
        let mut tier_one = [0u8; 32];
        let mut tier_two = [0u8; 32];
        derive_master_key_into(b"password", None, &TEST_SALT, &TEST_PARAMS, &mut tier_one).unwrap();
        derive_master_key_into(
            b"password",
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut tier_two,
        )
        .unwrap();
        assert_ne!(tier_one, tier_two);
    }

    #[test]
    fn test_derive_master_key_different_salts_produce_different_outputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        let salt_a = [0x55u8; 32];
        let salt_b = [0x66u8; 32];
        derive_master_key_into(b"password", None, &salt_a, &TEST_PARAMS, &mut first).unwrap();
        derive_master_key_into(b"password", None, &salt_b, &TEST_PARAMS, &mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn test_derive_master_key_default_params_succeeds() {
        let mut output = [0u8; 32];
        derive_master_key_into(
            b"correct horse battery staple",
            None,
            &TEST_SALT,
            &Argon2Params::DEFAULT,
            &mut output,
        )
        .expect("derivation with default params must succeed");
        assert_ne!(output, [0u8; 32]);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]

        #[test]
        fn test_derive_master_key_into_with_random_utf8_inputs_is_deterministic(
            password in ".{0,64}",
            use_key_file in any::<bool>(),
            key_file_seed in any::<u8>(),
            salt_seed in any::<u8>(),
        ) {
            let key_file = [key_file_seed; 32];
            let key_file_bytes = use_key_file.then_some(&key_file);
            let salt = [salt_seed; 32];
            let mut first = [0u8; 32];
            let mut second = [0u8; 32];

            derive_master_key_into(
                password.as_bytes(),
                key_file_bytes,
                &salt,
                &TEST_PARAMS,
                &mut first,
            )
            .expect("derivation must succeed");
            derive_master_key_into(
                password.as_bytes(),
                key_file_bytes,
                &salt,
                &TEST_PARAMS,
                &mut second,
            )
            .expect("derivation must succeed");

            prop_assert_eq!(first, second);
        }

        #[test]
        fn test_derive_master_key_into_with_distinct_utf8_passwords_produces_distinct_outputs(
            password_a in ".{0,64}",
            password_b in ".{0,64}",
            salt_seed in any::<u8>(),
        ) {
            prop_assume!(password_a != password_b);

            let salt = [salt_seed; 32];
            let mut first = [0u8; 32];
            let mut second = [0u8; 32];

            derive_master_key_into(password_a.as_bytes(), None, &salt, &TEST_PARAMS, &mut first)
                .expect("derivation must succeed");
            derive_master_key_into(password_b.as_bytes(), None, &salt, &TEST_PARAMS, &mut second)
                .expect("derivation must succeed");

            prop_assert_ne!(first, second);
        }
    }
}
