# `/implement-plan` — Plan-Driven Implementation Command

Implement the saved plan: $ARGUMENTS

---

## Design Principles

- **Plan is source of truth.** Unexpected reality is handled as a plan deviation, not improvised execution.
- **Hard-gated execution.** Gate failures halt execution; unattended operation must not silently bypass safeguards.
- **Thin orchestrator, explicit specialists.** The invoking agent sequences and verifies; designated agents own review, classification, and solution semantics.
- **Structured context handoff.** Once digest artifacts exist, downstream agents consume structured contracts rather than raw narrative.
- **Scope-driven reviewers.** Which reviewers run is determined by actual changed files — not plan frontmatter flags.

---

## Agent Roster

| Agent | Role | Output |
|---|---|---|
| `plan-context-builder` | Plan context extraction | `PLAN_DIGEST` |
| `rules-extractor` | Rule anchor extraction | `RULES_INDEX` |
| `design-extractor` | Design invariant extraction | `DESIGN_INDEX` |
| `shard-planner` | Scope-to-shard mapping and digest summaries | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `rust-implementer` | Delegated code and design-doc implementation | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | Rust quality review (all Rust-touching plans) | Structured findings |
| `architecture-reviewer` | Architecture integrity review (all Rust-touching plans) | Structured findings |
| `security-reviewer` | Security review (security-path-touching plans) | Structured findings |
| `cross-shard-reviewer` | Cross-shard contradiction detection | Structured findings |
| `finding-classifier` | Findings disposition/confidence quality gate | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Findings-to-fix synthesis and design challenge evaluation | `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` |
| `test-writer` | Test expansion (all code-changing plans) | Test additions/updates |

**Execution contract (hard):** The invoking agent owns implementation end-to-end. It may delegate coding steps to sub-agents but retains orchestration, verification, and final accountability.

**Orchestrator delegation contract (hard):** Keep orchestration thin. Delegate reviewer semantics, finding classification, and solution synthesis to designated agents. When structured artifacts exist (`PLAN_DIGEST`, `RULES_INDEX`, `DESIGN_INDEX`, shard `DIGEST_SLICE`, normalized findings), pass those artifacts — not raw prose.

## Structured contract ownership (hard)

| Artifact | Owner |
|---|---|
| `PLAN_DIGEST` | `.claude/agents/plan-context-builder.md` |
| `RULES_INDEX` | `.claude/agents/rules-extractor.md` |
| `DESIGN_INDEX` | `.claude/agents/design-extractor.md` |
| `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` | `.claude/agents/shard-planner.md` |
| `CLASSIFIED_FINDINGS` | `.claude/agents/finding-classifier.md` |
| `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` | `.claude/agents/problem-solver.md` |
| `IMPLEMENTATION_RESULT` | `.claude/agents/rust-implementer.md` |

This command owns orchestration and gates. Producer schema details live in agent contracts.

---

## Step 1 — Resolve the plan file

Locate the plan from $ARGUMENTS:

- Full filename (e.g., `phase-1-cryptographic-primitives.md`) → read `.claude/plans/$ARGUMENTS`
- Filename without `.md` → append and retry
- `latest` → most recently created file in `.claude/plans/` by `created` frontmatter, excluding `_template.md` and `status: blocked` plans
- Empty or no match → list all plans (excluding `_template.md`) with title, status, and created date; ask the user to choose

---

## Step 2 — Pre-flight checks

Any failure halts execution. Do not attempt to auto-fix these conditions.

1. **Working directory:** confirm cwd is the repo root (directory containing `CLAUDE.md`). If not, halt with actual and expected paths.
2. **Git state:** run `git status --porcelain` and `git branch --show-current`. Record branch for the Implementation Log. If the working tree is dirty, display the dirty files and halt — re-run with `--force-dirty` to override.
3. **Baseline build:** run `cargo check --workspace`. If it fails, halt and report errors — the baseline must be green before implementation begins.

---

## Step 3 — Validate the plan and enforce gates

Any hard-gate failure stops execution before implementation begins.

1. Read the plan file and parse its YAML frontmatter.

2. **Status gate (hard):**
   - `approved` → proceed.
   - `draft` → halt: "Plan is still a draft. Set `status: approved` before running `/implement-plan`."
   - `blocked` → halt: "Plan is blocked." Display **Design Concerns / Open Questions**.
   - `in-progress` → halt: "Plan is already in progress. Reset `status: approved` to re-run."
   - `implemented` / `superseded` → halt: "Plan is already `<status>`. Reset to `approved` if intentional."

3. **Blocking-concerns gate (hard):** scan **Design Concerns / Open Questions** for any **Blocking** entry. If found, halt and display each — regardless of `status`.

4. **Display Handoff Notes:** output the plan's **Handoff Notes for Implementer** section verbatim before continuing.

5. **Verify Assumptions:** for each Assumptions entry, verify it holds against the current repo state. If any assumption is now false, halt with: which assumption failed, the current state, and a suggested resolution.

6. **Sub-phase detection:** if `sub-phase` is present in frontmatter, enable sub-phase-aware implementation.

7. **Sub-phase prerequisites (sub-phase plans only):** read the sub-phase roadmap from `sub-phase-roadmap`. Extract the sub-phase section. If any prerequisite sub-phase is missing or not marked implemented, halt with the list.

8. **Governance-sync gate (hard, pre-implementation):**
   - Require frontmatter `governance-sync-required`.
   - If `true` but **Governance sync actions** section is missing or says "None" → halt.
   - If `false` but section lists actions → halt.
   - If actions are listed: execute them in order before proceeding. Apply `.claude/rules/*.md` and `.claude/agents/*.md` updates as declared. If any action touches `.claude/rules/*.md`, run `/copilot-sync` once after all rule edits. Re-read each target file and confirm the declared update is present. If any action cannot be completed or verified, invoke Plan-deviation protocol and halt.

### Post-gate setup

After all gates pass (before updating plan status):

1. **Build structured context artifacts** — spawn in parallel:
   - `plan-context-builder` → `PLAN_DIGEST`
   - `rules-extractor` → `RULES_INDEX`
   - `design-extractor` → `DESIGN_INDEX`
   - `shard-planner` (receives resolved Rust file list from plan Section 6a) → `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`

   Required consumer fields:
   - `PLAN_DIGEST`: `highest_implemented_phase`, `in_progress_phases`, `deferred_phases`, `plans[]`, `handoffs[]`
   - `RULES_INDEX`: `rules[].{id, source_file, anchor, verbatim, scope, severity_if_violated}`
   - `DESIGN_INDEX`: `invariants[].{id, source_file, anchor, verbatim, scope, challenged}`
   - `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`: per `.claude/agents/shard-planner.md`

   Do not pass full raw plan or design prose to reviewer or solver agents once these structures exist. If any artifact fails to build, halt and report which gatherer failed.

2. **Update `status` to `in-progress`** in the plan frontmatter. (Intentionally after artifact build — a failed build leaves plan status unchanged.)

---

## Step 4 — Implement

Follow the **Approach** section of the plan step by step, in order.

### Delegation model

`rust-implementer` is the default executor for all coding steps. The orchestrator retains orchestration, gate enforcement, review invocation, and verification throughout — it never delegates those.

- Delegate each coding-focused Approach step to `rust-implementer` with full step context, expected outputs, and constraints. Require `IMPLEMENTATION_RESULT`.
- If `rust-implementer` returns `BLOCKED` on an Approach step (not a finding), the orchestrator implements that step directly as fallback. Record the fallback in the Implementation Log.
- `finding-classifier` and `problem-solver` govern remediation structure; `rust-implementer` executes approved `SOLUTION_PACK` payloads including any design-doc updates.
- `rust-reviewer`, `security-reviewer`, and `architecture-reviewer` provide findings only; they do not implement.
- If any required delegation contract cannot be satisfied and direct fallback is also infeasible, invoke Plan-deviation protocol and halt.

1. Delegate coding-focused Approach steps to `rust-implementer` with full step context, expected outputs, and constraints.
2. Execute every Approach step as written — via delegation or direct fallback.
3. **No speculative fallback:** if a step cannot be completed as written by either path, follow Plan-deviation protocol and halt.
4. After each Approach step, run `cargo check --workspace` as a fast fail-check. Fix compile errors before moving to the next step.

### Review invocation (scope-driven)

Reviewers are invoked based on which files were actually changed — not plan flags. Read the plan's **Section 6 (Review focus areas)** for guidance on what each reviewer should check.

6. **Rust quality review:** if any `src-tauri/**/*.rs` files were changed, invoke `rust-reviewer` on touched Rust files. Pass the `DIGEST_SLICE_<shard_id>` for each touched shard; do not pass full `RULES_INDEX` or `DESIGN_INDEX`. If no Rust files changed, skip and record in Implementation Log.

7. **Security review:** if any files under `src-tauri/src/{crypto,auth,storage}/` were changed, invoke `security-reviewer`. Pass relevant `DIGEST_SLICE` and the security concerns from plan Section 6b. If no security-path files changed, skip and record in Implementation Log.
   - **Drift check (always runs):** compare files actually modified under `src-tauri/src/{crypto,auth,storage}/` against the plan's **Section 6b**. If any sensitive file was touched that the plan did not anticipate, invoke Plan-deviation protocol and halt.

8. **Architecture review:** if any `src-tauri/**/*.rs` files were changed, invoke `architecture-reviewer`. Pass relevant `DIGEST_SLICE`. If no Rust files changed, skip and record in Implementation Log.

### Findings remediation loop

9. **Finding canonicalization:** before passing findings to `finding-classifier`, assign stable `CF-NNN` IDs. Map each raw reviewer finding (`RR-NNN`, `AR-NNN`, `SR-NNN`) to a unique `CF-NNN` in arrival order (rust-reviewer first, then architecture-reviewer, then security-reviewer). Preserve original IDs in a `source_id` field. This mapping is fixed for the entire remediation loop — do not re-assign IDs across cycles.

10. **Severity normalization (required before classification):** map security-reviewer severities to the common scale — `CRITICAL` stays `CRITICAL`, `WARNING` → `HIGH`, `NOTE` → `MEDIUM`. Rust and architecture findings use `HIGH|MEDIUM|LOW` directly.

11. **Finding classification:** invoke `finding-classifier` with canonicalized normalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Require `CLASSIFIED_FINDINGS` per `.claude/agents/finding-classifier.md`.

12. **Problem-solver invocation:** if `ACTIONABLE_NOW` is empty, continue to Testing. Otherwise:
    - Pass the full `design_challenge_ledger` from `CLASSIFIED_FINDINGS` to `problem-solver` as `design_challenge_entries`. Problem-solver evaluates each challenge and decides whether to accept or reject it — its `SOLUTION_PACK` includes rationale and, for accepted challenges, a design-doc update step.
    - Group actionable findings before solver launch:
      - One isolated solver invocation per CRITICAL finding
      - HIGH findings: one per finding or grouped by root cause at orchestrator discretion
      - MEDIUM findings grouped by shard (max 10 per invocation)
      - One LOW batch
    - Invoke `problem-solver` per group with scoped files, `DIGEST_SLICE`, and `design_challenge_entries`. Require one of `SOLUTION_PACK`, `NO_ACTIONABLE_FIXES`, or `BLOCKED_SOLUTIONS` per `.claude/agents/problem-solver.md`.
    - If `BLOCKED_SOLUTIONS` returned: invoke Plan-deviation protocol and halt.
    - If `NO_ACTIONABLE_FIXES` returned: continue.
    - Invoke `rust-implementer` with `SOLUTION_PACK`. Solutions may include design-doc update steps — `rust-implementer` implements these alongside code changes. Require `IMPLEMENTATION_RESULT`. If any item returns `BLOCKED`, the orchestrator implements that item directly; if that is also infeasible, invoke Plan-deviation protocol and halt.
    - **Cross-shard consistency pass:** after each remediation cycle, when two or more shards had changed files, invoke `cross-shard-reviewer` once with per-shard finding records + `SHARD_DIGEST_SUMMARY[]`. Use stable labels (`remediation-cycle-1`, `remediation-cycle-2`, ...). CRITICAL/HIGH cross-shard findings feed back into the next `finding-classifier` invocation.
    - **Re-review cycle:** after each remediation, re-run enabled reviewers on changed files. Repeat steps 9–12.
    - **Acceptance thresholds:**
      - All CRITICAL and HIGH findings → must remediate before completion.
      - MEDIUM and LOW findings → record in Implementation Log with rationale when deferred.
    - **Max cycles:** 8. If required thresholds are not met after cycle 8, invoke Plan-deviation protocol and halt.

### Plan-deviation protocol

If any Approach step cannot be executed as written, or a governance sync action cannot be completed as specified:

1. Revert or stash any partial work for that step so the repo is in a consistent state.
2. Append a `## Plan Deviation` section to the plan file:
   - **Step** — which step hit the problem
   - **Expected** — what the plan said to do
   - **Actual** — what is true in the repo
   - **Suggested resolution** — one or two options
3. Update `status: blocked`.
4. Halt and report the deviation. Do not proceed with subsequent steps.

### Testing

Invoke `test-writer` with the specific focus from plan Section 6d. Run `cargo test` after completion and report results.

If plan Section 6d says no tests are needed and the plan made no Rust changes: skip `test-writer` and record rationale in the Implementation Log.

### Sub-phase implementation decisions sync (mandatory for sub-phase plans)

1. Locate the sub-phase document path (prefer explicit path in plan body).
2. Ensure the sub-phase doc has an `## Implementation Decisions` section; create if missing.
3. Append or update bullets for decisions made during implementation that were previously optional or ambiguous.
4. Keep entries concise: **decision + rationale + deferred follow-up if any**.
5. Required before Step 6 status can be set to `implemented`.

### Validation checkpoint

If sub-phase plan: after implementation, read the Validation checkpoint from the sub-phase roadmap. Run automated tests. Display manual verification steps and acceptance criteria to the user — do not mark `implemented` if the automated portion fails.

---

## Step 5 — Verify

1. Run `cargo test` (full workspace). Fix related failures before marking implemented. Record pre-existing unrelated failures in the Implementation Log.
2. Run `cargo clippy --workspace -- -D warnings`. Fix new warnings introduced by this implementation; note pre-existing warnings in the Implementation Log.

---

## Step 6 — Mark complete and report

1. **Sub-phase decision-sync gate (hard):** if sub-phase plan, verify the target sub-phase doc has an `## Implementation Decisions` section reflecting this run. If missing or stale, return to the decision-sync step in Step 4.

2. **Design-doc sync gate (hard):** if `SOLUTION_PACK` contained any accepted design challenges, verify that each referenced design document under `docs/architecture/designs/` was updated to reflect the accepted change. If any accepted challenge has no corresponding design-doc update, halt and require the update before proceeding.

3. Update `status: implemented` in the plan frontmatter.

4. Append an **Implementation Log** section to the plan file:
   - **Date** — ISO 8601 datetime
   - **Branch** — recorded in Step 2
   - **Execution mode** — `rust-implementer` (delegated, default) or `orchestrator` (direct fallback) per step; note which steps fell back and why
   - **Agent evidence** — table: `Approach step | Agent | Agent ID | Outcome`
   - **Files changed** — list of modified/created files (including any updated design docs)
   - **Test results** — `cargo test` summary
   - **Clippy results** — clean / warnings introduced / pre-existing noted
   - **Rust review** — `rust-reviewer` findings summary or "Skipped — no Rust files changed"
   - **Architecture review** — `architecture-reviewer` findings summary or "Skipped — no Rust files changed"
   - **Security review** — `security-reviewer` findings summary or "Skipped — no security-path files changed"
   - **Cross-shard review** — invocation count + any findings or "N/A"
   - **Findings quality gate** — counts by disposition
   - **Design challenge outcomes** — for each challenge: finding ID, challenge summary, decision (accepted/rejected), rationale, design doc updated (path or "N/A")
   - **Governance sync** — action count, files updated, `/copilot-sync` outcome when applicable
   - **Sub-phase decisions sync** — target doc path + decisions added/updated (or "N/A")
   - **Deviations from plan** — small adjustments (large deviations should have halted at Step 4)
   - **Documentation flagged** — verbatim list from the plan's **Documentation impact** section

5. **Do not commit, push, or open a pull request.** Leave the working tree dirty.

6. **Report to the user:**

**Sub-phase plan:**
```
✓ Phase [X.Y] implementation complete — status: implemented
✓ Branch: [branch]
✓ Execution mode: [rust-implementer delegated | orchestrator fallback steps noted]
✓ Agent evidence: [summary]
✓ Rust review: [Skipped | findings summary]
✓ Architecture review: [Skipped | findings summary]
✓ Security review: [Skipped | findings summary]
✓ Cross-shard review: [N/A | invocation count + findings summary]
✓ Findings quality gate: [counts by disposition]
✓ Design challenge outcomes: [None | accepted/rejected summary with doc paths]
✓ Solution synthesis: [N/A | problem-solver summary]
✓ Tests: [summary]
✓ Clippy: [clean | N warnings]
→ Validation checkpoint (manual): [checkpoint description from sub-roadmap]
→ Acceptance criteria (manual): [list from sub-roadmap]
→ Files changed: [list]
→ Governance sync: [summary]
→ Sub-phase decisions sync: [doc path + decisions count]
→ Documentation flagged: [list from Documentation impact]
→ Next sub-phase: [X.Y+1 title, or "end of roadmap"]
```

**Full-phase or ad-hoc plan:** report implemented work, branch, agent evidence summary, review summaries, cross-shard summary, findings quality gate counts, design challenge outcomes, problem-solver summary, test results, clippy results, governance-sync summary, files changed, and the Documentation impact list.

---

## Guardrails

- Preserve hard-gate semantics in Step 3; do not silently downgrade failures.
- Do not skip the design-doc sync gate in Step 6 when accepted challenges exist.
- Do not broaden implementation scope outside the approved plan without triggering Plan-deviation protocol.
- Do not auto-chain `/review-only` and `/implement-review`; this command is a separate entrypoint.
- Do not commit, push, or open pull requests.
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` (IDs only) — never full `DIGEST_SLICE` content.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| Plan file cannot be resolved | Halt and report candidate plan files |
| Any Step 3 gate fails | Halt before implementation begins |
| Structured context artifact build fails | Halt before updating plan status |
| Governance sync action fails verification | Plan-deviation protocol, then halt |
| `rust-implementer` BLOCKED and orchestrator direct fallback also infeasible | Plan-deviation protocol, then halt |
| `BLOCKED_SOLUTIONS` returned during required remediation | Plan-deviation protocol, then halt |
| Required findings thresholds not met after cycle 8 | Plan-deviation protocol, then halt |
| Sensitive path drift detected outside Section 6b | Plan-deviation protocol, then halt |
| Accepted design challenge missing design-doc update at Step 6 | Halt — require design-doc update before marking implemented |
