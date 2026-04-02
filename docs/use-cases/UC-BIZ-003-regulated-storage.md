# UC-BIZ-003: Regulated Industry Cloud Storage

**Category**: Business & Enterprise

**Status**: Active

---

## Overview

A healthcare, legal, or financial services organization operates under strict data protection regulations (HIPAA, GDPR, FINRA) and needs cloud storage that guarantees patient/client data never exists in plaintext outside their control.

## Actors

- **Primary Actor**: Compliance officer or data protection officer
- **Secondary Actors**: Healthcare providers, legal staff, financial advisors (data owners), Cloud provider (untrusted), VoidGate system

## Preconditions

- Organization operates under HIPAA, GDPR, FINRA, or similar regulations
- Organization has performed risk assessment and determined zero-knowledge encryption is required
- Organization has deployed VoidGate with company-controlled USB key and password policy
- Organization has configured Rclone to HIPAA-eligible cloud (e.g., AWS with BAA, Azure Government)

## Main Flow

1. Compliance officer evaluates cloud storage options
2. Compliance officer determines VoidGate satisfies zero-knowledge requirement
3. Organization deploys VoidGate to workstations
4. Organization generates company USB key file and stores in secure, audited location
5. Organization establishes password policy (minimum complexity, rotation schedule)
6. Organization configures Rclone to HIPAA-eligible or GDPR-compliant cloud backend
7. Healthcare provider (e.g., doctor) needs to store patient records:
8. Provider unlocks vault with company password + USB key
9. Provider uploads patient record (PDF, images, notes)
10. VoidGate encrypts record with XChaCha20-Poly1305 before upload
11. VoidGate uploads encrypted chunks to cloud (random UUIDs, no PHI in blob names)
12. Provider locks vault
13. Auditor performs compliance review:
14. Auditor requests evidence that PHI/PII is encrypted at rest and in transit
15. Compliance officer demonstrates:
    - Cloud provider never receives plaintext PHI/PII
    - Encryption uses industry-standard algorithms (XChaCha20-Poly1305, Argon2id)
    - Cloud blobs are opaque (no identifiable information)
    - Access requires dual-factor authentication (USB key + password)
16. Auditor reviews VoidGate design documents and threat model
17. Auditor verifies cloud provider BAA (Business Associate Agreement) is in place
18. Auditor confirms compliance with HIPAA Security Rule (45 CFR Part 164 Subpart C)
19. Organization passes audit

## Alternate Flows

### Data Breach at Cloud Provider

**Trigger**: Cloud provider suffers data breach (attacker gains access to storage)

**Steps**:
1. Cloud provider notifies organization of breach
2. Organization assesses impact:
   - Attacker has encrypted blobs (random UUIDs, no metadata)
   - Attacker does not have USB key file or password
   - Encrypted data is protected by XChaCha20-Poly1305
3. Organization determines NO PHI/PII exposure (data remains encrypted)
4. Organization reports breach to regulators with evidence that data was encrypted
5. Organization avoids HIPAA penalties (breach notification exemption for encrypted data)

### GDPR Right to Erasure (Right to be Forgotten)

**Trigger**: EU citizen (patient/client) requests data deletion under GDPR Article 17

**Steps**:
1. Data subject submits erasure request
2. Organization identifies records in VoidGate manifest
3. Organization selects records for deletion
4. VoidGate deletes encrypted chunks from cloud (blob deletion)
5. VoidGate removes entries from manifest
6. Organization provides confirmation to data subject
7. No residual plaintext or metadata remains in cloud

### Regulatory Audit with Data Access

**Trigger**: Regulator requires access to specific records (e.g., FINRA inspection)

**Steps**:
1. Regulator requests access to specific client records
2. Compliance officer unlocks vault with company USB key + password
3. Compliance officer searches manifest for requested records
4. Compliance officer downloads and decrypts records
5. Compliance officer provides plaintext records to regulator (in secure format)
6. Regulator reviews records
7. Regulator confirms organization has control over data (can produce on demand)

### Encryption Key Escrow (Regulatory Requirement)

**Trigger**: Some regulations require key escrow for government access

**Steps**:
1. Organization stores backup USB key file in escrow (e.g., with legal counsel or escrow service)
2. If regulatory authority demands access: organization provides escrowed USB key + password
3. Authority unlocks vault and accesses records
4. VoidGate design supports this (no backdoor required — escrowed key is legitimate)

## Success Criteria

- Patient health information (PHI) or personally identifiable information (PII) never exists in plaintext at cloud provider
- Cloud provider cannot perform content analysis, indexing, or AI processing on sensitive data
- Organization can demonstrate encryption at rest and in transit to auditors
- Data breach at cloud provider does not expose PHI/PII (breach notification exemption applies)
- Organization complies with GDPR right to erasure (can delete encrypted data)
- Organization maintains audit logs for access control (who accessed what, when)
- Organization can produce records on demand for regulatory inspections

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — Dual-factor authentication (USB key + password), session timeout, access control
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 AEAD (industry-standard), Argon2id key derivation, BLAKE3 integrity
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Encrypted manifest (no PHI/PII metadata visible), fixed-size chunks (no size inference)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Rclone BYOC (HIPAA-eligible backend), encrypted vault header, manifest backup

## Security Considerations

### Threats Addressed

- **Cloud provider breach**: Encrypted data rendered unusable/indecipherable may not be subject to breach notification under HIPAA Safe Harbor (45 CFR § 164.402) <!-- CITE: 45 CFR § 164.402; HHS Guidance on Risk Analysis -->
- **Insider threats at provider**: Provider employees cannot access PHI/PII
- **Unauthorized AI processing**: Cloud cannot run ML on encrypted records (privacy preserved)
- **Traffic analysis**: Fixed-size chunks prevent file size inference (no distinguishing MRI vs. prescription)
- **Regulatory penalties**: Zero-knowledge architecture satisfies HIPAA, GDPR, FINRA requirements

### Assumptions

- Organization secures USB key in audited, access-controlled location
- Organization enforces strong password policy (password manager, rotation)
- Organization uses HIPAA-eligible or GDPR-compliant cloud provider (AWS BAA, Azure Government, etc.)
- Organization implements backup and disaster recovery for USB key
- Workstations are hardened (antivirus, endpoint protection, no unauthorized software)

### Out of Scope

- **Quantum computing attacks**: Current encryption is post-quantum vulnerable (future consideration)
- **Physical coercion**: Attacker physically coercing user to provide USB key + password
- **Malware on workstation**: Keylogger or screen capture during session
- **Key escrow backdoors**: VoidGate has no backdoor — escrowed key is legitimate access

## Notes

This use case is critical for VoidGate's applicability in regulated industries. Key regulatory mappings:

**HIPAA (US Healthcare)**:
- **Security Rule (164.312)**: Encryption and decryption (addressable) — VoidGate uses industry-standard encryption
- **Breach Notification (164.402)**: Encrypted data exempt from breach notification if key not compromised
- **Business Associate Agreement (BAA)**: Cloud provider must sign BAA, but cannot access PHI

**GDPR (EU Privacy)**:
- **Article 32**: Security of processing — encryption at rest and in transit
- **Article 17**: Right to erasure — VoidGate supports deletion of encrypted blobs
- **Article 33**: Breach notification — encrypted data may reduce notification burden

**FINRA (US Financial)**:
- **Rule 4511**: Books and records — VoidGate enables secure storage and retrieval
- **SEA Rule 17a-4**: Immutable records (VoidGate does not implement WORM, but cloud backend can)

**Comparison to Traditional Compliance Solutions**:
- **Encrypted databases (TDE)**: Provider still sees metadata and structure
- **Customer Managed Keys (CMK)**: Provider sees ciphertext but may have key access
- **VoidGate**: Zero-knowledge — provider has no keys, metadata, or access

---

**References**:
- HIPAA Security Rule: 45 CFR § 164.312(a)(2)(iv)
- GDPR Article 32: Security of Processing
- NIST SP 800-66: HIPAA Security Rule Implementation
- FINRA Regulatory Notice 17-18: Electronic Storage of Records
