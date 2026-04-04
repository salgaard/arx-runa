# UC-IND-003: Hardware MFA and Key Loss

**Category**: Individual Privacy

**Status**: Active

---

## Overview

A user wants maximum security for a vault by choosing Tier 2 authentication — password plus a physical USB key file. This use case covers Tier 2 vault creation, daily access, and the irrecoverable loss scenarios that result from losing either authentication factor — a deliberate zero-knowledge design choice.

## Actors

- **Primary Actor**: Individual user requiring hardware-based authentication
- **Secondary Actors**: VoidGate system, USB key file (hardware factor)

## Preconditions

- User has VoidGate installed on their local machine
- User has configured an Rclone backend
- User has a dedicated USB drive for key file generation

## Main Flow

1. User launches VoidGate and selects "Create Vault"
2. VoidGate prompts: "Choose authentication tier — Tier 1 (password only) or Tier 2 (password + USB key)"
3. User selects Tier 2
4. User sets vault password
5. VoidGate prompts: "Insert USB drive for key file generation"
6. User inserts USB drive
7. VoidGate generates a random key file and writes it to the USB drive
8. VoidGate displays: "Store this USB key securely — losing it means permanent data loss for this vault"
9. VoidGate derives encryption keys from the password and key file, then creates the vault
10. User removes USB drive and stores it securely
11. Later, user accesses the vault:
12. VoidGate prompts: "Insert USB key and enter password"
13. User inserts USB drive; VoidGate reads key_file_bytes and derives keys
14. User accesses files; locks vault and removes USB key when done

## Alternate Flows

### Password Loss (Tier 2 Vault)

**Trigger**: User forgets vault password but has USB key

**Steps**:
1. User inserts USB key and attempts vault unlock with incorrect password
2. VoidGate derives wrong master_key; SQLCipher decryption fails
3. VoidGate displays: "Authentication failed"
4. No password reset mechanism exists — Tier 2 vault data is permanently inaccessible

**Outcome**: Data lost. Mitigation: store password in a password manager or physical safe.

### USB Key Loss (Tier 2 Vault)

**Trigger**: User loses the USB drive

**Steps**:
1. User knows password but cannot locate USB key file
2. VoidGate scans removable drives for a file matching the key file fingerprint stored in the vault
3. No matching key file found; VoidGate displays: "Key file not found"
4. Tier 2 vault data is permanently inaccessible — 256 bits of entropy cannot be brute-forced

**Outcome**: Data lost. Mitigation: create backup USB key copies before loss occurs.

### Backup USB Key Restoration

**Trigger**: User loses primary USB key but has a backup copy

**Steps**:
1. User retrieves backup USB drive from secure storage (e.g., fireproof safe, safety deposit box)
2. VoidGate finds the 32-byte file with matching BLAKE3 fingerprint
3. User enters password; VoidGate derives same master_key (identical key_file_bytes)
4. Vault unlocks successfully

**Outcome**: Data recovered. Create backup copies immediately after generating the key file.

### USB Key Compromised

**Trigger**: Attacker obtains a copy of the USB key file but not the password

**Steps**:
1. Attacker attempts brute-force against vault with copied key file
2. The key derivation function makes each attempt computationally expensive
3. Vault remains secure as long as password has sufficient entropy
4. User should rotate the USB key file (VoidGate re-wraps internal keys without re-encrypting cloud data)

## Success Criteria

- Tier 2 vault cannot be unlocked with password alone (USB key mandatory)
- Tier 2 vault cannot be unlocked with USB key alone (password mandatory)
- USB key file is deterministic: identical bytes always produce the same master_key
- No cloud-based factors, no third-party recovery, no admin override
- Authentication is fully offline — no internet required
- A separate Tier 1 vault (if the user has one) remains accessible with password only

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)

## Security Considerations

### Threats Addressed

- **Password-only attack**: Attacker with password but no USB key cannot unlock Tier 2 vault
- **USB-only attack**: Attacker with USB key but no password faces expensive Argon2id brute-force
- **Cloud provider subpoena**: Provider has only encrypted blobs with no key material
- **Coerced account recovery**: No backdoor exists for law enforcement or VoidGate developers
- **Insider threats**: No admin mechanism that could be abused to bypass authentication

### Assumptions

- User physically secures USB key (locked drawer, safe, or safety deposit box)
- User creates at least one backup USB key and stores it in a separate physical location
- User chooses a strong password (≥12 characters, mixed case, symbols, numbers)
- User accepts that Tier 2 key loss means permanent data loss for that vault

### Out of Scope

- Social engineering or coercion to provide both factors
- Malware capturing key file bytes during session
- Tier 1 vault key loss (password-only; recover via password manager)

## Notes

Zero-knowledge architecture is incompatible with account recovery: any recovery mechanism requires a server to hold or re-derive key material, which violates the zero-knowledge guarantee. Users who require data recoverability should apply Tier 2 only to their highest-value vaults and maintain backup USB keys.
