# UC-IND-004: Hardware MFA for Personal Data Protection

**Category**: Individual Privacy

**Status**: Active

---

## Overview

A privacy-conscious user wants multi-factor authentication for cloud storage but refuses to use SMS, TOTP apps, or biometrics due to concerns about phone compromise, cloud-based authenticators, or biometric database breaches. They prefer a physical, deterministic hardware factor.

## Actors

- **Primary Actor**: Individual user requiring hardware-based authentication
- **Secondary Actors**: VoidGate system, USB key file (hardware factor), password (knowledge factor)

## Preconditions

- User has VoidGate installed
- User has generated a USB key file (32 bytes random entropy) on a dedicated USB drive
- User has created a vault with password + USB key file
- User understands that losing USB key = permanent vault loss (no recovery)

## Main Flow

1. User creates new vault in VoidGate
2. VoidGate prompts: "Insert USB drive for key file generation"
3. User inserts blank or dedicated USB drive
4. VoidGate generates 32 bytes of cryptographically random entropy
5. VoidGate writes key file to USB drive: `voidgate.key`
6. VoidGate prompts: "Set vault password"
7. User enters strong password (≥12 characters recommended)
8. VoidGate derives master_key = Argon2id(password || key_file_bytes, salt)
9. VoidGate creates encrypted vault header with salt
10. User removes USB drive and stores securely (e.g., safe, locked drawer)
11. Later, user wants to access vault:
12. User launches VoidGate
13. VoidGate prompts: "Insert USB key and enter password"
14. User inserts USB drive
15. VoidGate reads key_file_bytes from USB drive
16. User enters password
17. VoidGate derives keys and unlocks vault
18. User accesses encrypted files
19. User locks vault and removes USB key

## Alternate Flows

### USB Key Lost or Destroyed

**Trigger**: User loses USB drive or drive becomes corrupted

**Steps**:
1. User attempts to unlock vault without USB key
2. VoidGate displays: "Key file not found — vault cannot be unlocked"
3. User cannot recover vault (by design — no password-only fallback)
4. Vault remains encrypted and inaccessible permanently
5. Flow terminates with data loss

**Mitigation**: User should create backup copy of USB key file and store in separate secure location (e.g., safety deposit box)

### Password Forgotten

**Trigger**: User forgets password but has USB key

**Steps**:
1. User inserts USB key
2. User enters incorrect password
3. VoidGate derives wrong master_key
4. Vault header decryption fails
5. VoidGate displays generic "Authentication failed"
6. User cannot recover password (no reset mechanism)
7. Vault remains encrypted and inaccessible

**Mitigation**: User should use password manager or write password in physical safe

### USB Key Compromised (Attacker Has Copy)

**Trigger**: Attacker copies USB key file but does not have password

**Steps**:
1. Attacker obtains USB key file (e.g., physical theft, malware)
2. Attacker attempts to unlock vault without password
3. Attacker tries password brute-force
4. Argon2id (m=19456 KiB, t=2) makes brute-force computationally expensive
5. Vault remains secure as long as password has sufficient entropy
6. User should rotate vault (create new USB key + password) if compromise suspected

### Creating Backup USB Key

**Trigger**: User wants redundant USB key for disaster recovery

**Steps**:
1. User unlocks vault with original USB key + password
2. User inserts second USB drive
3. User selects "Export Key File" in VoidGate
4. VoidGate copies `voidgate.key` to second USB drive
5. User stores backup USB key in separate secure location
6. Both USB keys are now valid for vault unlock (identical key_file_bytes)

## Success Criteria

- Vault cannot be unlocked with password alone (USB key mandatory)
- Vault cannot be unlocked with USB key alone (password mandatory)
- USB key file is deterministic (same bytes always produce same master_key)
- No cloud-based factors (TOTP servers, SMS gateways) required
- User has full control over hardware factor (can create backups, store securely)
- Authentication is offline (does not require internet connection)

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — USB key file format, Argon2id key derivation, mandatory dual-factor authentication, no password-only fallback
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — CSPRNG for key generation, deterministic key derivation tree
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Vault header stores salt (unencrypted) for key derivation on any device

## Security Considerations

### Threats Addressed

- **Password-only attacks**: Attacker with password but no USB key cannot unlock vault
- **USB key-only attacks**: Attacker with USB key but no password faces computationally expensive offline brute-force attack due to Argon2id cost (feasibility depends on password entropy and KDF parameters)
- **Phone compromise**: No SMS or TOTP app on phone (attacker gaining phone access does not compromise vault)
- **Cloud authenticator breach**: No cloud-based factors (Google Authenticator, Authy synced accounts)
- **Biometric spoofing**: No fingerprint/face recognition (no biometric database risk)

### Assumptions

- User physically secures USB key (locked drawer, safe, or safety deposit box)
- User creates backup USB key and stores separately (mitigates loss/destruction)
- User chooses strong password (≥12 characters, mixed case, symbols, numbers)
- Argon2id parameters (m=19456 KiB, t=2, p=1) provide sufficient brute-force resistance

### Out of Scope

- Social engineering attacks (user coerced into providing USB key + password)
- Malware on user's device capturing USB key file during session
- Physical torture/coercion to extract password
- Quantum computing implications for password-derived key search and key derivation parameters (future consideration; Grover-style speedups may reduce effective brute-force cost, but no claim of quantum resistance) <!-- CITE: RFC 9106; NIST post-quantum cryptography guidance -->
- Key rotation after compromise (user must manually create new vault and re-encrypt)

## Notes

This use case addresses a growing concern: users who distrust phone-based MFA due to SIM swapping, malware, or cloud authenticator breaches. VoidGate's USB key approach is:
- **Deterministic**: Same bytes always work (unlike TOTP time-based codes)
- **Offline**: No internet required for authentication
- **User-controlled**: No third-party service holds recovery keys

**Trade-off**: USB key loss = permanent data loss (no account recovery, no password reset). This is an intentional design choice — user convenience vs. security. Users who require recoverability should use traditional cloud storage with account recovery.

**Comparison to YubiKey**: Unlike YubiKeys (challenge-response, FIDO2), VoidGate's USB key file is a simple 32-byte file that can be:
- Copied to multiple USB drives (backups)
- Stored in password managers (though this reduces physical separation)
- Printed as QR code or hex string (extreme backup strategy)

This flexibility is a feature, not a bug — users have full control over backup strategies.

---

**References**:
- NIST SP 800-63B: Digital Identity Guidelines (Authenticator Types)
- YubiKey vs. File-Based Keys: [FIDO Alliance](https://fidoalliance.org/)
- Argon2id: [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106.html)
