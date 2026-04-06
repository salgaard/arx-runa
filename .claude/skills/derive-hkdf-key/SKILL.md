---
name: derive-hkdf-key
description: Safely add a new purpose-specific HKDF key to the Arx Runa key derivation tree. Use when a new cryptographic key with a distinct purpose is needed.
---

Follow every step in order. Do not skip the documentation updates — stale key derivation trees cause security review failures.

**Existing info strings (must not be reused):**

See `docs/architecture/designs/authentication-and-session-management/design.md` for the current canonical key derivation tree.

As of the last update, these info strings are allocated:
- `b"voidgate-key-encryption"` → key_encryption_key (wraps per-file file_keys at rest)
- `b"voidgate-sqlcipher"` → sqlcipher_key
- `b"voidgate-manifest-backup"` → manifest_key

**Before proceeding, verify the current tree in the canonical source to avoid collisions.**

**Step 1: Choose an info string.** Format: `b"voidgate-<purpose>"`. It must be unique, descriptive, and not a variation of an existing one.

**Step 2: Define the key type as a named newtype with `ZeroizeOnDrop`:**
```rust
#[derive(ZeroizeOnDrop)]
pub struct NewPurposeKey(Secret<[u8; 32]>);

impl NewPurposeKey {
    pub fn expose_secret(&self) -> &[u8; 32] { self.0.expose_secret() }
}
```

**Step 3: Write the derivation function using `Zeroizing` for the intermediate buffer:**
```rust
fn derive_new_purpose_key(master_key: &Secret<[u8; 32]>) -> Result<NewPurposeKey, KeyDerivationError> {
    let hkdf = Hkdf::<Sha256>::new(None, master_key.expose_secret());
    let mut key_bytes = Zeroizing::new([0u8; 32]);
    hkdf.expand(b"voidgate-<purpose>", key_bytes.as_mut())
        .map_err(|_| KeyDerivationError::HkdfExpand)?;
    Ok(NewPurposeKey(Secret::new(*key_bytes)))
    // Zeroizing zeroes the stack buffer here on drop.
}
```
`Zeroizing` is required because `Secret::new(*key_bytes)` copies the bytes — without it, the intermediate `[u8; 32]` remains on the stack until the frame is released.

**Step 4: Add the new key to `SessionKeys` in `src-tauri/src/auth/` and derive it alongside the existing keys.** The `master_key` must be zeroed at the end of that same scope — it must never be stored.

**Step 5: Update the canonical source:**
1. Add the new key to `docs/architecture/designs/authentication-and-session-management/design.md` — key derivation tree section
2. Update `.claude/rules/auth.md` if needed
3. Run `/copilot-sync` to sync to GitHub instructions

**Note:** `CLAUDE.md` references the design doc for details — no direct update needed unless the high-level principle changes.

**Step 6: Write a zeroize verification test for the new key type** (see the `crypto-roundtrip-test` skill for the pattern).

After all changes, invoke the security-reviewer agent on every modified file.
