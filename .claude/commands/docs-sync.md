Documentation maintenance and GitHub Pages coordination for Arx Runa.

## Arguments

- No argument or `check` → validate documentation structure and report issues
- `fix` → automatically fix detected issues (add orphaned files to SUMMARY.md)
- `build` → test mdBook build locally before committing
- `status` → show GitHub Pages deployment status and readiness

---

## Check Mode (default)

Validates documentation structure without making changes.

### Validation checks

1. **Orphaned files** — files exist in `docs/` subdirectories but not referenced in `SUMMARY.md`
2. **Broken links** — references in `SUMMARY.md` point to non-existent files
3. **mdBook configuration** — required fields present in `book.toml`
4. **Workflow status** — whether `.github/workflows/docs.yml` is enabled
5. **Excluded files** — intentionally excluded files (report-log/, bachelor-report-requirements.md)

### Output format

```
Documentation Validation Report
================================

✓ SUMMARY.md structure valid
✓ No broken links
✗ 3 orphaned files detected:
  - architecture/designs/new-design.md
  - architecture/diagrams/new-diagram.md
  - guides/new-guide.md

✓ book.toml configuration complete
✓ GitHub Pages workflow enabled (.github/workflows/docs.yml)

Run `/docs-sync fix` to add orphaned files to SUMMARY.md
```

### Step-by-step

1. Read `docs/SUMMARY.md` and extract all file references
2. Scan `docs/` subdirectories for all `.md` files (exclude `book/`, `SUMMARY.md`, `README.md`, `404.md`)
3. Exclude intentional omissions:
   - `docs/report-log/**` (bachelor project internal)
   - `docs/guides/bachelor-report-requirements.md` (project internal)
4. Compare: files in filesystem vs references in SUMMARY.md
5. Check each SUMMARY.md reference exists on disk
6. Verify `docs/book.toml` has required fields: `site-url`, `git-repository-url`
7. Check `.github/workflows/docs.yml` exists (not `.disabled`)
8. Generate structured report

---

## Fix Mode

Automatically fixes detected issues.

### What it fixes

1. **Orphaned files** → adds them to appropriate sections in SUMMARY.md
   - Files in `architecture/designs/` → under "# Architecture → Designs"
   - Files in `architecture/diagrams/` → under "# Architecture → Diagrams"
   - Files in `guides/` → under "# Guides"
   - Files in `architecture-decisions/` → under "# Decisions"
   - Maintains alphabetical order within each section

2. **Broken links** → prompts for removal (requires confirmation)

### Safety rules

- NEVER remove or modify existing content beyond adding new entries
- ALWAYS maintain existing order and indentation structure
- ALWAYS preserve section headers and separators
- When adding files, respect alphabetical or logical ordering
- Backup SUMMARY.md before making changes

### Output format

```
Fixing Documentation Issues
============================

Adding orphaned files to SUMMARY.md:
  ✓ Added architecture/designs/new-design.md under Designs
  ✓ Added architecture/diagrams/new-diagram.md under Diagrams
  ✓ Added guides/new-guide.md under Guides

Broken links found:
  ✗ guides/missing-file.md (referenced but doesn't exist)
  → Remove this reference? (y/n): 

Changes saved to docs/SUMMARY.md
Run `mdbook build docs/` to verify
```

### Step-by-step

1. Run check mode validation first
2. For each orphaned file:
   - Determine target section based on path
   - Extract title from file (first H1 heading or filename)
   - Find correct insertion point in SUMMARY.md (alphabetical within section)
   - Insert new line with proper indentation
3. For each broken link:
   - Prompt user: "Remove reference to <file>? (y/n)"
   - If yes, remove the line from SUMMARY.md
4. Write updated SUMMARY.md
5. Run validation again to confirm fixes

---

## Build Mode

Tests mdBook build locally before committing.

### Prerequisites check

1. Verify `mdbook` is installed (`mdbook --version`)
2. Verify `mdbook-mermaid` is installed (`mdbook-mermaid --version`)
3. If missing, provide installation instructions

### Build process

1. Change to `docs/` directory
2. Run `mdbook-mermaid install .` (setup Mermaid preprocessor)
3. Run `mdbook build`
4. Capture output (stdout + stderr)
5. Check exit code
6. Verify `docs/book/index.html` exists

### Output format

**Success:**
```
mdBook Build Test
=================

✓ mdbook v0.4.40 found
✓ mdbook-mermaid v0.14.0 found
✓ Mermaid preprocessor configured
✓ Build completed successfully
✓ Output verified at docs/book/index.html

Ready to commit documentation changes.
```

**Failure:**
```
mdBook Build Test
=================

✓ mdbook v0.4.40 found
✓ mdbook-mermaid v0.14.0 found
✗ Build failed with errors:

<error output>

Fix errors before committing.
```

### Installation guidance

If mdbook not found:
```
mdbook is not installed. Install with:

# Pre-built binary (recommended for Windows):
Download from: https://github.com/rust-lang/mdBook/releases/download/v0.4.40/mdbook-v0.4.40-x86_64-pc-windows-msvc.zip
Extract mdbook.exe to ~/.cargo/bin/ or add to PATH

# Or via Rust cargo:
cargo install mdbook --version 0.4.40
```

If mdbook-mermaid not found:
```
mdbook-mermaid is not installed. Install with:

# Pre-built binary (recommended for Windows):
Download from: https://github.com/badboy/mdbook-mermaid/releases/download/v0.14.0/mdbook-mermaid-v0.14.0-x86_64-pc-windows-msvc.zip
Extract mdbook-mermaid.exe to ~/.cargo/bin/ or add to PATH

# Or via Rust cargo:
cargo install mdbook-mermaid --version 0.14.0
```

---

## Status Mode

Reports GitHub Pages deployment status and configuration.

### Information gathered

1. **Workflow status** — `.github/workflows/docs.yml` exists (enabled) or `.github/workflows/docs.yml.disabled` (disabled)
2. **Expected URL** — `https://salgaard.github.io/arx-runa/` (from `book.toml` + repository)
3. **Last build** — query Git for last commit that modified `docs/`
4. **GitHub Actions status** — if possible, query GitHub API for last workflow run (optional, may require auth)

### Output format

```
GitHub Pages Status
===================

Workflow: ENABLED (.github/workflows/docs.yml)
Triggers: push to master branch when docs/ or workflow file changes

Expected URL: https://salgaard.github.io/arx-runa/
Configuration: docs/book.toml (site-url: /arx-runa/)

Last docs change: 2026-04-02 02:44:15 (commit: abc1234)

GitHub Actions:
  To enable deployment:
  1. Go to Settings → Pages in GitHub repository
  2. Under "Build and deployment", select "Source: GitHub Actions"
  3. Next push to master with docs/ changes will auto-deploy

Ready for deployment: YES
```

### Step-by-step

1. Check if `.github/workflows/docs.yml` exists
   - If not, check `.github/workflows/docs.yml.disabled`
2. Parse `docs/book.toml` for `site-url` and `git-repository-url`
3. Determine expected GitHub Pages URL from repository URL + site-url
4. Run `git log -1 --format="%H %ci" -- docs/` for last docs commit
5. Check workflow file for trigger configuration (branches, paths)
6. Generate status report

---

## When to Use

**Invoke manually:**
- `/docs-sync` or `/docs-sync check` — validate documentation
- `/docs-sync fix` — auto-fix orphaned files and broken links
- `/docs-sync build` — test local build before pushing
- `/docs-sync status` — check deployment configuration

---

## Exclusion Rules

These paths are intentionally excluded from public documentation:

| Path | Reason | Should appear in SUMMARY.md |
|------|--------|----------------------------|
| `docs/report-log/**` | Bachelor project progress notes (internal) | NO |
| `docs/guides/bachelor-report-requirements.md` | Project requirements (internal) | NO |
| `docs/book/` | Build output directory (gitignored) | NO |
| `docs/SUMMARY.md` | Navigation file (not a content page) | NO |
| `docs/README.md` | Landing page (implicit in mdBook) | YES (as Introduction) |
| `docs/404.md` | Error page (implicit in mdBook) | NO |

The command must respect these exclusions when detecting orphaned files.

---

## Error Handling

**Broken SUMMARY.md syntax:**
- Report parse error with line number
- Suggest fixing manually before running fix mode

**Write permission denied:**
- Report error
- Suggest checking file permissions

**mdbook command not found:**
- Provide installation instructions
- Exit gracefully

**Build errors:**
- Capture full error output
- Highlight relevant error lines
- Suggest common fixes (broken links, invalid Mermaid syntax, missing files)
