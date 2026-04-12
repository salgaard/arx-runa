---
title: "Phase 0.3 — Frontend Build Pipeline and Verification"
created: "2026-04-13T00:00:00Z"
status: implemented
roadmap-phase: 0
sub-phase: "0.3"
design-document: "docs/architecture/designs/project-scaffolding/design.md"
sub-phase-roadmap: "docs/architecture/designs/project-scaffolding/sub-phases/roadmap.md"
tags: [scaffolding, frontend, trunk, tailwind, phase-0]
---

# Phase 0.3 — Frontend Build Pipeline and Verification

## 1. Goal

Wire the Trunk + Tailwind CSS v4 build pipeline into the existing Tauri v2 + Leptos 0.8 scaffold so that `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test --all-targets`, `cargo build --release`, and `cargo tauri dev` all succeed and the compiled frontend CSS contains the Arx Runa brand token palette (iron, stone, steel, rune, bone).

## 2. Context

**Roadmap**: Phase 0 — Project Scaffolding (`C:\Users\chris\source\repos\arx-runa\docs\roadmap.md` lines 35–41). Final sub-phase — on completion, Phase 0 is marked complete and Phase 1 (Cryptographic Primitives) becomes unblocked.

**Sub-phase dependencies**: Phase 0.3 depends strictly on Phase 0.2 (Dependencies and module skeleton), which is already implemented (commits `835eb54`, `61cc70a`, `af66df8`, `ab1759e`, `c1e1f3e`). Verified:

- `C:\Users\chris\source\repos\arx-runa\Cargo.toml` is a package+workspace manifest with `src-tauri` as a member and all five frontend crates declared (lines 1–23).
- `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml` contains all Phase 1–6 crate dependencies (crypto, storage, dev-deps) with the versions from the design.
- All six backend module skeletons exist under `src-tauri\src\` (`crypto`, `auth`, `storage`, `sync`, `memory`, `ui`).
- `C:\Users\chris\source\repos\arx-runa\src\main.rs` and `src\app.rs` contain a bare Leptos entrypoint rendering `<h1>"Hello Arx Runa"</h1>` inside a `<main class="container">`.

**Current build pipeline state** (partially scaffolded by `cargo create-tauri-app`, must be converted to the design's spec):

- `C:\Users\chris\source\repos\arx-runa\Trunk.toml` already exists (10 lines) with `[build]`, `[watch]`, and `[serve]` sections **but no `[[hooks]]` block** — the Tailwind pre-build hook is missing.
- `C:\Users\chris\source\repos\arx-runa\index.html` already exists with a `<link data-trunk rel="rust" data-wasm-opt="z" />` tag — good — but references `styles.css`, not `output.css`.
- `C:\Users\chris\source\repos\arx-runa\styles.css` exists and carries the `cargo create-tauri-app` default styles (lines 1–113). It is **not** the Tailwind input file.
- `C:\Users\chris\source\repos\arx-runa\input.css` does **not** exist.
- `C:\Users\chris\source\repos\arx-runa\package.json` does **not** exist.
- `C:\Users\chris\source\repos\arx-runa\tailwind.config.js` does **not** exist (nothing to delete — Tailwind v4 has no JS config).
- `C:\Users\chris\source\repos\arx-runa\public\leptos.svg` and `public\tauri.svg` exist from the template and are currently copied into `dist/` via `<link data-trunk rel="copy-dir" href="public" />`. Neither asset is referenced from `src/app.rs`.
- `C:\Users\chris\source\repos\arx-runa\.gitignore` ignores `/target` and `/dist/` but **does not** contain `node_modules/`.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\tauri.conf.json` already sets `beforeDevCommand = "trunk serve"`, `beforeBuildCommand = "trunk build"`, `devUrl = "http://localhost:1420"`, `frontendDist = "../dist"` — all matching the design.
- `C:\Users\chris\source\repos\arx-runa\docs\guides\development.md` has Rust/Trunk prerequisites but does **not** mention Node.js, `npm install`, Tailwind v4, or `cargo tauri dev`/`cargo tauri build` explicitly.

**Pending Architectural Decisions**: None. Decisions #4, #5, #6, #11 in the design's "Decisions Made" table are already resolved — this sub-phase just executes them.

**Estimated scope** (from sub-roadmap): ~60 lines created/modified, no test code.

## 3. Design Concerns / Open Questions

Step 1.75 review of `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\project-scaffolding\sub-phases\0.3-frontend-build-pipeline.md` against the parent design and current repo state.

### Concern 1 — Existing `Trunk.toml` must be updated, not created

**Concern**: Deliverable #1 says "Create `Trunk.toml` at the project root". The file already exists (created in Phase 0.1 by `cargo create-tauri-app`) with `[watch]` and `[serve]` sections that the design's `Trunk.toml` example does not show.
**Source**: `0.3-frontend-build-pipeline.md` line 12; current `Trunk.toml` lines 1–10.
**Impact**: If Codex blindly overwrites, it loses the `[watch] ignore = ["./src-tauri"]` setting (which prevents Trunk from rebuilding on backend edits) and the `[serve] port = 1420, open = false` setting (which pins the dev port matched by `tauri.conf.json`'s `devUrl`).
**Classification**: Non-blocking.
**Resolution**: The plan updates `Trunk.toml` in place — it **adds** the `[[hooks]]` block and sets `[build] target = "index.html"` (dropping the `./` prefix for consistency with the design), but **keeps** the existing `[watch]` and `[serve]` sections. See Assumption 1.

### Concern 2 — Orphaned `styles.css` after switch to Tailwind

**Concern**: The sub-phase does not explicitly instruct deletion of the pre-existing `C:\Users\chris\source\repos\arx-runa\styles.css` (template default). After switching `index.html`'s CSS link to `output.css`, `styles.css` is unreferenced but still sitting at the project root, which is confusing and noisy.
**Source**: Sub-phase has no mention of `styles.css`; current `styles.css` lines 1–113 and `index.html` line 6.
**Impact**: Leaving it creates two root-level CSS files with unclear roles (`input.css` — Tailwind source — vs. `styles.css` — dead file). A future contributor editing `styles.css` will see their changes go nowhere.
**Classification**: Non-blocking.
**Resolution**: Delete `styles.css` as part of the switch to `output.css`. Documented as an explicit step (Step 5 below).

### Concern 3 — `.container` CSS class referenced by `src/app.rs` disappears

**Concern**: `src/app.rs` line 7 uses `<main class="container">`, which is currently styled by `styles.css` (lines 20–27: `margin`, `padding-top: 10vh`, `flex column`, `center`). Tailwind v4 does **not** ship a `container` utility by default — it was removed from core. After deleting `styles.css`, the class becomes a no-op and the app renders as an unstyled left-aligned `<h1>`.
**Source**: `src/app.rs` line 7; current `styles.css` lines 20–27; Tailwind v4 release notes.
**Impact**: The sub-phase's acceptance criterion is a **smoke test** ("a desktop window must open showing the default Leptos content" — sub-phase lines 44, 69). A window with an unstyled `<h1>Hello Arx Runa</h1>` still passes that criterion. No regression in deliverables.
**Classification**: Non-blocking.
**Resolution**: Leave `src/app.rs` as-is. Do not rewrite Leptos markup in Phase 0.3 — frontend styling is Phase 6's domain. Recorded as Assumption 2.

### Concern 4 — Orphaned template SVGs under `public/`

**Concern**: `C:\Users\chris\source\repos\arx-runa\public\leptos.svg` and `public\tauri.svg` are copied to `dist/` by `index.html`'s `<link data-trunk rel="copy-dir" href="public" />` but are never referenced by `src/app.rs`.
**Source**: `index.html` line 7; `src/app.rs`; `public/` directory listing.
**Impact**: Pure dead assets — ~5 KB. Not a blocker, but removing the `copy-dir` link and deleting `public/` keeps Phase 0 clean.
**Classification**: Non-blocking.
**Resolution**: Remove the `<link data-trunk rel="copy-dir" href="public" />` line and delete `C:\Users\chris\source\repos\arx-runa\public\`. Recorded as Step 5.

### Concern 5 — Node.js prerequisite not documented

**Concern**: The sub-phase requires `package.json` with `tailwindcss` + `@tailwindcss/cli` and a pre-build hook that runs `npx @tailwindcss/cli`. This assumes Node.js/`npm`/`npx` is on `PATH`. Current `docs/guides/development.md` lists Rust, Trunk, Strawberry Perl, and VS Code but **not Node.js**.
**Source**: `0.3-frontend-build-pipeline.md` line 12 (hook uses `@tailwindcss/cli`); `docs/guides/development.md` lines 5–12 (prerequisites list).
**Impact**: New contributors cloning the repo will get a cryptic `npx: command not found` failure when running `cargo tauri dev` for the first time, with no pointer to the root cause.
**Classification**: Non-blocking — the fix is to document it (which Deliverable #7 already requires).
**Resolution**: When updating `docs/guides/development.md` (Step 9), add Node.js ≥ 18 to the prerequisites list **and** note that `npm install` must be run once after cloning to populate `node_modules/` with the Tailwind packages (because Trunk's hook calls `npx @tailwindcss/cli` rather than managing the install itself).

### Concern 6 — `node_modules/` not yet ignored

**Concern**: `.gitignore` does not include `node_modules/`. After `npm install`, ~50 MB of `node_modules/tailwindcss/**` and `node_modules/@tailwindcss/**` would otherwise be stageable.
**Source**: `.gitignore` lines 1–19; sub-phase acceptance criterion "node_modules/ is in .gitignore" (line 78).
**Impact**: Accidentally committed `node_modules/` is a real risk on the first run — a `git add -A` would sweep it all in.
**Classification**: Non-blocking (the sub-phase already calls this out).
**Resolution**: Add `node_modules/` to `.gitignore` **before** running `npm install`, so the install cannot race a commit. Recorded as Step 1.

### Concern 7 — `cargo test -p arx-runa-tauri` package name

**Concern**: Deliverable #7 tells the plan to document "Running only backend tests: `cargo test -p arx-runa-tauri`". Verified against `src-tauri/Cargo.toml` line 2: the package name is indeed `arx-runa-tauri`. No conflict.
**Source**: `0.3-frontend-build-pipeline.md` line 51; `src-tauri/Cargo.toml` line 2.
**Impact**: None — just recording that the check was performed.
**Classification**: Non-blocking (informational).
**Resolution**: Use `cargo test -p arx-runa-tauri` verbatim in the development.md update.

### Concern 8 — `cargo tauri dev` smoke test cannot run on headless CI

**Concern**: Acceptance criterion "`cargo tauri dev` launches a window showing the Leptos app" is a **manual, interactive** check. The project's CI runs on `ubuntu-latest` (per the sub-roadmap notes, line 122) with no display. CI can build but cannot open a window.
**Source**: `0.3-frontend-build-pipeline.md` lines 43–44, 69; sub-roadmap line 122.
**Impact**: None for CI (which only runs `fmt`/`clippy`/`test`/`build --release`). But a Codex agent running the plan's verification steps might wait forever on `cargo tauri dev` without a user present.
**Classification**: Non-blocking.
**Resolution**: Mark `cargo tauri dev` as a **manual** verification step in Step 10, explicitly requiring a human on a desktop. Codex's automated verification runs only the four `cargo` commands. Recorded in Handoff Notes.

No blocking concerns. Plan status: `draft`.

## 4. Assumptions

1. **`Trunk.toml` keeps `[watch]` and `[serve]` sections**. The design's example `Trunk.toml` shows only `[build]` and `[[hooks]]`, but the current file's `[watch] ignore = ["./src-tauri"]` and `[serve] port = 1420, open = false` are load-bearing (port matches `tauri.conf.json#devUrl`; ignore prevents frontend rebuilds on backend edits). They stay.

2. **`src/app.rs` is not rewritten to apply Tailwind classes**. The smoke test only requires a window with Leptos content; design tokens must appear **in `output.css`**, which Tailwind generates from scanning `src/**/*.rs` for class usage — but Tailwind v4 includes the `@theme` CSS custom properties in output regardless of whether any HTML class references them. Frontend styling of app components is Phase 6's responsibility.

3. **Trunk's pre-build hook resolves `@tailwindcss/cli` via `npx`**. This requires `node_modules/` to have been populated by `npm install` **before** the first `cargo tauri dev`. The dev workflow documented in `development.md` will mention `npm install` as a one-time setup step.

4. **The `[[hooks]]` stage is `pre_build`**. Matches the design's Trunk Configuration block verbatim (`design.md` line 282).

5. **Acceptance criterion "Tailwind brand token classes appear in compiled CSS"** is satisfied by the `@theme { --color-iron: … }` etc. declarations appearing in `output.css` as standard CSS custom properties — **not** by literal utility class strings like `.bg-iron { background-color: … }`, which Tailwind v4 only emits on demand when it sees class usage. The manual verification step greps `output.css` for the hex values and custom property names (`--color-iron`, `--color-stone`, `--color-steel`, `--color-rune`, `--color-bone`), not for utility class names.

6. **The root `[package]` section in `Cargo.toml` stays**. It was set up in Phase 0.1 as a package+workspace manifest — Trunk needs `[package]` to target the root directory. Do not touch `Cargo.toml` in this sub-phase.

7. **`tauri.conf.json` does not need changes**. The `beforeDevCommand`, `devUrl`, and `frontendDist` already match the design (verified above). Do not touch `tauri.conf.json`.

8. **CSP hardening is deferred to Phase 6**. The current `tauri.conf.json` has `"csp": null`, which the `.claude/rules/tauri.md` file flags as requiring tightening. The sub-phase does not include CSP work; this is consistent with the design's "Capabilities" section ("Phase 6 must tighten this").

## 5. Approach

All paths are absolute Windows paths rooted at `C:\Users\chris\source\repos\arx-runa\`.

### Step 1 — Add `node_modules/` to `.gitignore`

Edit `C:\Users\chris\source\repos\arx-runa\.gitignore`. Add a `node_modules/` line under the existing `/dist/` line:

```gitignore
/target
/dist/
node_modules/

# Claude Code - local settings (personal, not shared)
.claude/settings.local.json
…
```

Rationale (Concern 6): Ignore **before** running `npm install` so nothing can accidentally stage.

### Step 2 — Create `C:\Users\chris\source\repos\arx-runa\package.json`

New file, exactly:

```json
{
  "devDependencies": {
    "tailwindcss": "^4",
    "@tailwindcss/cli": "^4"
  }
}
```

Rationale: Tailwind v4 separates CLI from core; both packages must be declared. Matches design.md lines 289–291 verbatim.

### Step 3 — Create `C:\Users\chris\source\repos\arx-runa\input.css`

New file. Copy the full `@import` + `@theme { … }` block from `design.md` lines 297–330 verbatim:

```css
/* input.css */
@import "tailwindcss";

@theme {
  /* Core palette */
  --color-iron:  #09090B;   /* darkest — page fill */
  --color-stone: #0C0E14;   /* card surfaces */
  --color-steel: #222736;   /* borders, dividers */
  --color-rune:  #5C7090;   /* primary accent, logomark */
  --color-bone:  #DBD7CD;   /* primary text */

  /* Text scale */
  --color-text-primary:   #DBD7CD;
  --color-text-secondary: #9AA3B0;
  --color-text-muted:     #636D7E;
  --color-text-ghost:     #3E4A5E;

  /* Surface scale */
  --color-surface-base:    #09090B;
  --color-surface-raised:  #0C0E14;
  --color-surface-overlay: #111420;

  /* Border scale */
  --color-border-subtle:  #181C26;
  --color-border-default: #1C2030;
  --color-border-strong:  #222736;

  /* Typography */
  --font-display: 'Helvetica Neue', Helvetica, Arial, sans-serif;
  --font-body:    Georgia, 'Times New Roman', serif;
  --font-mono:    'Courier New', Courier, monospace;
}
```

Rationale: Tailwind v4 is CSS-configured; the `@theme` block is the full brand token surface sourced from `docs/arx-runa-brand.css`.

### Step 4 — Update `C:\Users\chris\source\repos\arx-runa\Trunk.toml`

**Modify in place** (Concern 1 — keep `[watch]` and `[serve]`). Target state:

```toml
[build]
target = "index.html"

[[hooks]]
stage = "pre_build"
command = "npx"
command_arguments = ["@tailwindcss/cli", "-i", "input.css", "-o", "output.css"]

[watch]
ignore = ["./src-tauri"]

[serve]
port = 1420
open = false
```

Changes vs. current file:

- `[build] target = "./index.html"` → `[build] target = "index.html"` (match design).
- **Insert** new `[[hooks]]` block between `[build]` and `[watch]`.
- `[watch]` and `[serve]` sections **unchanged**.

Rationale: Adds the Tailwind pre-build compilation step matching design.md lines 277–285 verbatim while preserving Phase 0.1 dev ergonomics.

### Step 5 — Update `C:\Users\chris\source\repos\arx-runa\index.html` and remove template assets

**Modify `index.html`** to point to `output.css` (Tailwind-generated) and drop the `public/` copy:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Arx Runa</title>
    <link data-trunk rel="css" href="output.css" />
    <link data-trunk rel="rust" data-wasm-opt="z" />
  </head>
  <body></body>
</html>
```

Changes vs. current file:

- Add `<!doctype html>`, `lang="en"`, `<meta charset>`, `<meta viewport>` (matches sub-phase deliverable #2 lines 15–27).
- `styles.css` → `output.css`.
- Remove `<link data-trunk rel="copy-dir" href="public" />` (Concern 4).
- Keep `data-trunk rel="rust" data-wasm-opt="z"` — the `data-wasm-opt="z"` was added in Phase 0.1 and is a release-size optimization we want to retain.

**Delete** the following files and directory (they are now orphaned):

- `C:\Users\chris\source\repos\arx-runa\styles.css` (Concern 2).
- `C:\Users\chris\source\repos\arx-runa\public\leptos.svg` (Concern 4).
- `C:\Users\chris\source\repos\arx-runa\public\tauri.svg` (Concern 4).
- `C:\Users\chris\source\repos\arx-runa\public\` (empty after file removal).

Do **not** delete or modify `C:\Users\chris\source\repos\arx-runa\src\app.rs` (Assumption 2) or anything under `src-tauri/`.

### Step 6 — Confirm no `tailwind.config.js` exists

Deliverable #4 from the sub-phase. Verified in Step 1.75 that the template did not generate one. This step is a **no-op verification**: run `ls tailwind.config.js` (expect: file not found). If a future template regenerates one, delete it — Tailwind v4 has no JS config.

### Step 7 — Run `npm install`

From `C:\Users\chris\source\repos\arx-runa\`:

```bash
npm install
```

Expected result: `node_modules/` populated with `tailwindcss` and `@tailwindcss/cli` (and their transitive deps). A `package-lock.json` is created at the project root. Commit `package-lock.json` per npm conventions (reproducible builds), but do not commit `node_modules/` (Step 1 ignored it).

### Step 8 — Run automated verification (four cargo commands)

Exactly the four commands from the sub-phase's Validation Checkpoint (lines 60–65), in this order:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

All four must exit 0. `cargo build --release` is the first command that triggers Trunk and therefore the Tailwind pre-build hook — the first invocation downloads WASM and builds Tailwind, so expect it to take several minutes.

**Failure recovery**:

- If `cargo fmt --all -- --check` reports diffs: run `cargo fmt --all` and re-run. Phase 0.3 adds no Rust code, so any diff is pre-existing drift in `src-tauri/src/` skeletons or `src/app.rs`.
- If `clippy` fails: the error is in backend skeletons from Phase 0.2 and belongs in a Phase 0.2 fix, not here. Report the failure and stop.
- If `cargo build --release` fails with `npx: command not found`: Node.js is not on `PATH`. Install Node ≥ 18 and retry. Document in Handoff Notes.
- If `cargo build --release` fails with `Cannot find module '@tailwindcss/cli'`: `npm install` (Step 7) did not run or ran in the wrong directory. Re-run from project root.

### Step 9 — Update `C:\Users\chris\source\repos\arx-runa\docs\guides\development.md`

Deliverable #7. Required additions:

- **Prerequisites section** (currently lines 5–12): add a `Node.js ≥ 18` bullet with the note that it is needed by Trunk's Tailwind CSS v4 pre-build hook.
- **Cargo workflow section** (currently lines 66–86): add explicit `cargo tauri dev` / `cargo tauri build` entries (they are referenced implicitly by `.vscode/launch.json` but not documented as first-class commands). Also add a "Frontend only" subsection covering `trunk serve` and a "Backend tests only" entry using `cargo test -p arx-runa-tauri` (package name verified — Concern 7).
- **New section — "Tailwind CSS"** (after the Cargo workflow section): document `npm install` as a one-time post-clone setup step, note that Tailwind v4 is CSS-configured via `input.css#@theme`, and point to `docs/arx-runa-brand.css` as the brand token source.

Minimum acceptable wording for the new bullets (Codex may reword for flow, but the commands must appear verbatim):

```markdown
## Prerequisites
- …existing bullets…
- [Node.js](https://nodejs.org/) v18 or later (`node --version`) — required by the
  Trunk pre-build hook which invokes `npx @tailwindcss/cli` to compile Tailwind CSS v4.

## First-time setup after cloning

```bash
npm install
```

Populates `node_modules/` with `tailwindcss` and `@tailwindcss/cli`. Required
before the first `cargo tauri dev` or `cargo build --release`. `node_modules/`
is gitignored.

## Cargo workflow
…existing content…

# Full app (frontend + backend, hot-reload)
cargo tauri dev

# Production bundle
cargo tauri build

# Frontend only (Leptos/WASM via Trunk)
trunk serve

# Backend tests only
cargo test -p arx-runa-tauri
```

### Tailwind CSS v4

Tailwind v4 has no `tailwind.config.js`. The brand token palette (iron, stone,
steel, rune, bone) is declared in `input.css` inside an `@theme` block. Source:
`docs/arx-runa-brand.css`. The Trunk pre-build hook (`Trunk.toml`) compiles
`input.css` → `output.css` on every build.
```

Do not remove existing development.md sections (debugging, hooks, SSOT, AI setup, encryption stack table).

### Step 10 — Manual smoke test (human required)

On a desktop (Windows/macOS/Linux with a display), run:

```bash
cargo tauri dev
```

Verify:

1. The command prints `trunk serve` output showing `input.css → output.css` compilation.
2. A desktop window opens titled "Arx Runa" (800 × 600 per `tauri.conf.json`).
3. The window shows `<h1>Hello Arx Runa</h1>` (unstyled because `styles.css` was removed — expected per Concern 3).
4. Inspect `C:\Users\chris\source\repos\arx-runa\dist\` after the Trunk build: `output.css` must exist.
5. Grep `output.css` for the brand token hex values and custom property names:

   ```bash
   grep -E "#09090B|#0C0E14|#222736|#5C7090|#DBD7CD" dist/output.css
   grep -E "--color-iron|--color-stone|--color-steel|--color-rune|--color-bone" dist/output.css
   ```

   Both greps must return matches (Assumption 5 — Tailwind v4 emits the `@theme` custom properties in `output.css` regardless of utility class usage).

6. Close the window (Ctrl+C to stop `tauri dev`).

If any check fails, **do not** mark Phase 0 complete — diagnose and fix before Step 11.

### Step 11 — Mark Phase 0 complete in `C:\Users\chris\source\repos\arx-runa\docs\roadmap.md`

Deliverable #8. Edit the status column of the Phase 0 row in the roadmap's phases table (line 7):

```
| 0 — Scaffolding | Project structure, build pipeline, CI | Complete |
```

Change only the word `Planned` → `Complete` on that row. Leave the other phase rows untouched.

## 6. Security Implications

### (a) Expected sensitive path set

**None anticipated.** Phase 0.3 touches only:

- `.gitignore`
- `package.json` (new)
- `input.css` (new)
- `Trunk.toml`
- `index.html`
- `styles.css` (deleted)
- `public/leptos.svg`, `public/tauri.svg`, `public/` (deleted)
- `docs/guides/development.md`
- `docs/roadmap.md`

No file under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` should be touched. If `/implement-plan`'s drift check reports any file under those paths, **stop and investigate** — that is a Plan Deviation and must be audited.

### (b) Invoke `security-reviewer` agent? **NO**

**Rationale**: Phase 0.3 is build-tooling only. No cryptographic operations, no key material, no authentication logic, no data persistence. The only executable code added is a shell invocation (`npx @tailwindcss/cli …`) run at build time on a trusted developer machine, operating on non-sensitive CSS. The sub-phase's self-assessment ("Security Review: Not required" — `0.3-frontend-build-pipeline.md` line 102) is independently confirmed.

### (c) What would be unnecessary

- No cryptographic primitive review (no crypto in scope).
- No AAD / nonce / zeroization review (no secret handling in scope).
- No capability-file or IPC-command review (`tauri.conf.json` and `capabilities/` are unchanged — Assumption 7).
- No dependency audit beyond what `cargo audit` already catches in CI (npm packages `tailwindcss` and `@tailwindcss/cli` are build-time only and do not ship in the Tauri bundle).

One soft note for future Phase 6 work: the current `tauri.conf.json` has `"csp": null`, which `.claude/rules/tauri.md` requires tightening before release. This is known, deferred to Phase 6 per the design, and **out of scope** for Phase 0.3.

## 7. Testing Strategy

Phase 0.3 adds no application logic, so there are no unit tests, no property tests, and no adversarial tests. Validation is **compiler + build-tooling driven**.

**Test scope:**

- [ ] Basic unit tests — **N/A** (no application code changes).
- [ ] Adversarial tests — **N/A** (no cryptographic edge cases).
- [ ] Property-based tests — **N/A**.
- [ ] Integration tests — **N/A**.
- [x] **Build-pipeline verification** — the four `cargo` commands in Step 8 plus the manual smoke test in Step 10.

**Coverage target**: N/A (no application code).

**Automated acceptance criteria** (all four must exit 0):

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

**Manual acceptance criteria** (Step 10, human required):

- `cargo tauri dev` opens a window titled "Arx Runa".
- `dist/output.css` exists after the first Trunk build.
- `dist/output.css` contains the five core brand hex values (`#09090B`, `#0C0E14`, `#222736`, `#5C7090`, `#DBD7CD`).
- `dist/output.css` contains the five core `--color-*` custom properties.
- `docs/guides/development.md` mentions Node.js, `npm install`, `cargo tauri dev`, `cargo tauri build`, `trunk serve`, and `cargo test -p arx-runa-tauri`.
- `docs/roadmap.md` row for Phase 0 reads `Complete`, not `Planned`.
- `node_modules/` is in `.gitignore` and not staged.

**Invoke `test-writer` agent? NO** — Phase 0.3 writes no test code. Rationale: no application logic, no security surface, no behaviors to verify beyond "the build pipeline compiles and runs". The `test-writer` agent is for adversarial and property-based coverage of runtime code — it has nothing to test in a build-tooling sub-phase. This matches the sub-roadmap's "Phase 0 has no application logic to unit test" statement (sub-roadmap line 71).

## 8. Documentation Impact

- **`C:\Users\chris\source\repos\arx-runa\docs\guides\development.md`** — update per Step 9: Node.js prerequisite, `npm install` first-time setup, `cargo tauri dev` / `cargo tauri build` / `trunk serve` commands, `cargo test -p arx-runa-tauri` for backend-only tests, new "Tailwind CSS v4" section pointing to `input.css` and `docs/arx-runa-brand.css`.
- **`C:\Users\chris\source\repos\arx-runa\docs\roadmap.md`** — flip Phase 0 row status `Planned` → `Complete` (Step 11).
- **`C:\Users\chris\source\repos\arx-runa\docs\report-log\`** — the existing post-commit hook auto-creates a stub when `src-tauri/src/` is touched; Phase 0.3 touches no backend code, so **no stub is expected**. If a stub appears, it's from an unrelated edit and should be investigated.
- **`C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\project-scaffolding\`** — no design document changes. All facts in this plan come directly from the existing `design.md` and sub-phase file.

## 9. Handoff Notes for Implementer

**Working directory**: `C:\Users\chris\source\repos\arx-runa\`. Shell: bash (forward slashes fine). All paths in this plan are absolute.

**Order of operations**: Steps 1 → 11 in order. Step 1 must precede Step 7 (`.gitignore` before `npm install`). Step 8's automated verification must precede Step 10's manual smoke test must precede Step 11's roadmap status flip — do not mark Phase 0 complete until the smoke test is green.

**Self-containment**: This plan is self-contained — all verbatim file contents (package.json, input.css, Trunk.toml target state, index.html target state, .gitignore addition, roadmap row) are inlined. The implementer does not need to re-read `0.3-frontend-build-pipeline.md` or `design.md` unless a concern during implementation is unclear.

**Known traps**:

1. **Node.js must be installed first.** The plan does not install Node — it documents the requirement. If running Codex on a Windows CI runner, `winget install OpenJS.NodeJS.LTS` (or equivalent on Linux/macOS) must happen before Step 7. If Node is missing, `npm install` fails and Step 8's `cargo build --release` fails with a confusing `npx: command not found`.
2. **First `cargo build --release` is slow.** Cargo compiles the full Tauri + Leptos + OpenSSL-vendored-for-SQLCipher backend for the first time. Expect 5–15 minutes on fresh `target/`. This is not a hang.
3. **`cargo tauri dev` (Step 10) requires a display.** It opens a window. On headless CI or a bare SSH session, it will not exit and will block any automated harness. Flag to the user as a manual step — do not try to run it non-interactively.
4. **Do not modify `Cargo.toml` (root) or `src-tauri/Cargo.toml`.** All crate dependencies were finalised in Phase 0.2. Only Phase 0.3 frontend/build files (listed in Section 6(a)) should change.
5. **Do not touch `src/app.rs`, `src/main.rs`, or anything under `src-tauri/src/`.** Phase 0.3 is build-pipeline only. If a verification step requires rewriting application code, that is a sign something is wrong — stop and re-read Assumption 2.
6. **`cargo tauri dev` first run may trigger Trunk to download the `wasm32-unknown-unknown` toolchain.** Harmless; just slow.
7. **Cross-platform**: the plan uses `npx`, `cargo`, and `trunk` commands that work identically on Windows, macOS, and Linux. Developer-docs prerequisites (Step 9) must mention Node.js for all three platforms, not just Windows.

Plan status: `implemented`.

## 10. Implementation Log

- **Date**: 2026-04-12T22:32:05.3030720Z
- **Branch**: `development`
- **Files changed**:
  - `C:\Users\chris\source\repos\arx-runa\.claude\plans\phase-0-3-frontend-build-pipeline.md`
  - `C:\Users\chris\source\repos\arx-runa\.gitignore`
  - `C:\Users\chris\source\repos\arx-runa\package.json` (new)
  - `C:\Users\chris\source\repos\arx-runa\package-lock.json` (new)
  - `C:\Users\chris\source\repos\arx-runa\input.css` (new)
  - `C:\Users\chris\source\repos\arx-runa\Trunk.toml`
  - `C:\Users\chris\source\repos\arx-runa\index.html`
  - `C:\Users\chris\source\repos\arx-runa\styles.css` (deleted)
  - `C:\Users\chris\source\repos\arx-runa\public\leptos.svg` (deleted)
  - `C:\Users\chris\source\repos\arx-runa\public\tauri.svg` (deleted)
  - `C:\Users\chris\source\repos\arx-runa\docs\guides\development.md`
  - `C:\Users\chris\source\repos\arx-runa\docs\roadmap.md`
- **Test results**:
  - `cargo fmt --all -- --check` — passed
  - `cargo clippy --all-targets --all-features -- -D warnings` — passed
  - `cargo test --all-targets` — passed (`0 passed; 0 failed`)
  - `cargo build --release` — passed
  - `cargo tauri dev` startup smoke run — Trunk hook executed and backend app launched; visual window confirmation remains a manual desktop check
- **Clippy results**:
  - `cargo clippy --workspace -- -D warnings` — clean
- **Security review**:
  - N/A (plan section 6(b) = NO). Drift check: no touched files under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/`.
- **Deviations from plan**:
  - Updated the Trunk hook command from `npx @tailwindcss/cli ...` to `node ./node_modules/@tailwindcss/cli/dist/index.mjs ...` because Trunk could not spawn `npx`/`npm` on Windows (`program not found`) despite Node being installed; this restored successful `trunk build` and `cargo tauri dev` startup.
  - Trunk outputs hashed CSS asset names in `dist/` (for example `output-1f664076d8e1f10e.css`) rather than literal `dist/output.css`; token verification was run against the generated `output-*.css` file.
- **Documentation flagged**:
  - **`C:\Users\chris\source\repos\arx-runa\docs\guides\development.md`** — update per Step 9: Node.js prerequisite, `npm install` first-time setup, `cargo tauri dev` / `cargo tauri build` / `trunk serve` commands, `cargo test -p arx-runa-tauri` for backend-only tests, new "Tailwind CSS v4" section pointing to `input.css` and `docs/arx-runa-brand.css`.
  - **`C:\Users\chris\source\repos\arx-runa\docs\roadmap.md`** — flip Phase 0 row status `Planned` → `Complete` (Step 11).
  - **`C:\Users\chris\source\repos\arx-runa\docs\report-log\`** — the existing post-commit hook auto-creates a stub when `src-tauri/src/` is touched; Phase 0.3 touches no backend code, so **no stub is expected**. If a stub appears, it's from an unrelated edit and should be investigated.
  - **`C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\project-scaffolding\`** — no design document changes. All facts in this plan come directly from the existing `design.md` and sub-phase file.
