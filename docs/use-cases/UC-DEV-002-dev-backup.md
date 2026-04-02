# UC-DEV-002: Development Artifact Backup

**Category**: Developer & Technical

**Status**: Active

---

## Overview

A developer or software team wants to back up build artifacts, compiled binaries, container images, or source code snapshots to cloud storage without exposing intellectual property or proprietary algorithms to the cloud provider.

## Actors

- **Primary Actor**: Software developer, build engineer, or DevOps team
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system, USB key file

## Preconditions

- Developer has VoidGate installed on build machine or workstation
- Developer has generated USB key file and stored securely
- Developer has configured Rclone backend (personal cloud, company S3, etc.)
- Developer has build artifacts to back up (binaries, Docker images, source archives)

## Main Flow

1. Developer completes software build:
   - Compiles binary (Go, Rust, C++ executable)
   - Builds Docker container image (`docker save my-app:v1.0.0 -o my-app.tar`)
   - Creates source code snapshot (`git archive --format=zip HEAD -o snapshot.zip`)
2. Developer launches VoidGate and unlocks vault with password + USB key
3. Developer selects "Upload Build Artifacts"
4. Developer chooses artifact files (e.g., `my-app-linux-amd64`, `my-app.tar`, `snapshot.zip`)
5. VoidGate encrypts each artifact:
   - Chunks into 4 MiB fixed-size blocks
   - Encrypts with XChaCha20-Poly1305
   - Computes BLAKE3 checksum over encrypted blobs
6. VoidGate uploads encrypted chunks to cloud with random UUID blob names
7. VoidGate stores metadata in encrypted manifest:
   - Filename (e.g., `my-app-linux-amd64`)
   - Build version tag (e.g., `v1.0.0`)
   - Git commit hash (optional)
   - Timestamp
8. Developer locks vault
9. Later, developer needs to retrieve old build for debugging:
10. Developer unlocks vault on workstation
11. Developer searches manifest for "my-app v1.0.0"
12. VoidGate downloads encrypted chunks from cloud
13. VoidGate verifies BLAKE3 checksums (integrity check)
14. VoidGate decrypts and reassembles binary
15. Developer runs binary locally for regression testing
16. Developer locks vault

## Alternate Flows

### Container Image Backup

**Trigger**: Developer wants to back up Docker container image

**Steps**:
1. Developer exports Docker image: `docker save my-app:latest -o my-app.tar`
2. Developer uploads `my-app.tar` to VoidGate (large file, 500 MiB+)
3. VoidGate chunks into 125+ chunks (4 MiB each)
4. VoidGate encrypts and uploads chunks (may take several minutes)
5. Developer verifies upload completion
6. Later, developer pulls image from VoidGate:
7. Developer downloads and decrypts `my-app.tar`
8. Developer loads into Docker: `docker load -i my-app.tar`
9. Container image restored

### Source Code Snapshot (Alternative to Git)

**Trigger**: Developer wants encrypted backup of proprietary source code (not pushed to GitHub)

**Steps**:
1. Developer creates source archive: `tar -czf project.tar.gz src/`
2. Developer uploads to VoidGate
3. VoidGate encrypts archive (cloud cannot see source code)
4. Later, developer loses local copy (hard drive failure):
5. Developer unlocks vault on new machine
6. Developer downloads and decrypts `project.tar.gz`
7. Developer extracts: `tar -xzf project.tar.gz`
8. Source code recovered (no cloud provider exposure)

### Build Artifact Retention Policy

**Trigger**: Developer wants to keep only last N versions (delete old builds)

**Steps**:
1. Developer configures retention policy: "Keep last 5 builds per project"
2. VoidGate tracks build versions in manifest
3. When 6th build is uploaded:
4. VoidGate prompts: "Retention policy exceeded. Delete oldest build?"
5. Developer confirms
6. VoidGate deletes encrypted chunks for oldest build
7. Manifest updated (only 5 builds retained)

### Cross-Platform Builds Backup

**Trigger**: Developer builds for multiple platforms (Linux, macOS, Windows)

**Steps**:
1. Developer builds for all platforms:
   - `my-app-linux-amd64`
   - `my-app-darwin-arm64`
   - `my-app-windows-amd64.exe`
2. Developer uploads all three binaries to VoidGate
3. VoidGate tags each with platform metadata
4. Later, developer searches for "my-app v1.0.0 linux"
5. VoidGate filters to Linux binary
6. Developer downloads correct platform artifact

## Success Criteria

- Build artifacts are encrypted before upload (cloud cannot reverse-engineer binaries)
- Source code snapshots are encrypted (intellectual property protected)
- Container images are encrypted (Docker layers not exposed)
- Developer can retrieve specific versions by tag or commit hash
- Checksums verify integrity (detect corruption or tampering)
- Large artifacts (500+ MiB) are chunked and uploaded efficiently

## Related Designs

- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 encryption, BLAKE3 integrity checksums
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — 4 MiB fixed-size chunks for large binaries, encrypted manifest for metadata (version tags, commit hashes)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Rclone BYOC (developer can use personal Backblaze, Wasabi, etc.)
- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — USB key file prevents password-only access to proprietary artifacts

## Security Considerations

### Threats Addressed

- **Reverse engineering by cloud provider**: Encrypted binaries cannot be disassembled or analyzed
- **Source code leakage**: Proprietary algorithms and trade secrets protected
- **Container image analysis**: Cloud cannot scan Docker layers for vulnerabilities or secrets
- **Intellectual property theft**: Encrypted artifacts prevent unauthorized access
- **Supply chain attacks**: Checksums detect tampering (BLAKE3 over encrypted blobs)

### Assumptions

- Developer's build machine is trusted (no malware injecting backdoors during build)
- Developer secures USB key (physical access control)
- Cloud provider does not delete blobs (developer must verify backups periodically)
- Artifacts are not excessively large (multi-GB builds may be slow to upload/download)

### Out of Scope

- **Build reproducibility**: VoidGate stores artifacts, does not ensure deterministic builds
- **Code signing**: Developer must sign binaries separately (VoidGate provides encrypted storage, not signing)
- **Container registry alternative**: VoidGate is backup, not a replacement for Docker Hub/ECR
- **Continuous deployment**: VoidGate is manual backup, not integrated with CI/CD pipelines

## Notes

This use case highlights VoidGate's applicability beyond personal documents. Developers often build proprietary software and need secure backups without exposing IP to cloud providers.

**Why Not Use Docker Registry?**
- Docker Hub, ECR, GCR: Images are stored in plaintext on provider (provider can scan, analyze)
- VoidGate: Encrypted `docker save` archives (provider has no access to layers)

**Why Not Use Git with Encryption (git-crypt)?**
- Git-crypt: Tied to git repository, not for arbitrary binaries
- VoidGate: General-purpose encrypted storage (binaries, Docker images, archives)

**Why Not Use S3 with Server-Side Encryption (SSE-S3)?**
- SSE-S3: AWS manages keys (AWS can decrypt)
- VoidGate: Zero-knowledge (cloud provider has no keys)

**Performance Note**: 4 MiB chunks work well for binaries (10-100 MiB typical) but may be slow for multi-GB container images. Future optimization: adaptive chunk sizes or streaming uploads.

**Comparison to Artifact Repositories**:
- **Artifactory, Nexus**: Artifact metadata visible to provider (package names, versions)
- **VoidGate**: Encrypted manifest (provider sees only opaque blobs)

---

**References**:
- Docker Save/Load: [Docker Documentation](https://docs.docker.com/engine/reference/commandline/save/)
- Build Artifact Retention: [Artifactory Best Practices](https://www.jfrog.com/confluence/display/JFROG/Repository+Management)
- Supply Chain Security: SLSA Framework (checksums for integrity)
