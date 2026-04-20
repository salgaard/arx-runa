# Rust patterns reference

On-demand reference. Load when scaffolding new Rust types or functions.

## Newtype pattern
Wrap primitive types to add semantic meaning and prevent accidental misuse.
Zero runtime cost — the wrapper is erased at compile time.
```rust
/// Represents a chunk's position within a file (0-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkIndex(u32);

impl ChunkIndex {
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}
```
Use for: `FileId`, `ChunkIndex`, `NodeId`, `VaultId`, `BlobName`, `FileKey`,
`KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `ContactId`.

## RAII guards and ZeroizeOnDrop
Resources that require cleanup on scope exit use RAII. Sensitive key types
implement `ZeroizeOnDrop` and wrap contents in `SecretBox<[u8; 32]>`:
```rust
use secrecy::SecretBox;
use zeroize::ZeroizeOnDrop;

#[derive(ZeroizeOnDrop)]
pub struct FileKey(SecretBox<[u8; 32]>);
```
Prefer `SecretBox::init_with_mut(...)` or `SecretBox::new(Box::new(bytes))`
for secret initialization rather than transient unprotected stack copies.
Use for: all key types, database connections, file locks, session handles.

## Builder pattern
For structs with many optional fields or where construction has side effects:
```rust
impl VaultConfig {
    pub fn builder() -> VaultConfigBuilder {
        VaultConfigBuilder::default()
    }
}

#[derive(Default)]
pub struct VaultConfigBuilder {
    timeout_seconds: Option<u64>,
    verify_after_write: Option<bool>,
}

impl VaultConfigBuilder {
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    pub fn build(self) -> VaultConfig {
        VaultConfig {
            timeout_seconds: self.timeout_seconds.unwrap_or(900),
            verify_after_write: self.verify_after_write.unwrap_or(true),
        }
    }
}
```
Use for: `VaultConfig`, `SyncOptions`, complex configuration structs.

## Borrowed types in signatures
Prefer `&str` over `&String`, `&[u8]` over `&Vec<u8>`:
```rust
// Good — accepts String, &str, literals, slices
fn compute_checksum(data: &[u8]) -> Blake3Hash;
fn search_filename(query: &str) -> Option<NodeId>;

// Avoid — unnecessarily restrictive
fn compute_checksum(data: &Vec<u8>) -> Blake3Hash;
fn search_filename(query: &String) -> Option<NodeId>;
```

## mem::take for ownership transfer
Move owned values out of structs without cloning:
```rust
use std::mem;

fn rotate_session_keys(session: &mut Session, new_keys: SessionKeys) {
    let old_keys = mem::take(&mut session.keys);
    drop(old_keys); // ZeroizeOnDrop triggers
    session.keys = new_keys;
}
```
Use for: key rotation, session replacement, state machine transitions.

## Return consumed argument on error
If a fallible function moves an argument, return it in the error for retry:
```rust
pub struct EncryptError {
    pub source: std::io::Error,
    pub plaintext: Vec<u8>,  // Return the consumed data
}

pub fn encrypt_chunk(plaintext: Vec<u8>, key: &FileKey) -> Result<Vec<u8>, EncryptError> {
    match do_encrypt(&plaintext, key) {
        Ok(ciphertext) => Ok(ciphertext),
        Err(e) => Err(EncryptError { source: e, plaintext }),
    }
}

// Caller can retry without re-reading the file:
let ciphertext = match encrypt_chunk(data, &key) {
    Ok(ct) => ct,
    Err(EncryptError { plaintext, .. }) => {
        encrypt_chunk_with_fallback(plaintext, &backup_key)?
    }
};
```
Use for: encryption with retry, network upload failures, transient errors.

## Pass variables to closure with rebinding
Control exactly what closures capture using a scoped block:
```rust
use std::sync::Arc;

let shared_key = Arc::new(encryption_key);
let chunk_data = get_chunk();
let file_id = "abc123";

let encrypt_task = {
    let key = Arc::clone(&shared_key);  // Clone the Arc (cheap)
    let file_id = file_id.to_owned();   // Clone to owned String
    // chunk_data will be moved
    move || {
        encrypt_chunk(chunk_data, &key, &file_id)
    }
};

// shared_key still usable here; chunk_data has been moved
```
Use for: async tasks, spawned threads, callbacks with controlled capture.

## Temporary mutability
After setup, rebind to immutable so compiler enforces no further mutation:
```rust
// Using nested block
let sorted_chunks = {
    let mut chunks = fetch_chunks()?;
    chunks.sort_by_key(|c| c.index);
    chunks.dedup();
    chunks  // Returned as owned, bound immutably
};

// Using variable rebinding
let mut data = decrypt_chunks()?;
data.sort_by_key(|c| c.index);
let data = data;  // Now immutable — compiler prevents accidental mutation
```
Use for: initialization that requires mutation, then read-only access.

## #[non_exhaustive] for forward compatibility
Mark public enums/structs so external code must handle future additions:
```rust
#[non_exhaustive]
pub enum CryptoError {
    InvalidKey,
    DecryptionFailed,
    IntegrityCheckFailed,
    // Future: can add variants without breaking downstream
}

// External code MUST use wildcard:
match error {
    CryptoError::InvalidKey => { /* ... */ }
    CryptoError::DecryptionFailed => { /* ... */ }
    CryptoError::IntegrityCheckFailed => { /* ... */ }
    _ => { /* handle unknown future variants */ }
}
```
Use for: public error types, vault header structs, anything that may evolve.

## On-stack dynamic dispatch
Avoid heap allocation for trait objects when both branches return same trait:
```rust
use std::io::{self, Read};

fn read_from_source(use_stdin: bool) -> io::Result<Vec<u8>> {
    let readable: &mut dyn Read = if use_stdin {
        &mut io::stdin()
    } else {
        &mut std::fs::File::open("config.toml")?
    };

    let mut buffer = Vec::new();
    readable.read_to_end(&mut buffer)?;
    Ok(buffer)
}
// No Box allocation — both branches live on stack
```
Use for: selecting between input sources, key sources, transport backends.

## Contain unsafe in small modules
Isolate unsafe code in minimal submodules with safe wrappers:
```rust
// src/memory/platform/unix.rs — the unsafe boundary
pub unsafe fn lock_memory(ptr: *const u8, len: usize) -> Result<(), Error> {
    // SAFETY: Caller ensures ptr is valid for len bytes and will remain
    // allocated for the lifetime of the lock.
    let result = libc::mlock(ptr as *const libc::c_void, len);
    if result == 0 { Ok(()) } else { Err(Error::MlockFailed) }
}

// src/memory/secure_buffer.rs — safe wrapper
pub struct SecureBuffer {
    data: Vec<u8>,
    locked: bool,
}

impl SecureBuffer {
    pub fn new(size: usize) -> Result<Self, Error> {
        let data = vec![0u8; size];
        // SAFETY: data is valid and will live as long as SecureBuffer
        unsafe { platform::lock_memory(data.as_ptr(), data.len())? };
        Ok(Self { data, locked: true })
    }
}

impl Drop for SecureBuffer {
    fn drop(&mut self) {
        self.data.zeroize();
        if self.locked {
            // SAFETY: we locked this memory in new()
            unsafe { platform::unlock_memory(self.data.as_ptr(), self.data.len()).ok() };
        }
    }
}
```
Use for: mlock/VirtualLock, platform-specific APIs, FFI boundaries.

## Custom traits for complex bounds
Simplify repetitive trait bounds by introducing a named trait:
```rust
// Verbose, repeated everywhere
fn process<T: Read + Seek + Send + 'static>(source: T) { ... }
fn upload<T: Read + Seek + Send + 'static>(source: T) { ... }

// Named abstraction
trait StreamSource: Read + Seek + Send + 'static {}
impl<T: Read + Seek + Send + 'static> StreamSource for T {}

fn process(source: impl StreamSource) { ... }
fn upload(source: impl StreamSource) { ... }
```

## Trait boundaries for external dependencies
Modules depend on traits, not concrete types. Enables mock-based testing:
```rust
pub trait KeySource: Send + Sync {
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError>;
}

// Production implementation
pub struct FileKeySource { path: PathBuf }
impl KeySource for FileKeySource { ... }

// Test implementation
pub struct MockKeySource { bytes: [u8; 32] }
impl KeySource for MockKeySource { ... }
```
Defined traits: `CloudTransport`, `KeySource`, `MetadataStore`, `DeviceMonitor`.

## Default trait for partial initialisation
Use `#[derive(Default)]` and struct update syntax:
```rust
#[derive(Default)]
struct UploadOptions {
    verify_checksum: bool,
    retry_count: u32,
}

let options = UploadOptions {
    verify_checksum: true,
    ..Default::default()
};
```
