# UC-IND-003: Privacy-Focused Photo Storage

**Category**: Individual Privacy

**Status**: Active

---

## Overview

A user wants to store personal photos and videos in cloud storage without exposing image content, faces, locations (EXIF metadata), or file sizes to the cloud provider.

## Actors

- **Primary Actor**: Individual user with private photos/videos
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system, USB key file

## Preconditions

- User has VoidGate installed and vault configured
- User has photos/videos on local device (e.g., from phone, camera)
- User has USB key file and password for vault unlock
- User has sufficient cloud storage space (photos require significant storage)

## Main Flow

1. User unlocks vault with password + USB key file
2. User selects photo library or video folder for backup
3. VoidGate processes each photo/video:
   - Strips EXIF metadata from files (optional privacy enhancement)
   - Reads file into memory in chunks
   - Encrypts with XChaCha20-Poly1305 in 4 MiB fixed-size chunks
   - Pads final chunk to 4 MiB (hides exact file size)
4. VoidGate uploads encrypted chunks with random UUID blob names
5. VoidGate stores file metadata (user-provided tags, not EXIF) in encrypted manifest
6. User adds optional privacy-safe tags (e.g., "vacation 2025" without location)
7. VoidGate pushes updated manifest to cloud
8. User locks vault
9. Later, user wants to view a photo:
10. User unlocks vault
11. User browses manifest for tagged photos
12. User selects photo to view
13. VoidGate downloads encrypted chunks from cloud
14. VoidGate decrypts chunks and displays photo (in-app viewer) or exports to temp location (external viewer)
15. User views photo
16. User locks vault when done

## Alternate Flows

### Streaming Large Video

**Trigger**: User wants to play video without downloading entire file

**Steps**:
1. User selects video file in manifest
2. VoidGate downloads first few chunks
3. VoidGate decrypts and streams to video player (in-app or via temp file for external player)
4. VoidGate downloads subsequent chunks as needed (progressive streaming)
5. User watches video with minimal latency
6. Flow continues until video ends or user stops playback

### Photo Sharing (External)

**Trigger**: User wants to share specific photo with friend (outside VoidGate)

**Steps**:
1. User selects photo in manifest
2. User chooses "Export Decrypted Copy"
3. VoidGate warns: "Exported file will be unencrypted"
4. User confirms and selects export location
5. VoidGate downloads, decrypts, and writes plaintext photo to disk
6. User shares exported file via email/messaging
7. VoidGate does not re-encrypt exported copy (user's responsibility)

### EXIF Metadata Preservation

**Trigger**: User wants to keep original EXIF data (camera settings, timestamps)

**Steps**:
1. User configures VoidGate: "Preserve EXIF metadata (encrypted)"
2. VoidGate stores EXIF in encrypted manifest (not stripped)
3. When viewing photo, VoidGate displays EXIF from manifest
4. Cloud provider never sees EXIF (encrypted within manifest)
5. Flow continues with EXIF preserved

### Duplicate Photo Detection

**Trigger**: User uploads same photo multiple times (e.g., from multiple devices)

**Steps**:
1. VoidGate computes content hash (BLAKE3 on plaintext before encryption)
2. VoidGate checks manifest for existing hash
3. If duplicate found: VoidGate prompts "Duplicate detected. Skip upload?"
4. User selects: skip, overwrite, or keep both
5. Flow continues based on user choice (saves bandwidth and storage)

## Success Criteria

- Photos and videos are encrypted before upload (cloud never sees image content)
- File sizes are padded to 4 MiB boundaries (exact size hidden)
- EXIF metadata (GPS, camera model, timestamps) is either stripped or encrypted
- Blob names are random UUIDs (cloud cannot infer album structure or order)
- User can view photos in-memory without writing plaintext to disk
- Optional streaming for large videos (progressive decryption)

## Related Designs

- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 encryption, BLAKE3 content hashing for deduplication
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — 4 MiB fixed-size chunks, zero-padding, SQLCipher manifest for metadata storage
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Random UUID blob names, encrypted manifest backup
- [Tauri IPC & Frontend](../architecture/designs/tauri-ipc-and-frontend/design.md) — In-memory image rendering, streaming video playback without temp files

## Security Considerations

### Threats Addressed

- **Cloud provider image analysis**: Cloud cannot run facial recognition, object detection, or content scanning on encrypted blobs
- **EXIF metadata leakage**: GPS locations, camera models, timestamps not exposed to cloud
- **File size inference**: Fixed 4 MiB padding prevents cloud from distinguishing thumbnails vs. full-resolution images
- **Album structure inference**: Random UUID names prevent cloud from reconstructing photo order or albums

### Assumptions

- User's local device is trusted during photo viewing (decrypted images displayed in RAM)
- User does not export and share decrypted photos insecurely
- Cloud provider does not delete blobs (user must verify backup integrity)
- Photos are not extremely large (>4 GiB) — chunking scales but may be slow for 4K video

### Out of Scope

- Client-side photo organization UI (tagging, albums) — basic manifest search only
- Thumbnail generation (would require decryption, increases local processing)
- Deduplication across users (single-user vault, no cross-user dedup)
- Face recognition or AI features (privacy-focused, no cloud-based ML)

## Notes

This use case is particularly relevant given cloud providers' use of AI for content scanning. Many users upload photos to Google Photos or iCloud, which perform facial recognition and object detection — VoidGate prevents this by design.

**Privacy Trade-off**: VoidGate does not support client-side thumbnail generation or AI organization features (searching by faces, objects). Users must manually tag photos or rely on filename-based search. This is an intentional privacy choice.

**Performance Note**: 4 MiB chunks work well for photos (1-10 MiB typical) but may require many chunks for 4K video (1 GB+ files). Future optimization could use adaptive chunk sizes or streaming-friendly formats.

---

**References**:
- EXIF metadata privacy concerns: [EXIF Tool Documentation](https://exiftool.org/)
- Cloud photo analysis: [Google Photos Privacy Policy](https://policies.google.com/privacy)
