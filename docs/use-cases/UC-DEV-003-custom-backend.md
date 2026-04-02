# UC-DEV-003: Custom Cloud Backend Integration

**Category**: Developer & Technical

**Status**: Active

---

## Overview

A technically sophisticated user wants full control over the cloud storage backend — choosing between commercial providers (S3, Azure), self-hosted solutions (MinIO, Ceph), or even experimental backends (IPFS, Storj) — without VoidGate imposing vendor lock-in or proprietary protocols.

## Actors

- **Primary Actor**: Power user, system administrator, or DevOps engineer
- **Secondary Actors**: Cloud storage provider (commercial, self-hosted, or decentralized), VoidGate system, Rclone

## Preconditions

- User has technical knowledge of cloud storage protocols (S3 API, WebDAV, SFTP, etc.)
- User has VoidGate installed with Rclone dependency
- User has access to desired storage backend (credentials, endpoint URLs)
- User has generated USB key file and vault password

## Main Flow

1. User evaluates cloud storage options:
   - **Commercial**: AWS S3, Azure Blob, Google Cloud Storage, Backblaze B2, Wasabi
   - **Self-hosted**: MinIO, Ceph, OpenStack Swift
   - **Decentralized**: IPFS (via Pinata), Storj, Sia
   - **Traditional**: SFTP server, WebDAV (Nextcloud, ownCloud)
2. User selects backend based on criteria:
   - Cost (Wasabi cheaper than AWS for bandwidth)
   - Privacy (self-hosted MinIO for maximum control)
   - Geographic compliance (EU-only providers for GDPR)
   - Decentralization (Storj for censorship resistance)
3. User configures Rclone backend:
   - Runs `rclone config` to set up remote
   - Provides credentials (API keys, endpoint URLs)
   - Tests connectivity: `rclone ls remote:`
4. User launches VoidGate
5. VoidGate prompts: "Select Rclone remote for vault"
6. User selects configured remote from dropdown (e.g., `wasabi-us-west`, `minio-homelab`)
7. User creates vault with password + USB key file
8. VoidGate stores Rclone remote name in vault configuration
9. User uploads files to vault
10. VoidGate encrypts files and uploads to selected backend via Rclone
11. User verifies blobs appear in backend (random UUIDs in bucket/container)
12. Later, user migrates to different backend:
13. User configures new Rclone remote (e.g., `storj-decentralized`)
14. User selects "Migrate Vault" in VoidGate
15. VoidGate downloads encrypted blobs from old backend
16. VoidGate uploads encrypted blobs to new backend (no re-encryption needed)
17. VoidGate updates vault configuration with new remote name
18. User deletes blobs from old backend (migration complete)

## Alternate Flows

### Self-Hosted MinIO Setup

**Trigger**: User wants maximum privacy and control (on-prem storage)

**Steps**:
1. User sets up MinIO server on home NAS or datacenter:
   - `docker run -p 9000:9000 minio/minio server /data`
2. User configures Rclone to point to MinIO:
   - Type: S3
   - Provider: MinIO
   - Endpoint: `http://192.168.1.100:9000`
   - Access Key / Secret Key
3. User creates vault with MinIO backend
4. VoidGate uploads encrypted blobs to MinIO (on-prem)
5. User has full control: no third-party provider, no data leaves network

### Cost Optimization (Wasabi vs. AWS)

**Trigger**: User wants cheaper storage without egress fees

**Steps**:
1. User analyzes costs:
   - AWS S3: $0.023/GB + egress fees
   - Wasabi: $0.0059/GB, no egress fees
2. User chooses Wasabi for 4x cost savings
3. User configures Rclone with Wasabi credentials
4. User creates vault with Wasabi backend
5. VoidGate uploads to Wasabi (identical workflow, lower cost)

### Decentralized Storage (Storj)

**Trigger**: User wants censorship-resistant, geographically distributed storage

**Steps**:
1. User creates Storj account and configures S3-compatible gateway
2. User configures Rclone with Storj S3 credentials
3. User creates vault with Storj backend
4. VoidGate uploads encrypted blobs to Storj
5. Storj distributes encrypted chunks across global peer network
6. User benefits from decentralization (no single point of failure)

### Multi-Backend Redundancy

**Trigger**: User wants to upload to multiple backends for redundancy (e.g., AWS + Backblaze)

**Steps**:
1. User configures multiple Rclone remotes: `aws-s3`, `backblaze-b2`
2. User creates Rclone union remote combining both:
   - `rclone config create multi-redundant union upstreams="aws-s3:bucket backblaze-b2:bucket"`
3. User selects `multi-redundant` as VoidGate backend
4. VoidGate uploads blobs to both AWS and Backblaze via union remote
5. If one provider fails, user can still access data from other (redundancy)

### Custom Protocol (SFTP Server)

**Trigger**: User has legacy SFTP server and wants to use it for encrypted backups

**Steps**:
1. User configures Rclone SFTP backend:
   - Type: SFTP
   - Host: `sftp.example.com`
   - User / Password or SSH key
2. User creates vault with SFTP backend
3. VoidGate uploads encrypted blobs over SFTP
4. Legacy server stores encrypted blobs (no S3 API required)

## Success Criteria

- User can choose any Rclone-supported backend (70+ providers)
- VoidGate imposes no vendor lock-in (encrypted blobs are portable)
- User can migrate between backends without re-encryption
- User can optimize for cost, privacy, compliance, or decentralization
- Self-hosted backends are fully supported (MinIO, Ceph, SFTP)
- Multi-backend redundancy is possible via Rclone union

## Related Designs

- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Rclone integration, CloudTransport trait, backend-agnostic design
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — Encrypted blobs are portable (no backend-specific encryption)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Random UUID blob names (backend-agnostic identifiers)

## Security Considerations

### Threats Addressed

- **Vendor lock-in**: User can switch providers without exposing data
- **Provider-specific encryption**: VoidGate encrypts before Rclone (no reliance on provider encryption)
- **Censorship**: User can choose decentralized backends (Storj, IPFS) or self-hosted
- **Cost exploitation**: User can switch to cheaper providers without re-work
- **Geographic compliance**: User can choose providers in specific jurisdictions (EU-only, US-only)

### Assumptions

- User has technical expertise to configure Rclone backends
- User validates backend security (HTTPS endpoints, credential management)
- User understands cost implications (egress fees, storage tiers)
- Self-hosted backends are secured by user (firewall, access controls)

### Out of Scope

- **Rclone configuration UI**: VoidGate does not provide GUI for Rclone setup (user must use `rclone config`)
- **Backend health monitoring**: VoidGate does not monitor backend uptime or performance
- **Automatic failover**: If primary backend fails, user must manually switch to backup
- **Cross-backend deduplication**: Redundant uploads store full copies (no deduplication across backends)

## Notes

This use case demonstrates VoidGate's philosophy: **user sovereignty over data and infrastructure**. By using Rclone as an abstraction layer, VoidGate supports:
- 70+ cloud providers (commercial, self-hosted, decentralized)
- 20+ protocols (S3, Azure, WebDAV, SFTP, FTP, HTTP, etc.)
- User-controlled migration (no vendor lock-in)

**Comparison to Competitors**:
- **Dropbox, Google Drive**: Proprietary protocols, no backend choice
- **Tresorit, SpiderOak**: Zero-knowledge but vendor-locked (cannot switch providers)
- **Cryptomator**: Local encryption but relies on cloud provider's native app (not backend-agnostic)
- **VoidGate + Rclone**: Zero-knowledge + BYOC (bring your own cloud)

**Technical Deep Dive**: VoidGate does not interact with cloud APIs directly. Instead:
1. VoidGate encrypts data locally (XChaCha20-Poly1305)
2. VoidGate generates random UUID blob names
3. VoidGate passes encrypted blobs to Rclone via CloudTransport trait
4. Rclone handles protocol-specific uploads (S3 API, WebDAV, SFTP, etc.)
5. This design isolates VoidGate from backend complexity (70+ providers supported with zero VoidGate code changes)

**Future Enhancement**: VoidGate could implement automatic failover or load balancing across multiple backends (currently requires manual Rclone union configuration).

---

**References**:
- Rclone: [Supported Backends](https://rclone.org/#providers)
- Storj: Decentralized cloud storage
- MinIO: High-performance S3-compatible object storage
- Cost Comparison: [Cloud Storage Pricing](https://www.backblaze.com/b2/cloud-storage-pricing.html)
