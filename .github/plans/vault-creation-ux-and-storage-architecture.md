# Vault Creation UX and Storage Architecture Plan

**Status:** Approved design with user decisions captured  
**Target Release:** Align frontend wizard with backend storage architecture and canonical design specs

---

## Problem Statement

The vault creation UX has four issues to resolve:

1. **Design Misalignment** - Current 3-step wizard (Identity → Storage → Review) deviates from canonical design which specifies a flat single-page form with chunk size presets and epoch buffer toggle visible upfront.

2. **Missing Fields** - Chunk size presets (Standard/Documents/Media/Paranoid) and epoch buffer toggle not yet exposed in wizard.

3. **Unclear Storage Architecture** - Frontend StorageProvider enum (Local/S3/B2/Rclone) doesn't map cleanly to backend's 3 destination types (Cloud/ExternalDrive/LocalPath). S3 and B2 are rclone backends, not separate providers.

4. **Cloud Setup Chicken-and-Egg Problem** - Vault creation wizard expects cloud to be pre-configured, but if user can't log in, how does initial cloud setup happen? Design doesn't specify when/how this occurs.

---

## Decisions & Architecture

### 1. Storage Destination Architecture

**Decision: Support 3 destination types with hybrid provider model**

Backend supports:
- **LocalPath** - Direct filesystem storage
- **ExternalDrive** - USB/external drive via Rclone
- **Cloud** - All cloud backends via Rclone (S3, B2, Google Drive, etc.)

Frontend wizard will present:

```
Select destination type:
  ○ Local Filesystem
  ○ External Drive (USB/Network)
  ○ Cloud Storage
    [If Cloud selected]
    ○ Amazon S3
    ○ Backblaze B2
    ○ Google Drive
    ○ Other Rclone Backend
```

**Mapping to backend:**

| Frontend | Backend | Implementation |
|----------|---------|-----------------|
| Local Filesystem | LocalPath | Direct path validation |
| External Drive | ExternalDrive | Rclone (local path) |
| Cloud (S3) | Cloud | Rclone with S3 backend |
| Cloud (B2) | Cloud | Rclone with B2 backend |
| Cloud (Google Drive) | Cloud | Rclone with Google Drive backend |
| Cloud (Custom Rclone) | Cloud | Rclone with custom remote config |

**SQLCipher Schema Context:**
- `destination_sessions` table stores one DestinationSession per configured destination (Cloud, ExternalDrive, or LocalPath)
- `rclone_remote_name` and `rclone_config_blob` store rclone configuration for the remote
- Credentials encrypted in SQLCipher database

**manifest_meta table** stores vault-level configuration:
```
('vault_id', '<uuid>')                   -- Vault identity
('chunk_size_bytes', '<bytes>')          -- Immutable after creation
('epoch_buffer_enabled', 'true|false')   -- User opt-in at vault creation
('schema_version', '1')                  -- SQLCipher schema version
('snapshot_counter', '0')                -- Incremented on each sync
```

---

### 2. Vault Creation Form Structure

**Decision: Flatten wizard to single-page form with sectioned layout**

Replace 3-step wizard with single page organized into logical sections:

```
┌─ IDENTITY SECTION ─────────────────┐
│ Vault Name          [________________]    |
│ Key File Location   [Browse...      ]    |
│ (Advanced)                             |
│ ├─ Chunk Size       [Standard ▼]        |
│ │                   Standard (4 MiB) - default
│ │                   Documents (2 MiB) - smaller files
│ │                   Media (16 MiB) - large media
│ │                   Paranoid (1 MiB) - maximum safety
│ │                   Custom: [________] bytes
│ │                                        |
│ │ ☐ Keep version history during        |
│ │   re-encryption (epoch buffer)       |
│ │   [?] ← Help text                    |
│ └────────────────────────────────┘  |
├─ DESTINATION SECTION ──────────────┤
│ Where to store encrypted files?       |
│ ○ Local Filesystem                   |
│   Path: [_____________________]      |
│ ○ External Drive                     |
│   USB/Network Path: [__________]     |
│ ○ Cloud Storage                      |
│   Provider: [Amazon S3       ▼]     |
│   [Cloud-specific fields]            |
│ (Test Connection)                    |
├─ REVIEW & CREATE ────────────────┤
│ Summary:                              |
│ • Vault: My Files                    |
│ • Storage: Local Filesystem          |
│ • Chunk Size: 4 MiB                  |
│ [Create Vault]  [Cancel]             |
└────────────────────────────────────┘
```

**Rationale:**
- **Single page** matches canonical design expectation
- **Sectioned layout** keeps form readable without multi-step overhead
- **Advanced toggle** hides chunk size/epoch buffer initially, available for power users
- **Inline cloud config** for S3/B2 eliminates extra steps

---

### 3. Chunk Size Presets

**Decision: Offer 4 presets + custom option**

Configuration options (stored in `manifest_meta.chunk_size_bytes`):

| Preset | Bytes | Recommended For | Tradeoff |
|--------|-------|-----------------|----------|
| **Standard** (default) | 4 MiB (4194304) | General use | Good balance |
| **Documents** | 2 MiB (2097152) | Lots of small files | More chunks = more metadata |
| **Media** | 16 MiB (16777216) | Large photos/videos | Fewer chunks, larger uploads |
| **Paranoid** | 1 MiB (1048576) | Maximum resilience | Most chunks = slowest uploads |
| **Custom** | User input | Advanced users | Validate: 256 KiB to 256 MiB |

**Frontend implementation:**
- Radio button group (5 options: 4 presets + Custom)
- Custom input field enabled only when "Custom" selected
- Validation on save: `256 * 1024 <= bytes <= 256 * 1024 * 1024`
- Default: Standard (4 MiB)

---

### 4. Epoch Buffer Toggle

**Decision: Expose with explanation; default OFF**

Epoch buffer keeps old file versions when re-encrypting due to key rotation.

**Frontend implementation:**
- Checkbox: "☐ Keep version history during re-encryption"
- Inline help text: "When you rotate encryption keys, the system can optionally preserve old file versions before re-encryption. Requires additional storage space."
- Default: OFF (unchecked)
- Stored in `manifest_meta.epoch_buffer_enabled`

---

### 5. Cloud Setup Timing

**Decision: Two-part setup flow**

**At app startup (new users):**
1. App detects missing cloud configuration
2. Shows **Cloud Setup Wizard** (separate from vault wizard)
   - Choose cloud provider (S3, B2, Google Drive, etc.)
   - Enter credentials / authorize OAuth
   - Validate connectivity
   - Save to `cloud-config.json` (backend creates if missing)

**At vault creation (existing users):**
1. Cloud is already configured (or user has local-only setup)
2. Vault wizard assumes cloud is ready if Cloud destination selected
3. If cloud not configured and user selects Cloud, wizard shows inline error: "Please configure your cloud account first in Settings"

**Rationale:**
- Separates cloud setup (infrastructure) from vault setup (application data structure)
- Unblocks users with only local storage
- Allows vault-specific cloud choices later (S3 vs B2, different buckets, etc.)

---

### 6. Design Deviations & Updates

**Current canonical design:** `docs/architecture/designs/tauri-ipc-and-frontend/design.md` (Phase 6.3)

**Deviations from design:**

| Aspect | Design Specifies | Our Implementation | Reasoning |
|--------|------------------|--------------------|-----------|
| Form layout | Flat single page | Single page, sectioned | Matches intent; improved scannability |
| Chunk size | Presets in form | Presets + Custom | Matches intent; added flexibility |
| Epoch buffer | Toggle visible | Toggle visible | Matches intent exactly |
| Cloud setup | Not specified | Separate app startup | Solves chicken-and-egg problem |
| Destination types | Not detailed | 3 types (Local/External/Cloud) | Maps to backend schema exactly |

**Design update required:**
- `design.md` Phase 6.3 should document:
  1. Cloud setup happens at app startup for new users
  2. Destination type selector (Local/External/Cloud with sub-provider choice)
  3. Chunk size presets rationale
  4. Epoch buffer explanation

---

### 7. Rclone Integration

**Backend rclone infrastructure (already exists):**
- `src-tauri/src/storage/cloud/rclone.rs` - Rclone transport layer
- `src-tauri/src/storage/cloud/wizard.rs` - Rclone config builder for S3/B2/Google Drive
- `src-tauri/src/storage/cloud/destination_session.rs` - Persists rclone remotes in SQLCipher
- `rclone_config_blob` stored encrypted in `destination_sessions` table

**Frontend integration:**
1. **Cloud provider form fields** - S3 (bucket, region, credentials), B2 (account, key), Google Drive (OAuth)
2. **Rclone config generation** - Backend `validate_storage_destination()` builds rclone remote, tests connectivity
3. **Credential security** - IPC passes credentials → backend encrypts → stored in SQLCipher

**No rclone binaries required** - Backend uses rclone config programmatically via `rclone.rs` transport

---

## Implementation Roadmap

### Phase 1: Form Restructuring (Priority 1)
- [ ] Replace 3-step wizard with single-page form
- [ ] Add "Advanced" collapsible section for chunk size + epoch buffer
- [ ] Implement sectioned layout (Identity / Destination / Review)
- [ ] Update validation flow

### Phase 2: Chunk Size & Epoch Buffer (Priority 1)
- [ ] Add chunk size preset radio group with custom input
- [ ] Add epoch buffer toggle with explanation
- [ ] Validate chunk size input (256 KiB to 256 MiB)
- [ ] Pass to backend `create_vault` command

### Phase 3: Storage Provider Selection (Priority 2)
- [ ] Replace StorageProvider enum with DestinationType enum (Local/External/Cloud)
- [ ] If Cloud selected, show provider picker (S3/B2/Google Drive/Rclone)
- [ ] Implement inline cloud config fields for each provider
- [ ] Test connection button for cloud destinations

### Phase 4: Cloud Setup Wizard (Priority 2)
- [ ] Detect if cloud configured at app startup
- [ ] Create separate cloud setup flow (modal/page)
- [ ] Allow users to skip cloud for local-only setups
- [ ] Show friendly error if vault requires Cloud but it's not configured

### Phase 5: Backend Integration (Priority 2)
- [ ] Update `create_vault` to accept chunk_size_bytes parameter
- [ ] Update `create_vault` to accept epoch_buffer_enabled parameter
- [ ] Ensure rclone config generation matches frontend provider selection
- [ ] Test storage validation for all destination types

### Phase 6: Testing & Documentation (Priority 3)
- [ ] End-to-end test: Local filesystem vault creation
- [ ] End-to-end test: External drive vault creation (mock)
- [ ] End-to-end test: Cloud vault creation (S3)
- [ ] Update design.md with cloud setup timing and destination types
- [ ] Add comments documenting deviations from original design

---

## Technical Details

### Backend Changes Required

**File: `src-tauri/src/ui/auth_commands.rs`**
- Add `chunk_size_bytes: u64` parameter to `create_vault` command
- Add `epoch_buffer_enabled: bool` parameter to `create_vault` command
- Pass to vault ceremony: `create_vault_ceremony(..., chunk_size_bytes, epoch_buffer_enabled, ...)`

**File: `src-tauri/src/storage/schema.rs`**
- Verify manifest_meta inserts for:
  - `('chunk_size_bytes', '<bytes>')`
  - `('epoch_buffer_enabled', 'true|false')`
- No schema changes needed (already in canonical schema)

**File: `src-tauri/src/storage/cloud/wizard.rs`**
- Ensure `RcloneRemoteBuilder` handles:
  - S3: bucket, region, access_key, secret_key
  - B2: account_id, application_key
  - Google Drive: OAuth token refresh
  - Custom: raw rclone config

### Frontend Changes Required

**File: `src/auth.rs`**
- Refactor `VaultCreationPage` from 3-step wizard to single page
- Add sectioned layout with subsections
- Add Advanced collapsible section

**File: `src/components/chunk_size_selector.rs`** (new)
- Radio button group for presets + custom
- Input field for custom bytes
- Validation and error display

**File: `src/components/epoch_buffer_toggle.rs`** (new)
- Checkbox with inline help text
- Explanation of what epoch buffer does

**File: `src/components/destination_selector.rs`** (new)
- Replace `storage_selector.rs`
- DestinationType enum (Local/ExternalDrive/Cloud)
- Conditional rendering of provider picker if Cloud
- Provider-specific form fields (S3, B2, Google Drive)

**File: `src/cloud_setup_modal.rs`** (new)
- Cloud setup wizard (triggered at app startup or from Settings)
- Provider selection, credential entry, validation

### Configuration Files

**Backend: No changes**
- Cloud setup wizard generates `cloud-config.json` at first app startup (when user runs app)

**Frontend: No changes**
- Leptos component structure already in place

---

## Testing Strategy

### Unit Tests
- [ ] Chunk size validation (min 256 KiB, max 256 MiB)
- [ ] Epoch buffer toggle serialization
- [ ] DestinationType to backend mapping

### Integration Tests
- [ ] Local filesystem vault creation with chunk size preset
- [ ] External drive vault creation (mock)
- [ ] Cloud vault creation with S3 credentials validation
- [ ] Error handling for invalid credentials

### End-to-End Tests
- [ ] Full vault creation flow: name + local destination + Standard chunk size
- [ ] Full vault creation flow: name + cloud destination (S3) + Media chunk size + epoch buffer enabled
- [ ] Cloud setup wizard at app startup
- [ ] Skip cloud setup, create local vault, add cloud later

---

## Success Criteria

- [ ] Vault creation form matches canonical design (single page, sectioned)
- [ ] Chunk size presets and custom option working
- [ ] Epoch buffer toggle working and documented
- [ ] Local, External, and Cloud destinations all supported
- [ ] Cloud setup at app startup for new users
- [ ] All fields persist to SQLCipher correctly
- [ ] No design contradictions (deviations documented)
- [ ] Frontend and backend compile without errors
- [ ] Toast notifications for success/failure
- [ ] User-friendly error messages (no Rust errors leaked to UI)

---

## Dependencies & Constraints

1. **SQLCipher schema** - Must use existing `manifest_meta` table; no migration needed
2. **Rclone backend** - Already exists; no new binaries or infrastructure needed
3. **Toast system** - Already implemented in Phase A
4. **IPC commands** - Must extend existing `create_vault` command with new parameters

---

## Open Questions & Deferrals

1. **Epoch buffer storage impact** - Design doesn't quantify extra space needed; document in help text as "additional storage space" for now
2. **External drive detection** - How to auto-detect USB drives on Windows/macOS/Linux? Defer to Phase 2; for MVP, user provides path
3. **Multiple cloud accounts** - Can user add multiple S3 buckets or B2 accounts? Design allows via destination_sessions; UI can defer to later
4. **Rclone custom config** - Should users be able to paste raw rclone config, or only use presets? Defer to Phase 2; MVP uses presets only

---

## Sign-Off

**Plan created by:** Copilot  
**User approval:** [Pending]  
**Date:** Current session  

This plan is source-of-truth for all implementation work and design updates. Any deviations should be approved before code changes.
