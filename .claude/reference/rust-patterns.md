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
implement `ZeroizeOnDrop` and wrap contents in `Secret<T>`:
```rust
use secrecy::Secret;
use zeroize::ZeroizeOnDrop;

#[derive(ZeroizeOnDrop)]
pub struct FileKey(Secret<[u8; 32]>);
```
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
