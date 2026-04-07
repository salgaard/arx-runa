# Arx Runa: Market Opportunities & Future Directions

> **Document type**: Exploration / brainstorming  
> **Status**: Living document  
> **Last updated**: 2026-04-06

This document explores market positioning, mainstream adoption strategies, and future opportunities for Arx Runa. It includes speculative ideas and preliminary implementation notes.

---

## Table of Contents

1. [Current Landscape](#current-landscape)
2. [Consumer Privacy Sentiment](#consumer-privacy-sentiment)
3. [Market Opportunity](#market-opportunity)
4. [Progressive Security Model](#progressive-security-model)
5. [Auto-Sync UI (Drop Zone)](#auto-sync-ui-drop-zone)
6. [Alternative Authentication Factors](#alternative-authentication-factors)
7. [Post-Quantum Cryptography](#post-quantum-cryptography)
8. [AI Privacy & Private Cloud Compute](#ai-privacy--private-cloud-compute)
9. [Future Trends & Timing](#future-trends--timing)
10. [Feature Ideas for Mainstream Appeal](#feature-ideas-for-mainstream-appeal)
11. [Recommendation](#recommendation)
12. [Decisions](#decisions)
13. [Open Questions](#open-questions)
14. [Sources](#sources)

---

## Current Landscape

### Zero-Knowledge Storage Competitors

| Product | Model | Stars/Users | Key Differentiator |
|---------|-------|-------------|-------------------|
| **Cryptomator** | Open-source, free desktop, paid mobile (~$12) | 14.8k GitHub stars | Virtual drive integration, multi-cloud |
| **Tresorit** | Proprietary SaaS, subscription ($10-24/mo) | Enterprise focus | Swiss jurisdiction, ISO 27001, HIPAA/GDPR compliant |
| **Proton Drive** | Freemium SaaS | Part of Proton ecosystem | Bundled with email/VPN, brand trust |
| **Boxcryptor** | Freemium (acquired by Dropbox 2022) | Integrated into Dropbox now | — |
| **Internxt** | Open-source, freemium | 146 GitHub stars | Decentralized, "privacy as human right" positioning |
| **Peergos** | Open-source, P2P | 2.4k GitHub stars | IPFS-based, post-quantum ready |

### Competitor Deep Dive: Tresorit

From [Tresorit Security Page](https://tresorit.com/security):
- **Zero-knowledge authentication**: Password never leaves device
- **Non-convergent cryptography**: Unlike some E2E providers, no content-based deduplication (which can leak info)
- **RSA-4096 with OAEP** for key sharing + PKI certificates
- **Client-side integrity protection**: HMAC/AEAD authentication
- **Swiss privacy laws**: Stronger than US/EU
- **Compliance**: ISO 27001:2022, GDPR, HIPAA BAA, CCPA

**Arx Runa advantage**: Tresorit is SaaS-locked. Arx Runa's BYOC model means you own your cloud backend.

### Competitor Deep Dive: Internxt

From [Internxt Privacy Page](https://internxt.com/privacy):
- Positions privacy as "fundamental human right"
- Open-source approach
- Consumer-focused messaging

**Arx Runa advantage**: Hardware MFA, zero-trace architecture, more paranoid threat model.

### Arx Runa's Unique Position

Arx Runa differentiates on:
- **Hardware MFA as cryptographic requirement** — not just convenience 2FA
- **BYOC (Bring Your Own Cloud)** via Rclone — true vendor independence
- **Zero-Trace UI** — RAM-only, no temp files, paranoid by design
- **Fixed-size chunking** — prevents file size fingerprinting

**Gap in market**: No mainstream solution requires hardware-bound keys as a cryptographic factor while remaining cloud-agnostic.

---

## Consumer Privacy Sentiment

### Pew Research Center Survey (May 2023)

Source: [How Americans View Data Privacy](https://www.pewresearch.org/internet/2023/10/18/how-americans-view-data-privacy/)

**Key findings**:

| Statistic | Value |
|-----------|-------|
| Concerned about how **government** uses their data | 71% (up from 64% in 2019) |
| Concerned about how **companies** use their data | ~80% |
| Say they understand little/nothing about what companies do with data | 67% (up from 59%) |
| Feel they have little/no control over company data collection | 73% |
| Feel they have little/no control over government data collection | 79% |
| Support **more regulation** of company data practices | 72% |
| Little/no trust in social media CEOs to handle data responsibly | 77% |

**AI-specific concerns** (among those who've heard of AI):
- 70% have little/no trust in companies to make responsible AI decisions
- 81% expect AI use will lead to data being used in ways people won't be comfortable with
- 80% expect AI use will lead to unintended data uses

**Children's privacy**:
- 89% concerned about social media platforms knowing personal info about kids
- 85% say parents have responsibility to protect kids' online privacy
- 59% say tech companies have responsibility

**Actionable insight for Arx Runa**: 
- Privacy concern is widespread and growing
- Trust in tech companies is low
- There's appetite for tools that give users control
- AI privacy backlash is real and measurable

---

## Market Opportunity

### Target Segments (Priority Order)

#### 1. Privacy-Conscious Professionals (Near-term)
- Journalists protecting sources
- Lawyers with client privilege obligations
- Healthcare workers (HIPAA)
- Security researchers

**Why they'll pay**: Compliance requirements, professional liability, career risk.

#### 2. Developer/Technical Users (Near-term)
- Encrypted secrets management (API keys, SSH keys, .env files)
- Git-integrated encrypted config
- Self-hosting enthusiasts

**Why they'll pay**: Workflow integration, trust in open-source.

#### 3. Privacy-Aware Consumers (Mid-term)
- Post-Snowden privacy advocates
- Users fleeing Google Photos / iCloud after AI training announcements
- Parents protecting family photos
- Crypto/Web3 users (already have hardware wallets)

**Why they'll pay**: Emotional resonance ("my photos are mine").

#### 4. Mainstream Users (Long-term, requires UX investment)
- General cloud storage users
- Require "it just works" experience
- Need progressive security model

### Revenue Model Ideas

| Model | Pros | Cons |
|-------|------|------|
| **Cryptomator model** (free desktop, paid mobile) | Proven, low friction | Mobile development cost |
| **Freemium storage tiers** | Scales with usage | Need hosting (conflicts with BYOC) |
| **Enterprise/Team plan** | High ARPU | Sales complexity |
| **Hardware bundle** (sell USB keys with software) | Physical product margin | Logistics, returns |
| **Donation/Sponsor** | Simple | Unpredictable |

**Recommendation**: Start with Cryptomator model. Desktop free, mobile paid. Add enterprise tier later.

---

## Progressive Security Model

**Core insight**: All tiers remain zero-knowledge. "Progressive" refers to authentication strength, not encryption strength.

### Tier Overview

| Tier | Name | Auth Factors | Target User | Zero-Knowledge? |
|------|------|--------------|-------------|-----------------|
| 1 | **Easy** | Password only | Casual users migrating from Dropbox | ✅ Yes |
| 2 | **Balanced** | Password + Biometric (or NFC) | Security-aware mainstream | ✅ Yes |
| 3 | **Paranoid** | Password + USB Key File (current model) | High-risk users | ✅ Yes |

### Why Password-Only is Still Zero-Knowledge

Zero-knowledge means the server never sees plaintext or keys. Password-only achieves this:
- Key derivation happens client-side (Argon2id)
- Encrypted blobs uploaded to cloud
- Cloud provider cannot decrypt

**Trade-off**: Password-only is vulnerable to:
- Weak passwords
- Password reuse
- Phishing
- Keyloggers

Hardware factors add **key material** the attacker must physically possess.

### Draft Implementation Notes

```
Tier 1 (Password-only):
  master_key = Argon2id(password, vault_salt)
  
Tier 2 (Password + Biometric):
  # Biometric unlocks a device-stored key
  device_key = Platform.BiometricProtectedKey()  # OS keychain
  master_key = Argon2id(password || device_key, vault_salt)
  
Tier 3 (Password + USB Key File):
  key_file_bytes = read(usb_key_file)  # 32 bytes entropy
  master_key = Argon2id(password || key_file_bytes, vault_salt)
```

**Key insight for Tier 2**: The biometric doesn't directly derive the key — it unlocks a device-bound secret from the OS keychain (Windows Hello, macOS Secure Enclave, Android Keystore). This secret is then combined with the password.

### Upgrade/Downgrade Path

- **Upgrade**: Re-encrypt vault header with new auth factors, keep data blobs unchanged
- **Downgrade**: Should require current auth + confirmation (prevent attacker downgrade)
- **Migration UI**: "Add stronger protection" wizard

### Security Comparison

| Attack Vector | Tier 1 | Tier 2 | Tier 3 |
|---------------|--------|--------|--------|
| Weak password | ❌ Vulnerable | ⚠️ Partially mitigated | ✅ Mitigated |
| Password stolen (phishing/breach) | ❌ Compromised | ✅ Protected | ✅ Protected |
| Device stolen (unlocked) | ❌ Compromised | ❌ Compromised | ✅ Protected |
| Device stolen (locked) | ✅ Protected | ✅ Protected | ✅ Protected |
| Remote attacker | ❌ If password known | ✅ Need device | ✅ Need USB |
| Physical attacker + time | ⚠️ Argon2 slows | ⚠️ Biometric bypass risk | ✅ USB not present |

---

## Auto-Sync UI (Drop Zone)

### The Problem

Current encrypted storage tools require manual encrypt/decrypt workflows. Users want "Dropbox but private."

### The Vision

A drag-and-drop zone in the Arx Runa UI that:
1. Accepts files/folders dropped onto it
2. Encrypts immediately (client-side)
3. Uploads encrypted chunks to configured cloud
4. Shows sync status
5. Maintains zero-knowledge (no temp files)

### Why UI-Based (Not Filesystem)

**Traditional approach** (virtual drive / FUSE):
- Mounts as drive letter (e.g., `V:\`)
- Any app can save directly
- **Problem**: OS may create temp files, thumbnails, search indexes → breaks zero-trace

**UI-based approach**:
- User explicitly drops files into Arx Runa window
- Arx Runa reads directly from source, encrypts in RAM, uploads
- Source file remains unchanged (or optionally deleted)
- **Advantage**: Full control over data flow, true zero-trace

### Draft UX Flow

```
┌─────────────────────────────────────────┐
│  Arx Runa                    [—][□][×]  │
├─────────────────────────────────────────┤
│                                         │
│   ┌─────────────────────────────────┐   │
│   │                                 │   │
│   │     📁 Drop files here          │   │
│   │        or click to browse       │   │
│   │                                 │   │
│   └─────────────────────────────────┘   │
│                                         │
│   Recent uploads:                       │
│   ✅ family-photos.zip     2 min ago   │
│   ✅ tax-2025.pdf          5 min ago   │
│   ⏳ project-backup.tar    uploading... │
│                                         │
└─────────────────────────────────────────┘
```

### Watch Folder (Optional Enhancement)

For power users who want auto-sync:
- User designates a "hot folder" on disk
- Arx Runa watches it (via file system events)
- New files → encrypt → upload → optionally delete source
- **Trade-off**: Source folder has plaintext briefly, but Arx Runa never writes unencrypted

### Implementation Notes (Tauri)

```rust
// Drag-and-drop handler
#[tauri::command]
async fn handle_drop(paths: Vec<PathBuf>, state: State<VaultState>) -> Result<()> {
    for path in paths {
        // Stream-read from source, encrypt in RAM, upload chunks
        let file = BufReader::new(File::open(&path).await?);
        encrypt_and_upload_stream(file, &state).await?;
    }
    Ok(())
}
```

**Key constraint**: Never write plaintext to disk. Use `tokio::io` streaming, encrypt chunk-by-chunk.

---

## Alternative Authentication Factors

### Overview

| Factor | Type | Pros | Cons | Zero-Knowledge? |
|--------|------|------|------|-----------------|
| **Password** | Knowledge | Universal, no hardware | Weak if user chooses poorly | ✅ |
| **USB Key File** | Possession (file) | Strongest, explicit entropy | UX friction, loss risk | ✅ |
| **Biometric** | Inherence | Convenient, familiar | Device-bound, spoofable | ✅ (if done right) |
| **NFC Tag** | Possession (tap) | Convenient, cheap ($2/tag) | Clonable, not tamper-resistant | ✅ |
| **Hardware Wallet** | Possession (crypto) | Tamper-resistant, crypto users have them | Complex integration, niche | ✅ |
| **Passkey/FIDO2** | Possession + Biometric | Industry standard, phishing-resistant | Designed for auth, not KDF | ⚠️ Complex |

### Biometric Integration

**How it works**:
1. On vault creation, generate random 32-byte `device_key`
2. Store `device_key` in OS secure storage, protected by biometric
   - Windows: Windows Hello + Credential Guard
   - macOS: Secure Enclave + Touch ID / Face ID
   - Linux: (limited — TPM or fallback to password)
3. On unlock: biometric releases `device_key`, combined with password for Argon2

**Tauri integration**:
- Use `tauri-plugin-biometric` or native bindings
- Platform detection to show appropriate UI

**Limitation**: Device-bound. Lose device → lose vault access (unless backup recovery method).

### NFC Tag Support

**Use case**: Tap phone/tag to NFC reader as second factor.

**How it works**:
1. User purchases blank NFC tag (NTAG215, ~$2)
2. Arx Runa writes 32 bytes of random entropy to tag (write-protected after)
3. On unlock: user taps tag, Arx Runa reads entropy, combines with password

**Advantages**:
- Cheap and replaceable
- Familiar gesture (like transit cards)
- Can create backups (multiple tags with same entropy)

**Limitations**:
- Tags can be cloned if attacker has physical access
- NFC readers not universal on desktops (need USB NFC reader)
- Mobile-first advantage (phones have NFC)

**Draft flow**:
```
┌─────────────────────────────────────────┐
│  Unlock Vault                           │
├─────────────────────────────────────────┤
│                                         │
│  Password: [••••••••••••]               │
│                                         │
│  Tap your NFC key:                      │
│     ┌─────────┐                         │
│     │  📱 ))) │  Waiting for tap...     │
│     └─────────┘                         │
│                                         │
│  [Unlock]                               │
└─────────────────────────────────────────┘
```

### Hardware Wallet Integration

**Target users**: Crypto enthusiasts who already own Ledger/Trezor.

**Why it's interesting**:
- Tamper-resistant secure element
- Users already trust it with high-value secrets
- Existing ecosystem (8.9k stars on Hanko, 1.1k on Frame)

**How it could work**:
1. Arx Runa generates 32-byte challenge
2. Hardware wallet signs challenge with device-specific key
3. Signature (deterministic) used as key material for Argon2
4. Same challenge + same device = same signature = same derived key

**Challenges**:
- Requires user interaction (button press on device)
- Different APIs per vendor (Ledger vs Trezor vs others)
- Firmware updates might change behavior

**Libraries**:
- `ledger-rs` (Rust bindings for Ledger)
- `trezor-client` (Rust)
- Or use WebUSB in frontend

### Passkey / FIDO2 Consideration

**Important caveat**: Passkeys (FIDO2/WebAuthn) are designed for **authentication**, not **key derivation**.

**The problem**:
- Passkey gives you a signature, not deterministic key material
- Challenge-response changes each time (by design — replay protection)
- Can't directly derive encryption key from passkey

**Possible workarounds**:
1. **PRF extension** (WebAuthn Level 3): Allows deriving symmetric key from passkey
   - Limited browser/platform support as of 2026
   - Not universally available
2. **Hybrid**: Use passkey for auth, then release a stored `device_key`
   - Passkey protects access to key, not the key itself

**Recommendation**: Monitor PRF extension adoption. For now, simpler factors (biometric, NFC, USB file) are more practical.

### Passkey Industry Momentum

Source: [FIDO Alliance Passkeys](https://fidoalliance.org/passkeys/)

**What passkeys are**:
- FIDO authentication credentials based on FIDO standards
- Allow sign-in with same process as unlocking device (biometrics, PIN, pattern)
- Phishing-resistant and secure by design
- No passwords to steal, no sign-in data that can be used to perpetuate attacks

**Adoption stats**:
- 53% of people have enabled passkeys on at least one account
- 22% have enabled passkeys on every account they can
- 20% higher successful sign-in rate over passwords

**Benefits for organizations**:
- Higher sign-in success rates
- Reduction in phishing, credential stuffing
- Lower cart abandonment
- Reduced need for password resets
- Decreased customer support needs

**Arx Runa implications**:
- Users are becoming familiar with biometric + device auth patterns
- Passkey UX patterns can inform Tier 2 biometric flow
- Watch WebAuthn PRF extension for direct key derivation capability

---

## Post-Quantum Cryptography

### NIST PQC Standards (Finalized August 2024)

Source: [NIST News Release](https://www.nist.gov/news-events/news/2024/08/nist-releases-first-3-finalized-post-quantum-encryption-standards)

**Context**: Quantum computers could break current RSA/ECC encryption within a decade. NIST has finalized the first post-quantum cryptography standards after an 8-year international effort.

**The three finalized standards**:

| FIPS | Algorithm | Purpose | Renamed To |
|------|-----------|---------|------------|
| **FIPS 203** | CRYSTALS-Kyber | General encryption / key encapsulation | **ML-KEM** (Module-Lattice-Based Key-Encapsulation Mechanism) |
| **FIPS 204** | CRYSTALS-Dilithium | Digital signatures (primary) | **ML-DSA** (Module-Lattice-Based Digital Signature Algorithm) |
| **FIPS 205** | Sphincs+ | Digital signatures (backup, hash-based) | **SLH-DSA** (Stateless Hash-Based Digital Signature Algorithm) |

**FIPS 206** (upcoming): FALCON algorithm, to be renamed **FN-DSA**

**Key quotes from NIST**:
> "We encourage system administrators to start integrating them into their systems immediately, because full integration will take time."
> 
> "There is no need to wait for future standards. Go ahead and start using these three."

### CISA Post-Quantum Cryptography Initiative

Source: [CISA Quantum](https://www.cisa.gov/quantum)

**The threat**: 
- Quantum computing could break current encryption methods within a decade
- "Harvest now, decrypt later" attacks are already a concern
- Critical infrastructure systems rely on encryption that will become vulnerable

**CISA's four focus areas**:
1. **Risk Assessment**: Identify vulnerable systems across 55 National Critical Functions
2. **Planning**: Prioritize resources and stakeholder engagement
3. **Policy and Standards**: Foster adoption of PQC standards
4. **Engagement and Awareness**: Develop mitigation plans and technical products

**Most critical National Critical Functions for PQC migration**:
1. Provide Internet-Based Content, Information, and Communication Services
2. Provide Identity Management and Associated Trust Support Services
3. Provide Information Technology Products and Services
4. Protect Sensitive Information

**CISA recommendations for organizations**:
1. Inventory systems using public-key cryptography
2. Inventory, categorize, and determine lifecycle of organizational data
3. Test new PQC standards in lab environment
4. Create transition plan with interdependence analysis
5. Create acquisition policies regarding PQC
6. Alert IT departments and vendors
7. Educate workforce

### Arx Runa PQC Strategy

**Current state**:
- Arx Runa uses XChaCha20-Poly1305 (symmetric) — NOT vulnerable to quantum attacks
- HKDF-SHA256 for key derivation — NOT vulnerable
- Argon2id for password hashing — NOT vulnerable
- **No RSA/ECC in current design** — minimal quantum exposure

**Why Arx Runa is well-positioned**:
- Symmetric cryptography (ChaCha20) is quantum-resistant (Grover's algorithm only halves effective key length)
- 256-bit keys remain secure even against quantum computers
- No key exchange with remote servers (client-side only)
- HKDF allows easy algorithm substitution

**Future considerations**:
- If Arx Runa adds key sharing features, use ML-KEM (FIPS 203)
- If digital signatures needed, use ML-DSA (FIPS 204)
- Monitor Rust ecosystem: `pqcrypto` crate, RustCrypto implementations
- **Marketing opportunity**: Position as "quantum-ready" even though current design is already resistant

---

## AI Privacy & Private Cloud Compute

### Apple's Privacy-Preserving AI Model (June 2024)

Source: [The Verge - Apple AI Privacy](https://www.theverge.com/2024/6/10/24175405/wwdc-apple-ai-privacy-cloud-compute)

**Apple Intelligence** (announced WWDC 2024) demonstrates market demand for private AI:

**On-device processing**:
- Many AI features run locally on Apple Silicon
- "Deeply integrated into your iPhone, iPad, and Mac"
- "Designed to protect your privacy at every step"

**Private Cloud Compute** (for complex requests):
1. Only data required for request is sent to Apple Silicon servers
2. Request is processed — never stored or accessible to Apple
3. Response returned to user only
4. Independent experts can inspect server code to verify privacy

**Key insight**: Apple — the world's largest company — is betting that "great powers come with great privacy" is a winning market message.

### Consumer AI Privacy Concerns

From Pew Research (2023):
- 70% have little/no trust in companies to make responsible AI decisions
- 81% expect AI will lead to data being used in uncomfortable ways
- 80% expect AI will lead to unintended data uses

**The opportunity**: Users want AI capabilities but don't trust companies. Arx Runa could be the **private storage layer** for:
- Local AI conversation history
- AI-processed documents (summaries, analysis)
- Personal context/preferences for AI assistants
- Training data users want to keep private

### "AI-Proof" Storage Positioning

**Marketing angle**: "Your data trains you, not Big Tech"

**Technical backing**:
- Zero-knowledge means cloud provider can't use your data for AI training
- Client-side encryption means even if cloud is breached, data is useless
- BYOC means you choose providers with favorable AI training policies

**Potential features**:
1. **AI Memory Vault**: Encrypted storage for local AI assistant context
2. **Document Intelligence**: Process documents locally, store summaries encrypted
3. **Privacy-First Backup**: Back up your AI chat history without cloud access

---

## Future Trends & Timing

### Near-Term (2026-2028)

| Trend | Impact on Arx Runa | Action |
|-------|-------------------|--------|
| **AI training on user data** | Major privacy backlash, users seeking "AI-proof" storage | Marketing opportunity: "Your data stays yours" |
| **Passkey adoption** | Users comfortable with biometric + device auth | Prepare Tier 2 biometric UX |
| **Ransomware surge** | Demand for immutable encrypted backups | Position as ransomware-resilient |
| **EU Data Act / DSA enforcement** | Regulatory pressure for user-controlled encryption | Compliance selling point for EU enterprise |

### Mid-Term (2028-2032)

| Trend | Impact on Arx Runa | Action |
|-------|-------------------|--------|
| **Post-quantum migration** | NIST PQC standards finalized, early adopters migrating | Architect for algorithm agility (Arx Runa already uses HKDF — swap primitives) |
| **Decentralized storage maturity** | IPFS/Filecoin/Arweave become practical | BYOC via Rclone positions well — add backends |
| **Health data ownership** | Wearables generate sensitive biometric streams | "Health vault" feature — encrypted health data aggregation |
| **Hardware wallet ubiquity** | More users own secure hardware | Prioritize Ledger/Trezor integration |

### Long-Term (2032+)

| Trend | Impact on Arx Runa | Action |
|-------|-------------------|--------|
| **Personal AI agents** | Users need private storage for AI context, memories, conversations | "AI memory vault" — huge use case |
| **Digital identity wallets** | EU eIDAS 2.0, government-issued digital ID | Integration point — store credentials in Arx Runa |
| **Biometric normalization** | Face/fingerprint everywhere | Tier 2 becomes default expectation |
| **Zero-knowledge as table stakes** | What's "paranoid" today becomes standard | Arx Runa's architecture becomes mainstream requirement |

### The "AI Memory Vault" Opportunity

**Speculative but high-potential**:

As personal AI assistants become prevalent, users will want:
- Private storage for AI conversation history
- Encrypted context that AI can access but cloud can't
- "My AI knows me, but Big Tech doesn't"

Arx Runa could become the **private memory layer** for local AI:
- Store conversation logs, preferences, learned context
- AI runs locally (see: Oxide-Lab, 104 stars — Rust+Tauri local AI)
- Arx Runa provides encrypted persistence

---

## Feature Ideas for Mainstream Appeal

### 1. "Secure Drop" for Non-Technical Recipients

**Problem**: "I want to share encrypted files with my parents, but they won't install software."

**Solution**:
- Generate one-time encrypted link
- Recipient clicks link → browser-based decryption (WebCrypto)
- No account required for recipient
- Link expires after download or time limit

**Trade-off**: Browser-based decryption is less secure than native app, but dramatically increases accessibility.

### 2. "Camera Roll Shield" (Mobile)

**Problem**: "iCloud/Google Photos trains AI on my photos."

**Solution**:
- Mobile app auto-encrypts camera roll
- Replaces or supplements cloud photo backup
- Familiar UX: "like iCloud but private"

**Market validation**: Major pain point in 2025-2026 news cycle.

### 3. "Dead Man's Switch" / Digital Inheritance

**Problem**: "What happens to my encrypted data when I die?"

**Solution**:
- Designate trusted contacts
- After X days of inactivity, send recovery key shards (Shamir secret sharing)
- Combine K of N shards to recover vault

**Emotional appeal**: "Protect your family's memories and documents for generations."

### 4. Developer Mode: Encrypted .env / Secrets

**Problem**: Developers store secrets in plaintext .env files.

**Solution**:
- `arx-runa secrets set API_KEY=xxx`
- Encrypted storage, decrypted only in memory at runtime
- Git-safe (commit encrypted blob, not plaintext)
- Integration with CI/CD

**Target**: Developers are early adopters and word-of-mouth spreaders.

### 5. "Privacy Score" Dashboard

**Problem**: Users don't understand their security posture.

**Solution**:
- Dashboard showing:
  - Auth tier (and upgrade prompts)
  - Cloud provider trust level
  - Last backup verification
  - Recovery readiness
- Gamification: "Improve your score"

---

## Recommendation

Arx Runa has a defensible niche today (hardware-bound zero-knowledge + BYOC) and strong positioning for future trends (AI privacy backlash, post-quantum, personal AI agents).

**Key strategic moves**:
1. **Progressive security model** — lower entry barrier while keeping paranoid tier
2. **Auto-sync drop zone UI** — "Dropbox but private" experience
3. **Alternative auth factors** — biometric and NFC for mainstream, hardware wallet for crypto users
4. **Developer tools** — encrypted secrets management for word-of-mouth
5. **Mobile monetization** — Cryptomator-proven revenue model

The architecture being built today is ahead of the curve. What's "paranoid" in 2026 becomes "standard practice" by 2030.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|

---

## Open Questions

1. **Mobile priority**: Build mobile app (revenue) or focus on desktop excellence first?

2. **Biometric trust**: How much to trust OS biometric APIs? What about deepfake faces?

3. **Recovery without hardware**: If user loses USB key, can we offer any recovery? (Shamir shards? Recovery seed like crypto wallets?)

4. **Browser extension**: Worth building for web-based cloud consoles? (e.g., encrypt before uploading to Drive web UI)

5. **Pricing psychology**: Is paid mobile + free desktop the right model, or should desktop have a "supporter" tier?

6. **Post-quantum timeline**: When to add PQC algorithms? Too early = complexity. Too late = migration pain.

7. **NFC tag standard**: Define our own format or adopt existing (if any)?

8. **Passkey PRF**: Monitor WebAuthn Level 3 PRF extension adoption — could simplify Tier 2.

---

## Sources

### Primary Sources (Web Research)

| Source | Topic | URL |
|--------|-------|-----|
| **NIST** | Post-Quantum Cryptography Standards | [nist.gov/news-events/news/2024/08/nist-releases-first-3-finalized-post-quantum-encryption-standards](https://www.nist.gov/news-events/news/2024/08/nist-releases-first-3-finalized-post-quantum-encryption-standards) |
| **CISA** | Post-Quantum Cryptography Initiative | [cisa.gov/quantum](https://www.cisa.gov/quantum) |
| **FIDO Alliance** | Passkeys Overview & Adoption | [fidoalliance.org/passkeys](https://fidoalliance.org/passkeys/) |
| **Pew Research Center** | How Americans View Data Privacy (May 2023) | [pewresearch.org/internet/2023/10/18/how-americans-view-data-privacy](https://www.pewresearch.org/internet/2023/10/18/how-americans-view-data-privacy/) |
| **The Verge** | Apple Intelligence & Private Cloud Compute | [theverge.com/2024/6/10/24175405/wwdc-apple-ai-privacy-cloud-compute](https://www.theverge.com/2024/6/10/24175405/wwdc-apple-ai-privacy-cloud-compute) |
| **Apple** | Privacy Philosophy | [apple.com/privacy](https://www.apple.com/privacy/) |
| **Tresorit** | Security Features | [tresorit.com/security](https://tresorit.com/security) |
| **Internxt** | Privacy Philosophy | [internxt.com/privacy](https://internxt.com/privacy) |

### GitHub Research

| Repository | Stars | Relevance |
|------------|-------|-----------|
| **cryptomator/cryptomator** | 14.8k | Primary competitor, encryption approach |
| **teamhanko/hanko** | 8.9k | FIDO2/WebAuthn authentication patterns |
| **Peergos/Peergos** | 2.4k | Post-quantum ready, P2P encrypted storage |
| **yackermann/awesome-webauthn** | 1.8k | WebAuthn/Passkey resources |
| **floating/frame** | 1.2k | Hardware wallet (Ledger/Trezor) integration patterns |
| **internxt/drive-web** | 146 | Zero-knowledge cloud storage implementation |
| **ANSSI-FR/MLA** | 367 | Rust archive format with PQC |

### Key Statistics Summary

| Metric | Value | Source |
|--------|-------|--------|
| Americans concerned about company data use | ~80% | Pew 2023 |
| Americans concerned about government data use | 71% | Pew 2023 |
| Americans who don't understand company data practices | 67% | Pew 2023 |
| Americans who support more data regulation | 72% | Pew 2023 |
| Little/no trust in social media CEOs | 77% | Pew 2023 |
| Little/no trust in companies' AI decisions | 70% | Pew 2023 |
| Expect AI to lead to uncomfortable data use | 81% | Pew 2023 |
| People with passkeys enabled (at least one account) | 53% | FIDO Alliance |
| Passkey sign-in success improvement over passwords | 20% | FIDO Alliance |
| Cryptomator GitHub stars | 14.8k | GitHub |

