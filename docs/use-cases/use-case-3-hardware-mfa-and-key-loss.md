# Use Case 3: Hardware MFA, Recovery, and Key Loss

## Overview

This use case covers all credential-loss and recovery scenarios across both authentication tiers. The main flow demonstrates Tier 2 vault creation (password + USB key file). Alternate flows cover: password loss for Tier 1 and Tier 2 vaults (with and without a recovery phrase), USB key loss for Tier 2 vaults, backup USB key restoration, recovery phrase setup, password change with an active recovery slot, and USB key compromise. The opt-in BIP-39 recovery phrase is the single recovery mechanism available to users of either tier.

## Actors

- **Primary Actor**: Individual user requiring hardware-based authentication
- **Secondary Actors**: Arx Runa system, USB key file (hardware factor)

## Preconditions

- User has Arx Runa installed on their local machine
- User has configured an Rclone backend
- User has a dedicated USB drive for key file generation

## Main Flow

1. User launches Arx Runa and selects "Create Vault"
2. Arx Runa prompts: "Choose authentication tier — Tier 1 (password only) or Tier 2 (password + USB key)"
3. User selects Tier 2
4. User sets vault password
5. Arx Runa prompts: "Insert USB drive for key file generation"
6. User inserts USB drive
7. Arx Runa generates a random key file and writes it to the USB drive
8. Arx Runa displays: "Store this USB key securely — losing it means permanent data loss for this vault"
9. Arx Runa derives encryption keys from the password and key file, then creates the vault
10. User removes USB drive and stores it securely
11. Later, user accesses the vault:
12. Arx Runa prompts: "Insert USB key and enter password"
13. User inserts USB drive; Arx Runa reads key_file_bytes and derives keys
14. User accesses files; locks vault and removes USB key when done

## Alternate Flows

### Password Loss — Without Recovery Phrase

**Trigger**: User forgets vault password and has no recovery phrase configured

**Steps**:
1. *(Tier 2 only)* User inserts USB key
2. User attempts vault unlock with incorrect password
3. Arx Runa derives wrong `master_key`; SQLCipher decryption fails
4. Arx Runa displays: "Authentication failed"
5. No recovery slot is configured — vault data is permanently inaccessible

**Outcome**: Data lost. Mitigations: store password in a password manager or physical safe; configure a recovery phrase at vault creation.

### Password Loss — With Recovery Phrase

**Trigger**: User forgets vault password but has a recovery phrase configured

**Steps**:
1. User selects "Recover with phrase" on the login screen
2. Arx Runa fetches vault header; confirms a `bip39` recovery slot is present
3. User enters 24-word recovery phrase
4. Arx Runa validates BIP-39 checksum — words not in the BIP-39 wordlist or an invalid checksum are caught immediately; if all words are valid but the phrase is incorrect, recovery fails with an authentication error
5. Arx Runa derives `recovery_key` via Argon2id and decrypts `wrapped_master_key`
6. HKDF derives vault-level session keys; session begins
7. Arx Runa prompts: "Set a new password to complete recovery"
8. User sets new password; vault is re-keyed; recovery slot re-wrapped under new `master_key`

**Outcome**: Vault recovered. *(Tier 2)* User should verify backup USB key is still functional after recovery.

### USB Key Loss (Tier 2 Vault) — Without Recovery Phrase

**Trigger**: User loses the USB drive and has no recovery phrase configured

**Steps**:
1. User knows password but cannot locate USB key file
2. Arx Runa scans removable drives for a file matching the key file fingerprint stored in the vault
3. No matching key file found; Arx Runa displays: "Key file not found"
4. No recovery slot is configured — Tier 2 vault data is permanently inaccessible

**Outcome**: Data lost. Mitigations: create backup USB key copies immediately after vault creation; configure a recovery phrase.

### USB Key Loss (Tier 2 Vault) — With Recovery Phrase

**Trigger**: User loses the USB drive but has a recovery phrase configured

**Steps**:
1. User selects "Recover with phrase" on the login screen
2. User enters 24-word recovery phrase; Arx Runa decrypts `wrapped_master_key` as above
3. Session begins; Arx Runa prompts: "Set a new password and insert a new USB key to complete recovery"
4. User sets new password and inserts a new USB drive; Arx Runa generates a new key file
5. Vault is re-keyed to the new password + new USB key; recovery slot re-wrapped

**Outcome**: Vault recovered. The old USB key file is irrevocably lost; the new USB key replaces it. The user should create backup copies of the new USB key immediately.

### Backup USB Key Restoration

**Trigger**: User loses primary USB key but has a backup copy

**Steps**:
1. User retrieves backup USB drive from secure storage (e.g., fireproof safe, safety deposit box)
2. Arx Runa finds the 32-byte file with matching BLAKE3 fingerprint
3. User enters password; Arx Runa derives same master_key (identical key_file_bytes)
4. Vault unlocks successfully

**Outcome**: Data recovered. Create backup copies immediately after generating the key file.

### Recovery Phrase Setup

**Trigger**: User wants to configure a recovery phrase for their vault

**Steps**:
1. User opens Security settings and selects "Set up recovery phrase"
2. Arx Runa prompts: "Enter your current password" (and "Insert USB key" for Tier 2)
3. Arx Runa re-derives `master_key` from current credentials
4. Arx Runa generates 256 bits of entropy; displays 24 words to the user
5. User writes down all 24 words; Arx Runa prompts: "I have written down my recovery phrase"
6. After acknowledgement, phrase is zeroed from memory; recovery slot added to vault header
7. Arx Runa displays: "Recovery phrase configured. Keep it in a secure, separate location from your USB key."

**Outcome**: Recovery slot active. The phrase is the only copy — Arx Runa does not store it.

### Password Change with Recovery Phrase Active

**Trigger**: User changes their vault password while a recovery slot is configured

**Steps**:
1. User opens Security settings and selects "Change password"
2. Arx Runa authenticates with current credentials
3. Arx Runa prompts: "Enter your recovery phrase to keep it valid after the password change"
4. User enters 24-word phrase; Arx Runa verifies it decrypts the current `master_key` correctly
5. User enters new password; Arx Runa derives new `master_key` and re-wraps all keys
6. Recovery slot is updated: `master_key` re-encrypted under the same `recovery_key` (phrase unchanged)
7. Vault header uploaded; session continues with new keys

**Outcome**: Password changed; existing recovery phrase remains valid. If the user cannot provide the phrase at step 4, they can skip it — the recovery slot is removed with a warning.

### USB Key Compromised

**Trigger**: Attacker obtains a copy of the USB key file but not the password

**Steps**:
1. Attacker attempts brute-force against vault with copied key file
2. The key derivation function makes each attempt computationally expensive
3. Vault remains secure as long as password has sufficient entropy
4. User should rotate the USB key file (Arx Runa re-wraps internal keys without re-encrypting cloud data)

## Success Criteria

- Tier 1 vault cannot be unlocked without the correct password — unless recovery phrase is used
- Tier 2 vault cannot be unlocked with password alone (USB key mandatory) — unless recovery phrase is used
- Tier 2 vault cannot be unlocked with USB key alone (password mandatory) — unless recovery phrase is used
- USB key file is deterministic: identical bytes always produce the same master_key
- No cloud-based factors, no third-party recovery, no admin override
- Authentication is fully offline — no internet required (vault header is cached locally after first download)
- A separate Tier 1 vault (if the user has one) remains accessible with password only
- Recovery phrase alone unlocks vault regardless of tier — when configured
- After recovery, user must set new primary credentials before vault is fully operational
- Recovery slot survives password change and key rotation when phrase is provided during the ceremony

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — Recovery Slot section covers BIP-39 derivation and authentication flow
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` functions
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — `RecoverySlot` struct and vault header format
- [Password and Key Recovery Research](../research/password-and-key-recovery.md) — Full feasibility analysis and decision rationale

## Security Considerations

### Threats Addressed

- **Password-only attack**: Attacker with password but no USB key cannot unlock Tier 2 vault (without recovery phrase)
- **USB-only attack**: Attacker with USB key but no password faces expensive Argon2id brute-force (without recovery phrase)
- **Cloud provider subpoena**: Provider has only encrypted blobs with no key material
- **Coerced account recovery**: No backdoor exists for law enforcement or Arx Runa developers
- **Insider threats**: No admin mechanism that could be abused to bypass authentication
- **Recovery phrase attack**: Attacker who obtains the 24-word phrase can unlock the vault regardless of tier. Mitigation: phrase has 256-bit entropy — brute-force is computationally infeasible. Physical security of the written phrase is the user's responsibility.

### Assumptions

- User physically secures USB key (locked drawer, safe, or safety deposit box)
- User creates at least one backup USB key and stores it in a separate physical location
- User chooses a strong password (≥12 characters, mixed case, symbols, numbers)
- User accepts that Tier 2 key loss means permanent data loss for that vault — unless a recovery phrase is configured
- If a recovery phrase is configured, user stores it in a secure location physically separate from the USB key (compromising both voids the two-factor protection)

### Out of Scope

- Social engineering or coercion to provide both factors
- Malware capturing key file bytes during session
- Tier 1 vault key loss (password-only; recover via password manager)

## Notes

Zero-knowledge architecture is compatible with client-side recovery mechanisms where recovery material is generated and stored entirely by the user — the server never sees keys or plaintext in any recovery flow. Server-side account recovery remains incompatible: any mechanism requiring a server to hold or re-derive key material violates the zero-knowledge guarantee.

Users who require data recoverability should configure the opt-in BIP-39 recovery phrase and store it in a secure, offline location separate from the USB key. Users who apply Tier 2 to their highest-value vaults and do not configure recovery must maintain backup USB key copies as their sole fallback.
