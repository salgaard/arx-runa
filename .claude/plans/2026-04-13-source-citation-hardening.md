---
title: "Research source hardening and standards freshness"
created: "2026-04-13T16:18:49Z"
status: implemented
roadmap-phase: null
sub-phase: null
design-document: null
sub-phase-roadmap: null
test-agent-required: false
governance-sync-required: true
tags: [docs, research, sources, standards, quality]
---

# Plan: Research source hardening and standards freshness

## 1. Goal

Raise citation quality across research and related architecture documents by fixing broken links, replacing unacceptable low-scientific references, refreshing superseded standards, and surfacing any design-decision conflicts caused by newer standards.

## 2. Context

- Scope root: `C:\Users\chris\source\repos\arx-runa\docs\research\`.
- Current source-table baseline (from existing audit artifacts):
  - 147 source rows, 137 unique URLs across 11 research docs with `## Sources`.
  - 123 URLs return 2xx/3xx from the current audit environment.
  - 14 unique URLs are currently problematic (status 0/4xx/403).
- Source-quality hotspot files (weak tertiary/news/blog usage):
  - `C:\Users\chris\source\repos\arx-runa\docs\research\compression-and-cloud-cost.md` (10/17 weak)
  - `C:\Users\chris\source\repos\arx-runa\docs\research\padding-overhead-reduction.md` (4/25 weak)
  - `C:\Users\chris\source\repos\arx-runa\docs\research\bin-packing.md` (3/10 weak)
  - `C:\Users\chris\source\repos\arx-runa\docs\research\market-and-future-directions.md` (2/8 weak)
- Outlier:
  - `C:\Users\chris\source\repos\arx-runa\docs\research\rust-programming-language.md` has no `## Sources` section and does not follow the research-document structure required by `.claude/rules/research.md`.
- Design docs to include in standards-freshness sweep:
  - `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\authentication-and-session-management\design.md`
  - `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\cloud-synchronisation\design.md`
  - `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\cryptographic-primitives\design.md`
  - `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\file-sharing\design.md`
- Existing rules to enforce:
  - `.claude/rules/research.md` requires proper `Sources` tables and standards-grade references for security/crypto claims.
  - Canonical design updates must be propagated to dependent sub-phases/diagrams/rules/instructions per repository governance.

## 3. Design Concerns / Open Questions

### Concern 1 — Conflict resolution when newer standards disagree with canonical design
- **Source**: User requirement to revisit design decisions when newer standards diverge.
- **Impact**: Potential contract-surface changes in canonical design docs, with downstream propagation.
- **Classification**: Non-blocking.
- **Resolution**: During implementation, prepare an explicit decision package for each standards conflict and pause for user approval before changing canonical design docs. After approval, apply canonical edits and propagation in one cohesive change set.
- **Documentation sync required on implementation**: `docs/architecture/designs/**/design.md`, matching `sub-phases/**`, and related diagram files where behavior/contracts change (after approval).

### Concern 2 — `crates.io` links are unstable in automated validation
- **Source**: Multiple `crates.io` URLs return 404 in this environment while corresponding crates exist on `docs.rs`.
- **Impact**: Source validation appears failed and references may look broken to reviewers.
- **Classification**: Non-blocking.
- **Resolution**: Prefer `docs.rs` API/docs pages (or canonical upstream repository links) for crate citations when `crates.io` is not reliably fetchable.
- **Documentation sync required on implementation**: Affected research docs only.

### Concern 3 — Tertiary references are mixed with normative claims in selected docs
- **Source**: Current weak-source concentration in compression/bin-packing/padding/market documents.
- **Impact**: Reduced scientific confidence for core claims.
- **Classification**: Non-blocking.
- **Resolution**: Apply claim-type rubric: tertiary sources may remain only for market sentiment/background context; replace them when used to justify cryptographic, security, performance, or architectural decisions.
- **Documentation sync required on implementation**: Affected research docs only.

### Concern 4 — `rust-programming-language.md` is structurally non-compliant
- **Source**: Missing required research sections and no `README` entry.
- **Impact**: Governance inconsistency and weak citation posture.
- **Classification**: Non-blocking.
- **Resolution**: Bring the file into compliance (or explicitly archive/de-scope it from `docs/research/`) in the same source-hardening implementation.
- **Documentation sync required on implementation**: `docs/research/rust-programming-language.md` and `docs/research/README.md`.

## 4. Assumptions

1. Old foundational papers are acceptable when still canonical and not contradicted by newer work; they should be paired with newer evidence when practical.
2. Superseded standards (withdrawn/replaced versions) should be updated to current revisions unless there is an explicit compatibility rationale.
3. 403 responses from ACM/ScienceDirect are treated as access restrictions, not automatically as invalid sources; if used, prefer adding an accessible primary or preprint companion citation.
4. If a broken source has no credible replacement, implementation will keep the claim but explicitly mark it unresolved and report it back.
5. Design-doc conflicts discovered during freshness review may require edits beyond `docs/research/`; those edits are in scope.
6. Canonical design conflicts are decision-gated: no canonical design edit is applied until explicit user approval is received.

## 5. Approach

### Step 5.1 — Build a complete citation inventory and claim map

1. Extract all sources from `## Sources` tables in:
   - `C:\Users\chris\source\repos\arx-runa\docs\research\*.md`
2. Extract inline citations in outlier files (currently `rust-programming-language.md`) and map each source to claim type:
   - cryptographic/security
   - performance/cost
   - market/sentiment
   - implementation anecdote/background
3. Produce an implementation-time inventory artifact under session workspace for traceability.

### Step 5.2 — Repair broken sources first (hard failures)

For each broken URL:
1. Search for a replacement source in priority order:
   1. standards body / official spec (NIST/RFC/etc.)
   2. peer-reviewed venue (ACM/IEEE/USENIX/IACR)
   3. official vendor whitepaper/docs page
2. Replace source rows and any dependent in-text claims in-place.
3. If no acceptable replacement exists, leave the original claim explicitly marked unresolved and add it to final unresolved report output.

Known starting set includes:
- `https://developers.mitid.dk/`
- `https://bitwarden.com/images/resources/security-white-paper-download.pdf`
- `https://www.helpnetsecurity.com/2016/08/11/crime-time-breach-and-heist-a-brief-history-of-compression-oracle-attacks-on-https/`
- `https://crates.io/crates/{bip39,unicode-normalization,hpke,aes-gcm-siv,sharks,vsss-rs,slip39}`

### Step 5.3 — Apply low-scientific-source replacement rubric

1. Evaluate weak references in:
   - `C:\Users\chris\source\repos\arx-runa\docs\research\compression-and-cloud-cost.md`
   - `C:\Users\chris\source\repos\arx-runa\docs\research\padding-overhead-reduction.md`
   - `C:\Users\chris\source\repos\arx-runa\docs\research\bin-packing.md`
   - `C:\Users\chris\source\repos\arx-runa\docs\research\market-and-future-directions.md`
2. Keep tertiary sources only when the usage is clearly non-normative (market sentiment/background).
3. Replace tertiary sources that underpin technical/security/performance conclusions with stronger primary sources.

### Step 5.4 — Run standards freshness and supersession audit (research + design)

1. Extract all standards/paper references from:
   - `C:\Users\chris\source\repos\arx-runa\docs\research\*.md`
   - `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\**\*.md`
2. For each standard, verify:
   - current version status (active/superseded/withdrawn)
   - whether current recommendation materially differs from referenced guidance
3. Update citations and wording where superseded standards are currently used.
4. Preserve foundational historical papers where appropriate, but add modern corroboration when available.

### Step 5.5 — Reconcile design decisions when newer standards differ

1. For each detected conflict between current design text and updated standards guidance:
   - build a decision packet (current design position, new-standard guidance, options, recommendation, impact)
   - pause and request explicit user decision before canonical edits
2. After user approval, update canonical design at source (`design.md` contract/rationale sections) and document the decision rationale.
3. Propagate approved canonical changes to dependent artefacts:
   - sub-phases under `docs/architecture/designs/<design-name>/sub-phases/`
   - diagrams under `docs/architecture/designs/**/diagrams/`
   - any related operational guidance if behavior/constraints changed.

### Step 5.6 — Bring research docs into structural compliance

1. Ensure all research files under `docs/research/` satisfy `.claude/rules/research.md`:
   - required sections order
   - valid `Sources` table format
   - `README` entry coverage
2. Specifically resolve `rust-programming-language.md` (normalize to required format or explicitly move/de-scope from this folder with consistent documentation updates).

### Step 5.7 — Final verification and reporting

1. Re-run link/status checks after edits.
2. Re-run weak-source classification and confirm reductions in technical claim areas.
3. Produce a final unresolved-items list for any claim where no better source could be found.

## 6. Security implications

### a. Expected sensitive path set

None anticipated. This is a documentation and standards-governance pass.

### b. Invoke security-reviewer agent?

NO.

Rationale: planned edits target `docs/research/**`, `docs/architecture/**`, and optionally documentation governance files; no `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` code changes are expected.

### c. What reviewer should check

Not applicable.

## 7. Execution and testing strategy

**Test scope:**
- [x] Documentation/link integrity checks
- [x] Source-quality classification checks
- [x] Standards supersession checks for cited standards
- [ ] Unit tests
- [ ] Adversarial tests
- [ ] Property-based tests
- [ ] Integration tests

**Invoke test-writer agent?**
- [ ] YES
- [x] NO — Reason: docs-focused implementation; no code-path behavior change.

**Acceptance criteria:**
1. No unresolved hard-broken links remain without explicit unresolved reporting.
2. Security/cryptographic claims are backed by standards-grade or peer-reviewed sources.
3. Superseded standards are replaced or explicitly justified.
4. If a design-guidance change is approved due standards refresh, dependent docs are updated in the same implementation.

## 8. Documentation impact

Primary files likely to change:
- `C:\Users\chris\source\repos\arx-runa\docs\research\*.md` (source rows and in-text claim support)
- `C:\Users\chris\source\repos\arx-runa\docs\research\README.md`
- `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\**\design.md` (if standards conflict requires decision updates)
- `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\**\sub-phases\*.md` and `**\diagrams\*.md` where canonical decisions propagate.

## 9. Governance sync actions (pre-implementation)

### Action GOV-1
- **Reason / linked concern**: Concern 3 (tertiary source usage policy needs explicit guardrails).
- **Target files**: `C:\Users\chris\source\repos\arx-runa\.claude\rules\research.md`
- **Required edit**: Add explicit source-quality rubric clarifying when tertiary/blog/news sources are acceptable vs unacceptable for normative technical claims.
- **Verification**: Re-read `research.md` to ensure rubric aligns with requested policy and does not conflict with existing mandatory sections.

### Action GOV-2
- **Reason / linked concern**: Rule mirror consistency after GOV-1.
- **Target files**: `C:\Users\chris\source\repos\arx-runa\.github\instructions\research.instructions.md` (generated via sync)
- **Required edit**: Run `/copilot-sync` after rule edits.
- **Verification**: Confirm mirror reflects the new rubric text and no drift remains.

## 10. Handoff Notes for Implementer

Work from `C:\Users\chris\source\repos\arx-runa`. Execute in this order: inventory -> broken-link replacement -> weak-source hardening -> standards freshness (research + design) -> decision packets for design conflicts -> wait for explicit decision -> approved canonical propagation -> final verification report. Treat this plan as self-contained; no external conversation context is required.

## 11. Implementation Log

- **Date**: 2026-04-13T17:08:43Z
- **Branch**: development

### Agent evidence

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| GOV-1 | invoking-agent | N/A | Added explicit source-quality rubric to `.claude/rules/research.md`. |
| GOV-2 | invoking-agent (`/copilot-sync`) | N/A | Synced `.github/instructions/*.instructions.md` from `.claude/rules/*.md`; verification reported all in sync. |
| 5.1 — citation inventory and claim map | invoking-agent | N/A | Produced inventory artifact: `C:\Users\chris\.copilot\session-state\9c3543ac-fc0a-497d-af4a-b885318a8158\files\citation-inventory-2026-04-13.json`. |
| 5.2 — broken-source repair | invoking-agent | N/A | Replaced hard-failure URLs (MitID dev link, Bitwarden PDF, HelpNetSecurity entry, and unstable crates.io citations). |
| 5.3 — weak-source hardening | invoking-agent | N/A | Replaced tertiary technical citations in hotspot docs; post-audit hotspot weak counts reduced to zero. |
| 5.4 — standards freshness audit | invoking-agent | N/A | Updated superseded references to current revisions and generated standards audit artifact with zero superseded refs remaining. |
| 5.5 — design conflict reconciliation | invoking-agent | N/A | No canonical design conflicts detected; no decision packet required. |
| 5.6 — structural compliance | invoking-agent | N/A | Normalized `rust-programming-language.md` to required research structure and added `docs/research/README.md` entry. |
| 5.7 — final verification and reporting | invoking-agent | N/A | Final link audit: `broken=0`; unresolved-items artifact created with `None`. |

### Files changed

- `.claude/plans/2026-04-13-source-citation-hardening.md`
- `.claude/rules/research.md`
- `.github/instructions/auth.instructions.md`
- `.github/instructions/crypto.instructions.md`
- `.github/instructions/leptos.instructions.md`
- `.github/instructions/memory-protection.instructions.md`
- `.github/instructions/mermaid.instructions.md`
- `.github/instructions/research.instructions.md`
- `.github/instructions/rust.instructions.md`
- `.github/instructions/storage.instructions.md`
- `.github/instructions/tauri.instructions.md`
- `docs/research/README.md`
- `docs/research/authentication-and-session-management.md`
- `docs/research/bin-packing.md`
- `docs/research/compression-and-cloud-cost.md`
- `docs/research/cryptographic-primitive-rationale.md`
- `docs/research/file-sharing-cryptography.md`
- `docs/research/market-and-future-directions.md`
- `docs/research/mfa-for-vault-authentication.md`
- `docs/research/mobile-photo-backup.md`
- `docs/research/padding-overhead-reduction.md`
- `docs/research/password-and-key-recovery.md`
- `docs/research/rust-programming-language.md`

### Test results

- `cargo test --workspace`: passed (`72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` in `arx_runa_tauri_lib`; remaining crates had `0` tests and passed).

### Clippy results

- `cargo clippy --workspace -- -D warnings`: clean.

### Security review

- N/A. Plan specified `Invoke security-reviewer agent? NO`, and no files under `src-tauri/src/{crypto,auth,storage}/` were modified.

### Governance sync

- Actions executed: 2 (`GOV-1`, `GOV-2`).
- Files updated: `.claude/rules/research.md` and mirrored `.github/instructions/*.instructions.md` set.
- `/copilot-sync` outcome: completed and verified in-sync state after regeneration.

### Sub-phase decisions sync

- N/A (non-sub-phase plan).

### Deviations from plan

- No blocking deviations.
- Non-blocking adjustment: `docs/research/rust-programming-language.md` was fully normalized into a concise, standards-backed compliant structure instead of incrementally patching the prior non-compliant draft.

### Documentation flagged

- `C:\Users\chris\source\repos\arx-runa\docs\research\*.md` (source rows and in-text claim support)
- `C:\Users\chris\source\repos\arx-runa\docs\research\README.md`
- `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\**\design.md` (if standards conflict requires decision updates)
- `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\**\sub-phases\*.md` and `**\diagrams\*.md` where canonical decisions propagate.

