# UC-BIZ-004: Zero-Trust Architecture Compliance

**Category**: Business & Enterprise

**Status**: Active

---

## Overview

An organization implementing Zero Trust architecture requires that no system — including cloud storage providers — is implicitly trusted with sensitive data. VoidGate provides cryptographic enforcement of "never trust, always verify" by ensuring cloud providers never access plaintext.

## Actors

- **Primary Actor**: Security architect or CISO implementing Zero Trust
- **Secondary Actors**: Employees, Cloud provider (explicitly untrusted), VoidGate system

## Preconditions

- Organization has adopted Zero Trust principles (NIST SP 800-207)
- Organization has identified cloud storage as a trust boundary requiring encryption
- Organization has deployed VoidGate with company-controlled authentication
- Organization has network segmentation and endpoint security in place

## Main Flow

1. Security architect defines Zero Trust requirements:
   - No cloud provider is trusted with plaintext data
   - All data must be encrypted before leaving trusted network
   - Authentication must use multiple factors (hardware + knowledge)
   - Access must be continuously verified (session timeout)
2. Security architect selects VoidGate as zero-knowledge cloud storage solution
3. Security architect documents trust boundaries:
   - **Trusted**: User workstations (managed, endpoint protection)
   - **Untrusted**: Cloud provider, network, internet transit
   - **Trust boundary**: VoidGate encryption layer
4. Employee uploads sensitive file:
5. VoidGate encrypts file on trusted workstation (before leaving trust boundary)
6. VoidGate uploads encrypted data to untrusted cloud provider
7. Cloud provider stores opaque blobs (no plaintext access)
8. Employee on different device accesses file:
9. Employee authenticates with hardware MFA (USB key) + password
10. VoidGate verifies session continuously (timeout after inactivity)
11. VoidGate downloads encrypted data from untrusted cloud
12. VoidGate decrypts only within trusted workstation (after authentication)
13. Employee accesses plaintext via download to controlled destination (ephemeral or persistent per policy)
14. Session expires after timeout — employee must re-authenticate
15. Security architect audits trust boundaries:
16. Security architect verifies encryption at rest (cloud blobs are ciphertext)
17. Security architect verifies encryption in transit (TLS + encrypted payload)
18. Security architect confirms no implicit trust in cloud provider
19. Zero Trust compliance validated

## Alternate Flows

### Session Timeout Enforcement

**Trigger**: Employee leaves workstation idle for 15 minutes

**Steps**:
1. VoidGate detects inactivity (no user interaction for 15 minutes)
2. VoidGate zeroizes session keys from memory (mlocked RAM overwritten)
3. VoidGate locks vault automatically
4. Employee returns and attempts to access file
5. VoidGate prompts: "Session expired — re-authenticate"
6. Employee must unlock vault again with USB key + password
7. Flow continues after re-authentication (continuous verification)

### Network Compromise (Man-in-the-Middle)

**Trigger**: Attacker intercepts traffic between workstation and cloud

**Steps**:
1. Attacker positions between workstation and cloud (MITM)
2. Attacker captures encrypted blobs in transit
3. Attacker cannot decrypt (no session keys, encrypted payload)
4. VoidGate uses TLS for transport security (additional layer)
5. Even if TLS is compromised, encrypted blobs remain protected
6. Zero Trust principle: assume network is hostile (defense in depth)

### Endpoint Compromise Detection

**Trigger**: Security team detects malware on employee workstation

**Steps**:
1. Security team identifies compromised workstation
2. Security team immediately rotates company password
3. Security team rotates USB key file (creates new vault, migrates data)
4. Compromised workstation can no longer unlock vault
5. Security team wipes and re-images compromised workstation
6. Zero Trust principle: assume endpoints may be compromised (time-limited access)

### Cloud Provider Compromise

**Trigger**: Cloud provider infrastructure is breached (attacker gains root access)

**Steps**:
1. Attacker compromises cloud provider (AWS, Azure, etc.)
2. Attacker has full access to storage buckets and databases
3. Attacker extracts encrypted blobs (random UUIDs, ciphertext)
4. Attacker cannot decrypt (no keys stored in cloud)
5. Zero Trust principle: assume cloud is adversarial (cryptographic boundary)

## Success Criteria

- No cloud provider has access to plaintext data (cryptographic enforcement)
- Authentication requires multiple factors (USB key + password)
- Sessions expire after inactivity (continuous verification)
- Data is encrypted before leaving trusted workstation (trust boundary enforced)
- Network compromise does not expose plaintext (encrypted in transit and at rest)
- Endpoint compromise has time-limited impact (session timeout, key rotation)
- Organization can audit trust boundaries (encryption at rest, in transit, in use)

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — Dual-factor authentication, session timeout, mlocked memory for keys
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 encryption before data leaves trust boundary
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Encrypted manifest (no metadata leakage to cloud)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Untrusted cloud backend (BYOC), encrypted uploads

## Security Considerations

### Threats Addressed

- **Implicit trust in cloud**: Zero Trust principle — cloud is explicitly untrusted
- **Network eavesdropping**: Encrypted payload even if TLS is compromised
- **Endpoint persistence**: Plaintext written only to user-controlled destinations (no unmanaged persistence)
- **Session hijacking**: Session timeout limits exposure window
- **Provider-side attacks**: Cloud breach does not expose plaintext

### Assumptions

- Workstations are managed with endpoint protection (trusted during session)
- Organization implements physical security for USB key file
- Employees follow security policy (do not share USB key or password)
- Session timeout policy is enforced (VoidGate automatically locks vault)
- Organization has incident response plan for endpoint compromise

### Out of Scope

- **Trusted Execution Environments (TEE)**: VoidGate does not use SGX, TrustZone, or secure enclaves
- **Homomorphic encryption**: Cloud cannot compute on encrypted data (no processing in encrypted domain)
- **Quantum-resistant encryption**: Current algorithms are post-quantum vulnerable (future work)
- **Insider threats within organization**: User with vault access can exfiltrate data (not prevented)

## Notes

This use case positions VoidGate as a Zero Trust-compliant solution. Key mappings to Zero Trust principles (NIST SP 800-207):

**Zero Trust Principle 1**: No resource is inherently trusted
- VoidGate: Cloud provider is explicitly untrusted (encryption boundary)

**Zero Trust Principle 2**: Access requires strong authentication
- VoidGate: Dual-factor authentication (USB key + password)

**Zero Trust Principle 3**: Access is continuously verified
- VoidGate: Session timeout, no persistent plaintext

**Zero Trust Principle 4**: Assume breach (minimize blast radius)
- VoidGate: Compromised cloud = encrypted data only, compromised endpoint = time-limited access

**Zero Trust Principle 5**: Least privilege
- VoidGate: Employees access only decrypted files they unlock (no ambient authority)

**Comparison to Traditional Cloud Storage**:
- **Traditional**: Implicit trust in provider (provider can decrypt for lawful access, AI processing)
- **VoidGate**: Explicit distrust (provider has no keys, no plaintext access)

**Organizational Fit**: VoidGate is a control in Zero Trust architecture, specifically addressing:
- **Data security**: Encryption at trust boundaries
- **Identity**: Hardware MFA (USB key)
- **Device security**: Session timeout, no persistent plaintext

---

**References**:
- NIST SP 800-207: Zero Trust Architecture
- Forrester: Zero Trust eXtended (ZTX) Ecosystem
- Google BeyondCorp: Zero Trust implementation case study
