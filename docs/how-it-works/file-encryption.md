# How Files Are Encrypted

When you add a file to Arx Runa, it never reaches the cloud in recognisable form. By the time the first byte leaves your device, the file has been stripped of hidden metadata, split into uniform chunks, padded to an identical size, and encrypted under a key that exists nowhere outside your vault. Here is what happens at each step — and why.

## Stripping hidden metadata

Before encryption begins, Arx Runa strips [EXIF](../guides/glossary.md#exif), XMP, and IPTC metadata from media files. A photo from your phone carries GPS coordinates, camera model, lens settings, and timestamps alongside the image itself — information you may not intend to archive. Arx Runa removes all of it in memory before any further processing. Your original file on disk is never modified; the stripped copy is what enters the encryption pipeline. When you later export a file, the exported copy will also be free of that embedded metadata.

## Splitting into chunks

The clean file is split into fixed-size chunks — 4 MiB by default, though you choose the size once at vault creation and it applies to every file thereafter. Chunking serves three purposes: it enables streaming so Arx Runa never holds an entire file in memory, it allows partial download when you only need part of a large file, and it keeps memory use bounded regardless of how big the file is.

## Padding every chunk to the same size

The final chunk of most files is shorter than the full chunk size. Rather than uploading a shorter blob — which would let an observer infer the file's true size — Arx Runa zero-pads the last chunk to the full chunk size before encryption. Every blob the cloud receives is identically sized, whether it contains 1 byte of real data or 4 MiB. The actual file size is stored inside your encrypted manifest database; the cloud learns only how many blobs exist, nothing more.

## A unique key for every file

When you add a file, Arx Runa generates a fresh random 256-bit key just for that file. This key is immediately *wrapped* — encrypted using the `key_encryption_key` derived from your master key — and stored in the manifest. The raw file key never touches disk and is zeroed from memory as soon as encryption is complete.

Because each file has its own independent key, the exposure or rotation of one file's key has no effect on any other file. Rekeying is surgical, not vault-wide.

## Encrypting each chunk

Each chunk is encrypted with [XChaCha20-Poly1305](../guides/glossary.md#xchacha20-poly1305), an authenticated encryption scheme. The cipher generates a fresh 192-bit random nonce for every single chunk — with a nonce space this large, the probability of two chunks ever sharing a nonce is negligible across any realistic number of files.

Alongside the nonce, each encryption operation takes in *associated authenticated data* (AAD) that binds the ciphertext to both the file's unique identity and the chunk's position in the sequence. This means a chunk cannot be silently reordered or transplanted from another file — if the file or position doesn't match, the authentication tag will fail and decryption is rejected before any data is returned.

The wire format stored for each chunk is:

```
[ 24-byte nonce | ciphertext | 16-byte Poly1305 tag ]
```

The 16-byte tag is what makes tampering detectable. Flip a single bit in transit or storage and decryption fails cleanly.

```mermaid
sequenceDiagram
    participant Caller
    participant encrypt_chunk
    participant CSPRNG
    participant XChaCha20Poly1305

    Caller->>encrypt_chunk: plaintext, file_key, file_id, chunk_index
    encrypt_chunk->>CSPRNG: generate_nonce()
    CSPRNG-->>encrypt_chunk: nonce (24 bytes)
    encrypt_chunk->>encrypt_chunk: construct AAD = file_id || chunk_index
    encrypt_chunk->>XChaCha20Poly1305: encrypt_in_place_detached(nonce, aad, plaintext)
    XChaCha20Poly1305-->>encrypt_chunk: tag (16 bytes)
    encrypt_chunk->>encrypt_chunk: assemble [nonce | ciphertext | tag]
    encrypt_chunk-->>Caller: blob
```

## Integrity check on the encrypted blob

After encryption, Arx Runa computes a [BLAKE3](../guides/glossary.md#blake3) checksum over the encrypted blob. This checksum is recorded in the manifest alongside the chunk record. When a chunk is downloaded, the checksum is verified *before* decryption begins — so bit rot or storage corruption is caught immediately, without any decryption key being exercised against corrupt data.

## The manifest

The manifest is a [SQLCipher](../guides/glossary.md#sqlcipher) database encrypted with its own key derived independently from the master key. It holds the mapping from your file paths and directory structure to chunk records — including chunk positions, blob identifiers, BLAKE3 checksums, and the wrapped file keys. Without the manifest, the cloud blobs are an anonymous, unordered collection of identically sized ciphertext. The manifest is also backed up to the cloud in encrypted form so you can restore it on a new device.

## The full pipeline

```mermaid
flowchart TD
    subgraph ENCRYPT ["Encrypt Path"]
        E1["Source file<br/>(streaming)"]:::io
        E2["Strip EXIF metadata<br/>(in memory only)"]:::proc
        E3["Read chunk_size bytes<br/>(zero-pad if last chunk)"]:::proc
        E4["encrypt_chunk<br/>(file_key, AAD = file_id || chunk_index)"]:::crypto
        E5["[24B nonce | ciphertext | 16B tag]<br/>wire_blob"]:::data
        E6["blake3::hash(wire_blob)<br/>→ blake3_checksum"]:::proc
        E7["Write to staging/{uuid}.blob"]:::io
        E8["ChunkRecord<br/>(chunk_index, blob_name, blake3_checksum)"]:::data
        E9["Insert node + chunks<br/>(SQLCipher transaction)"]:::db
    end

    subgraph KEYS ["Key Lifecycle"]
        K1["Generate file_key<br/>(256-bit CSPRNG)"]:::crypto
        K2["Wrap: encrypt(file_key, key_encryption_key)<br/>→ file_key_wrapped"]:::crypto
        K3["Store file_key_wrapped<br/>in manifest"]:::db
        K4["Zeroize file_key<br/>after use"]:::crypto
    end

    K1 --> K2 --> K3
    E1 --> E2 --> E3 --> E4
    K1 -.->|file_key| E4
    E4 --> E5 --> E6 --> E7 --> E8 --> E9 --> K4

    classDef io fill:#16a34a,stroke:#166534,color:#fff
    classDef proc fill:#2563eb,stroke:#1e40af,color:#fff
    classDef crypto fill:#dc2626,stroke:#991b1b,color:#fff
    classDef data fill:#9333ea,stroke:#6b21a8,color:#fff
    classDef db fill:#d97706,stroke:#92400e,color:#fff
```
