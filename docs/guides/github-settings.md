# GitHub Repository Settings Guide

This guide documents the recommended GitHub settings for VoidGate. These must be
configured manually through the GitHub web UI or API.

## Branch Protection Rules

### `master` branch (already configured)

- ✅ Require a pull request before merging
- ✅ Require status checks to pass before merging
  - CI (build-and-test)
  - Security Audit
- ✅ Require branches to be up to date before merging
- ✅ Do not allow bypassing the above settings

### `development` branch (recommended)

Navigate to: **Settings → Branches → Add rule**

- **Branch name pattern**: `development`
- ✅ Require a pull request before merging
- ✅ Require approvals: 1 (or 0 for solo development)
- ✅ Require status checks to pass before merging
  - Required checks: `build-and-test`
- ❌ Require branches to be up to date (optional, can slow down development)
- ✅ Restrict who can push to matching branches (optional)

## Security Features

Navigate to: **Settings → Code security and analysis**

### Recommended Settings

| Feature | Status | Notes |
|---------|--------|-------|
| Dependency graph | ✅ Enable | Required for other features |
| Dependabot alerts | ✅ Enable | Alerts for vulnerable dependencies |
| Dependabot security updates | ✅ Enable | Auto-creates PRs for security fixes |
| Dependabot version updates | Optional | Auto-creates PRs for all updates |
| Code scanning | Optional | Requires setup (CodeQL) |
| Secret scanning | ✅ Enable | Detects accidentally committed secrets |
| Secret scanning push protection | ✅ Enable | Blocks pushes containing secrets |

### Dependabot Configuration (Optional)

If you want automated version updates, create `.github/dependabot.yml`:

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5
    labels:
      - "dependencies"
      - "rust"

  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    labels:
      - "dependencies"
      - "ci"
```

## GitHub Projects Board

Navigate to: **Projects → New project**

### Recommended Setup

1. **Create a new project** (Board view)
2. **Name**: "VoidGate Development" or similar
3. **Columns**:
   - Backlog
   - Ready
   - In Progress
   - In Review
   - Done

### Automation Rules

In the project settings, add these automations:

| Trigger | Action |
|---------|--------|
| Item added to project | Set status to "Backlog" |
| Item reopened | Set status to "Ready" |
| Pull request merged | Set status to "Done" |
| Issue closed | Set status to "Done" |

### Linking to Repository

1. Go to **Settings → Features → Projects**
2. Enable "Projects" if not already enabled
3. Link your project board to the repository

## Repository Settings

Navigate to: **Settings → General**

### Features

| Feature | Recommended |
|---------|-------------|
| Wikis | ❌ Disable (use docs/ instead) |
| Issues | ✅ Enable |
| Sponsorships | Optional |
| Preserve this repository | Optional |
| Discussions | ✅ Enable |
| Projects | ✅ Enable |

### Pull Requests

| Setting | Recommended |
|---------|-------------|
| Allow merge commits | ✅ Enable |
| Allow squash merging | ✅ Enable (default) |
| Allow rebase merging | ✅ Enable |
| Always suggest updating PR branches | ✅ Enable |
| Automatically delete head branches | ✅ Enable |

## Environments (For Releases)

If you want release signing, create environments:

Navigate to: **Settings → Environments → New environment**

### `release` environment

- **Protection rules**: Require reviewers (optional)
- **Environment secrets**:
  - `TAURI_SIGNING_PRIVATE_KEY` — Tauri update signing key
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — Key password (if applicable)

## Actions Permissions

Navigate to: **Settings → Actions → General**

### Recommended Settings

- **Actions permissions**: Allow all actions and reusable workflows
- **Workflow permissions**: Read and write permissions
- ✅ Allow GitHub Actions to create and approve pull requests

## Discussion Categories (Optional)

Navigate to: **Discussions → Categories**

Suggested categories:

| Category | Purpose |
|----------|---------|
| Announcements | Project news and updates |
| General | General conversation |
| Ideas | Feature ideas and suggestions |
| Q&A | Questions and answers |
| Show and Tell | Share what you've built |

## Checklist

Use this checklist to verify your setup:

- [ ] `development` branch protection configured
- [ ] Dependabot alerts enabled
- [ ] Secret scanning enabled
- [ ] Push protection enabled
- [ ] Discussions enabled
- [ ] Project board created and linked
- [ ] Auto-delete head branches enabled
- [ ] Required status checks configured
- [ ] GitHub Pages enabled (source: GitHub Actions)

## GitHub Pages

Navigate to: **Settings → Pages**

### Setup

1. **Source**: Select "GitHub Actions"
2. Wait for the first deployment after pushing to `master`
3. Your documentation will be available at: `https://chorizzio.github.io/void-gate/`

### How It Works

The documentation is built using [mdBook](https://rust-lang.github.io/mdBook/)
with Mermaid diagram support. The workflow:

1. Triggered on push to `master` (when `docs/` changes)
2. Installs `mdbook` and `mdbook-mermaid`
3. Builds the book from `docs/`
4. Deploys to GitHub Pages

### Local Preview

To preview documentation locally:

```bash
# Install mdBook
cargo install mdbook mdbook-mermaid

# Build and serve
cd docs
mdbook-mermaid install
mdbook serve --open
```

The local server runs at `http://localhost:3000`.
