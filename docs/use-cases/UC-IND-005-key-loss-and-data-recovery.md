# UC-IND-005: Key Loss and Irrecoverable Data Scenarios

**Category**: Individual Privacy

**Status**: Active

---

## Overview

VoidGate implements true zero-knowledge encryption where vault data is mathematically unrecoverable if either authentication factor (password or USB key file) is lost. This use case documents the explicit consequences of key loss and user responsibilities for backup strategies.

## Actors

- **Primary Actor**: Individual user who has lost password, USB key file, or both
- **Secondary Actors**: VoidGate system, cloud storage provider (has encrypted vault data)

## Preconditions

- User has created a vault with password + USB key file authentication
- Vault data exists in cloud storage (encrypted blobs, manifest, vault header)
- User has lost access to one or both authentication factors

## Main Flow: Password Loss (USB Key Available)

1. User realizes they have forgotten their vault password
2. User inserts USB key drive containing valid key file
3. User launches VoidGate and attempts vault unlock
4. VoidGate prompts for password
5. User enters guessed/incorrect password
6. VoidGate performs Argon2id(incorrect_password || key_file_bytes, salt)
7. Argon2id produces a different master_key than the original
8. VoidGate attempts to open SQLCipher database with derived sqlcipher_key
9. SQLCipher decryption fails (wrong key)
10. VoidGate displays: "Authentication failed"
11. User cannot access vault — **data is permanently inaccessible**
12. No password reset, recovery questions, or admin override exists

**Outcome**: Vault remains encrypted. All files, metadata, and sharing relationships are permanently lost.

## Alternate Flows

### USB Key File Loss (Password Available)

**Trigger**: User loses USB drive containing key file, or drive is physically destroyed

**Steps**:
1. User knows their password but cannot locate USB key file
2. User launches VoidGate and attempts vault unlock
3. VoidGate scans removable drives for 32-byte files matching BLAKE3 hash in vault header
4. No matching key file found
5. VoidGate displays: "Key file not found — insert USB drive with key file"
6. User cannot proceed without exact 32-byte key file
7. **Data is permanently inaccessible**

**Outcome**: Password alone is insufficient by design. Vault cannot be unlocked. All data is permanently lost.

**Why recovery is impossible**: The BLAKE3 hash stored in the vault header is preimage-resistant — the original 32 bytes cannot be derived from the hash. The key file contains 256 bits of random entropy. Brute-forcing 2^256 possibilities is computationally infeasible.

### Both Factors Lost

**Trigger**: User forgets password AND loses USB key file

**Steps**:
1. User has neither authentication factor
2. User cannot authenticate
3. Vault data remains encrypted in cloud storage
4. **Data is permanently inaccessible**

**Outcome**: Total loss. No recovery mechanism exists.

### Backup USB Key Restoration (Success Path)

**Trigger**: User loses primary USB key but has created backup copy

**Steps**:
1. User loses primary USB drive
2. User retrieves backup USB drive from secure storage (e.g., safety deposit box, fireproof safe)
3. User inserts backup USB drive
4. VoidGate scans and finds 32-byte file with matching BLAKE3 hash
5. User enters password
6. VoidGate performs Argon2id(password || backup_key_file_bytes, salt)
7. Backup key file has identical bytes to original → same master_key derived
8. Vault unlocks successfully
9. User continues normal operation
10. User may optionally create new backup USB key

**Outcome**: Data recovered successfully. This demonstrates the importance of backup key files.

### Password Manager Recovery (Success Path)

**Trigger**: User forgets password but has stored it in password manager

**Steps**:
1. User cannot recall vault password
2. User opens password manager (e.g., 1Password, Bitwarden, KeePassXC)
3. User retrieves stored vault password
4. User inserts USB key drive
5. User enters recovered password
6. Vault unlocks successfully

**Outcome**: Data recovered successfully. Demonstrates importance of secure password storage.

## Success Criteria

- **Explicit User Awareness**: User is warned during vault creation that key loss = permanent data loss
- **No Silent Degradation**: VoidGate never falls back to password-only authentication
- **No Recovery Backdoor**: System provides no password reset, account recovery, or admin override mechanism
- **Cryptographic Guarantee**: Data is mathematically unrecoverable without both factors (not just "very difficult" — impossible)
- **Backup Guidance**: VoidGate documentation clearly explains backup strategies for both factors

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — Argon2id key derivation requiring both password and key file, BLAKE3 key file fingerprinting, no password-only fallback, mandatory dual-factor authentication
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — CSPRNG entropy for key file generation, preimage-resistant hash functions
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Vault header structure (salt, BLAKE3 hash), encrypted manifest backup

## Security Considerations

### Threats Addressed

- **Coerced Account Recovery**: No backdoor exists for law enforcement, cloud provider, or VoidGate developers to access user data
- **Cloud Provider Subpoena**: Cloud provider has only encrypted blobs with no mechanism to decrypt
- **Password Compromise Alone**: Attacker who obtains password through phishing/keylogger cannot access vault without USB key
- **USB Key Theft Alone**: Attacker who steals USB key faces expensive Argon2id brute-force attack without password (feasibility depends on password entropy)
- **Insider Threats**: VoidGate developers cannot add recovery mechanisms without user knowledge (open-source verification)

### Assumptions

- **User Accepts Responsibility**: User understands that cryptographic security comes at the cost of recoverability
- **User Creates Backups**: User proactively creates backup USB key and stores in separate physical location
- **User Secures Password**: User stores password in password manager, physical safe, or other secure method
- **User Has Minimum Password Entropy**: Password has sufficient strength to resist offline brute-force attacks (≥12 characters, mixed case, symbols, numbers recommended)

### Out of Scope

- **Escrowed Recovery Keys**: No third-party holds recovery keys (would violate zero-knowledge design)
- **Social Recovery**: No "trusted contacts" or threshold cryptography for recovery (future consideration for Phase 7+)
- **Biometric Fallback**: No face/fingerprint unlock option (would require on-device storage of recovery keys)
- **Time-Lock Puzzles**: No cryptographic time-lock mechanisms for delayed recovery
- **Hardware Security Modules**: No HSM-backed recovery (enterprise feature, Phase 7+)

## Notes

### Design Rationale: Why No Recovery?

VoidGate's zero-knowledge architecture is incompatible with account recovery mechanisms:

1. **Password Reset via Email**: Requires server to hold decryption keys (violates zero-knowledge)
2. **Recovery Questions**: Same problem — server needs keys to re-encrypt vault
3. **Admin Override**: No admin exists — cloud provider sees only opaque blobs
4. **Master Recovery Key**: Would create a single point of compromise

**Trade-off Accepted**: VoidGate prioritizes cryptographic security over convenience. Users who require account recovery should use traditional cloud storage (Google Drive, Dropbox) with provider-managed encryption.

### User Warnings During Vault Creation

VoidGate should display the following warnings during vault creation:

**At Password Entry**:
> ⚠️ **Warning**: If you lose your password, your data cannot be recovered. No password reset mechanism exists. Store your password in a secure password manager or physical safe.

**At USB Key File Generation**:
> ⚠️ **Warning**: This USB key file is mandatory for vault access. If lost or destroyed, your data is permanently inaccessible. Create backup copies and store in separate secure locations (safety deposit box, fireproof safe, trusted family member).

**Before Final Vault Creation**:
> ⚠️ **Final Warning**: You are about to create a zero-knowledge encrypted vault. VoidGate cannot recover your data if you lose your password or USB key file. This is an intentional design choice for maximum security. Do you accept responsibility for backup creation?
> 
> [ ] I understand that key loss = permanent data loss
> [ ] I will create backup USB key copies
> [ ] I will store my password securely
> 
> [Create Vault] [Cancel]

### Recommended Backup Strategies

**For USB Key File**:
1. **Immediate Backup**: Create 2-3 copies of USB key file on separate drives
2. **Geographic Distribution**: Store one backup at home safe, one at workplace locker, one at family member's home
3. **Safety Deposit Box**: Bank safety deposit box for long-term backup
4. **Fireproof Safe**: Home fireproof safe for quick access backup
5. **Paper Backup**: Print 32 bytes as hexadecimal or QR code, laminate, store in safe

**For Password**:
1. **Password Manager**: Store in encrypted password manager (1Password, Bitwarden, KeePassXC)
2. **Physical Safe**: Write password on paper, store in locked safe at home
3. **Trusted Contact**: Sealed envelope with trusted family member (emergency recovery)

**Testing Backups**:
- User should periodically test backup USB keys to verify they still work
- VoidGate could offer "Test Backup Key" feature (verifies BLAKE3 hash without unlocking vault)

### Comparison to Other Systems

| System | Recovery Mechanism | Zero-Knowledge? |
|--------|-------------------|-----------------|
| Google Drive | Email-based password reset | ❌ No (Google can decrypt) |
| Dropbox | Account recovery via email/SMS | ❌ No (Dropbox can decrypt) |
| iCloud | Device-based recovery, Apple ID | ❌ No (Apple can decrypt with lawful request) |
| Tresorit | Master password required, no reset | ✅ Yes |
| Cryptomator | Password-only, no recovery | ✅ Yes (password-only, not hardware MFA) |
| **VoidGate** | **No recovery mechanism** | ✅ **Yes (dual-factor mandatory)** |

### Future Enhancements (Out of Scope for Phase 1-6)

Potential recovery mechanisms that could be added while preserving zero-knowledge:

- **Shamir Secret Sharing**: Split recovery key into N shares, require M to recover (e.g., 3-of-5 trusted contacts)
- **Time-Lock Encryption**: Cryptographic puzzle that becomes solvable after X years (academic research, not production-ready)
- **Ledger/Trezor Integration**: Hardware wallet holds backup key with PIN protection
- **Notarized Key Escrow**: User voluntarily deposits encrypted key with lawyer/notary (legal recovery mechanism, not technical)

These would be Phase 7+ enterprise features with explicit user opt-in.

---

**References**:
- NIST SP 800-63B: Digital Identity Guidelines (Authentication & Lifecycle Management)
- ProtonMail Key Recovery Debate: [ProtonMail Blog](https://protonmail.com/blog/encrypted-email-recovery/)
- Tresorit Zero-Knowledge Architecture: [Tresorit Security Whitepaper](https://tresorit.com/security)
- Signal's Stance on Backdoors: [Moxie Marlinspike — Encryption Backdoors](https://signal.org/blog/)
