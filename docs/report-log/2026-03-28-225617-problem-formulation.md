---
timestamp: "2026-03-28T22:56:17+0100"
type: decision
report-sections:
  - problem
tags: [problem-formulation, scope, research-questions]
source: manual
commit: "703df41"
---

## Problem Formulation — Arx Runa

## Context

The bachelor project addresses a fundamental trust problem in cloud storage: current mainstream solutions (OneDrive, Google Drive, Dropbox) require users to trust the cloud provider with plaintext data, metadata, and file structure. A compromised or coerced provider can expose everything. The project proposes an alternative architecture where the cloud provider never receives intelligible data.

## Substance

The approved problem formulation (Danish original, advisor-approved):

> *Hvordan kan man designe og implementere en softwareløsning til sikker cloud-lagring, der gennem klient-baseret kryptering eliminerer behovet for tillid til tredjepartsudbydere, og hvordan kan anvendelsen af fysiske hardware-faktorer (MFA) samt "Zero-Trace" principper minimere den lokale angrebsflade på brugerens maskine?*

**English translation:** How can a software solution for secure cloud storage be designed and implemented such that client-side encryption eliminates the need for trust in third-party providers, and how can the use of physical hardware factors (MFA) and "Zero-Trace" principles minimise the local attack surface on the user's machine?

### Sub-questions

1. **Encryption standards and key management:** Which modern encryption standards and key management principles are best suited for ensuring data confidentiality and integrity when data must be stored in an environment outside the user's control?
   <!-- SOURCE: ChaCha20 and Poly1305 for IETF Protocols — https://datatracker.ietf.org/doc/html/rfc8439 — defines the ChaCha20 stream cipher and Poly1305 MAC as a combined AEAD construction for IETF protocols -->
   <!-- SOURCE: HMAC-based Extract-and-Expand Key Derivation Function (HKDF) — https://datatracker.ietf.org/doc/html/rfc5869 — specifies HKDF, a two-stage (extract and expand) key derivation function based on HMAC -->
   <!-- SOURCE: Guideline for Using Cryptographic Standards in the Federal Government: Cryptographic Mechanisms — https://csrc.nist.gov/publications/detail/sp/800-175b/rev-1/final — guidance on symmetric encryption, key establishment, and authentication for data at rest and in transit -->
   <!-- SOURCE: XChaCha: eXtended-nonce ChaCha and AEAD_XChaCha20_Poly1305 — https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha — defines the 192-bit nonce variant of ChaCha20, enabling safe random nonce generation per chunk without state tracking -->

2. **USB key file factor and offline recovery:** How can a physical USB key file be integrated into the authentication flow as a mandatory second factor — ensuring that password knowledge alone is insufficient to access vault data — and how can an offline BIP-39 recovery mechanism enable user-controlled credential recovery without delegating trust to a third party or introducing a server-side backdoor?
   <!-- SOURCE: NIST SP 800-63B: Digital Identity Guidelines—Authentication and Lifecycle Management — https://pages.nist.gov/800-63-3/sp800-63b.html — defines Authenticator Assurance Levels AAL1–AAL3 and requirements for multi-factor authentication -->
   <!-- SOURCE: BIP-39: Mnemonic code for generating deterministic keys — https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki — specifies the 2048-word wordlist, checksum scheme, and PBKDF2-HMAC-SHA512 derivation used as the basis for Arx Runa's recovery phrase mechanism -->
   <!-- SOURCE: Argon2: Memory-Hard Function for Password Hashing and Other Applications — https://www.rfc-editor.org/rfc/rfc9106 — RFC 9106, specifies Argon2id parameters and security considerations; used in Arx Runa's recovery_key derivation from the BIP-39 phrase -->

3. **Chunking, synchronisation, and provider-agnostic storage:** How can effective chunking and synchronisation logic be implemented to upload changes to the cloud without revealing filenames, directory structures, or metadata to the cloud provider — and how can the synchronisation protocol maintain consistency across multiple devices while remaining provider-agnostic, enabling redundant backup to multiple destinations without re-encryption?
   <!-- SOURCE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf — analyses security vulnerabilities in CDC implementations across major backup tools; supports the choice of fixed-size chunking for metadata privacy -->
   <!-- SOURCE: Chunking Attacks on File Backup Services using Content-Defined Chunking — https://eprint.iacr.org/2025/532.pdf — demonstrates that CDC leaks file size and content information as a side channel in backup services -->
   <!-- SOURCE: Cryptomator Security Architecture — https://docs.cryptomator.org/security/architecture — "Files are transparently en- and decrypted. There are no unencrypted copies on your hard disk drive." — key architectural comparison point -->

4. **Zero-Trace operation through a RAM-based UI:** How can a RAM-based in-application UI achieve Zero-Trace operation — ensuring that decrypted file content is never written to disk during a session — and what forensic residue, if any, persists on the host machine after the vault is locked?
   <!-- SOURCE: Resident $DATA Residue in NTFS MFT Entries — https://www.sans.org/blog/resident-data-residue-in-ntfs-mft-entries/ — SANS Institute: documents specific MFT artifacts and how file access operations leave forensic traces in NTFS Master File Table records -->
   <!-- SOURCE: Memory Forensic Acquisition and Analysis 101 — https://www.sans.org/blog/memory-forensic-acquisition-and-analysis-101/ — SANS Institute: covers volatile RAM evidence that does not persist to disk; supports the claim that RAM-only decryption leaves no filesystem artifacts -->
   <!-- SOURCE: RAM Forensics: Tools, Techniques, and Best Practices — https://belkasoft.com/ram-forensics-tools-techniques — Belkasoft: documents what evidence exists only in volatile memory and is unrecoverable after power-off; supports the Zero-Trace argument for RAM-based UI -->
   <!-- SOURCE: Harnessing MFT Parsing for Incident Response Investigations — https://www.magnetforensics.com/blog/harnessing-mft-parsing-for-incident-response-investigations/ — Magnet Forensics: demonstrates the breadth of forensic data recoverable from NTFS MFT when files are accessed through the native filesystem -->

5. **File sharing in a zero-trust system:** What cryptographic and protocol-level challenges arise when enabling file-granularity sharing between independent users in a zero-trust client-side encrypted system, and how does the proposed sharing architecture compare to existing approaches such as OneDrive sharing links and Cryptomator shared vaults?
   <!-- SOURCE: Cryptomator Security Architecture — https://docs.cryptomator.org/security/architecture — documents client-side AES-256 encryption with a virtual filesystem layer; primary architectural comparison target for sharing model -->
   <!-- SOURCE: Protect your OneDrive files in Personal Vault — https://support.microsoft.com/en-us/office/protect-your-onedrive-files-in-personal-vault-6540ef37-e9bf-4121-a773-56f98dce78c4 — "Personal Vault is a protected area in OneDrive where you can store your most important or sensitive files" — provider-trust model sharing links, not zero-knowledge sharing -->
   <!-- SOURCE: age encryption tool — https://github.com/FiloSottile/age — reference implementation of X25519-based ECIES used by Arx Runa sharing layer for share package encryption -->

## Implications

Each sub-question maps to a distinct module in the Arx Runa architecture:

| Sub-question | Arx Runa module | Report section |
|---|---|---|
| 1 — Encryption & key management | `src-tauri/src/crypto/` | Method, Analysis |
| 2 — USB key file factor + BIP-39 recovery | `src-tauri/src/auth/` | Method, Analysis |
| 3 — Chunking, sync, multi-device & multi-destination (UC2, UC5) | `src-tauri/src/storage/` | Method, Analysis |
| 4 — Zero-Trace RAM-based UI | `src-tauri/src/ui/` | Method, Analysis, Discussion |
| 5 — File sharing | `src-tauri/src/sharing/` | Method, Analysis, Discussion |

This mapping provides the structural backbone ("rød tråd") for the report: the problem formulation drives the architecture, the architecture drives the implementation, and sub-conclusions per module feed the final conclusion.

## References

<!-- SOURCE: UCL Erhvervsakademi og Professionshøjskole — Bachelorprojekt Guide og vejledning v.2.0 (2026), PBA Softwareudvikling og PBA Webudvikling — defines formal requirements: 30 normalsider max (solo), objective register, mandatory sections 7.3–7.7, APA citation standard recommended -->
<!-- SOURCE: Guideline for Using Cryptographic Standards in the Federal Government: Cryptographic Mechanisms — https://csrc.nist.gov/publications/detail/sp/800-175b/rev-1/final — NIST SP 800-175B Rev. 1 -->
<!-- SOURCE: Cryptomator Security Architecture — https://docs.cryptomator.org/security/architecture — primary comparison target for architectural analysis -->
