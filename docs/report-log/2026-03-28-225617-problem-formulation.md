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

2. **USB hardware factor in authentication:** How can a physical USB device be integrated into the authentication flow (as MFA or recovery mechanism) to ensure that knowledge of the password alone is insufficient to compromise data?
   <!-- SOURCE: NIST SP 800-63B: Digital Identity Guidelines—Authentication and Lifecycle Management — https://pages.nist.gov/800-63-3/sp800-63b.html — defines Authenticator Assurance Levels AAL1–AAL3 and requirements for multi-factor authentication -->
   <!-- SOURCE: FIDO User Authentication Specifications — https://fidoalliance.org/specifications/ — defines FIDO2/WebAuthn and CTAP protocols for hardware-backed and passwordless authentication -->

3. **Chunking and synchronisation without metadata leakage:** How can effective chunking and synchronisation logic be implemented to upload changes to the cloud without revealing filenames, directory structures, or metadata to the cloud provider?
   <!-- SOURCE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf — analyses security vulnerabilities in CDC implementations across major backup tools; supports the choice of fixed-size chunking for metadata privacy -->
   <!-- SOURCE: Chunking Attacks on File Backup Services using Content-Defined Chunking — https://eprint.iacr.org/2025/532.pdf — demonstrates that CDC leaks file size and content information as a side channel in backup services -->
   <!-- SOURCE: Cryptomator Security Architecture — https://docs.cryptomator.org/security/architecture — "Files are transparently en- and decrypted. There are no unencrypted copies on your hard disk drive." — key architectural comparison point -->

4. **RAM-based UI vs. virtual filesystem for Zero-Trace:** What are the advantages and disadvantages of presenting data through an isolated application UI (RAM-based) compared to a virtual filesystem, when the goal is to leave the fewest possible traces on the host machine?
   <!-- SOURCE: WinFSP — Windows File System Proxy — https://github.com/winfsp/winfsp — user-mode file system framework for Windows enabling FUSE-style virtual filesystem implementations; represents the virtual filesystem alternative to the RAM-based UI approach -->
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
| 2 — USB hardware factor | `src-tauri/src/auth/` | Method, Analysis |
| 3 — Chunking & sync | `src-tauri/src/storage/` | Method, Analysis |
| 4 — RAM-based UI vs. FUSE | `src-tauri/src/ui/` | Discussion |
| 5 — File sharing | `src-tauri/src/sharing/` | Method, Analysis, Discussion |

This mapping provides the structural backbone ("rød tråd") for the report: the problem formulation drives the architecture, the architecture drives the implementation, and sub-conclusions per module feed the final conclusion.

## References

<!-- SOURCE: UCL Erhvervsakademi og Professionshøjskole — Bachelorprojekt Guide og vejledning v.2.0 (2026), PBA Softwareudvikling og PBA Webudvikling — defines formal requirements: 30 normalsider max (solo), objective register, mandatory sections 7.3–7.7, APA citation standard recommended -->
<!-- SOURCE: Guideline for Using Cryptographic Standards in the Federal Government: Cryptographic Mechanisms — https://csrc.nist.gov/publications/detail/sp/800-175b/rev-1/final — NIST SP 800-175B Rev. 1 -->
<!-- SOURCE: Cryptomator Security Architecture — https://docs.cryptomator.org/security/architecture — primary comparison target for architectural analysis -->
