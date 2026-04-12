---
name: derive-hkdf-key
description: Safely add a new purpose-specific HKDF key to the Arx Runa key derivation tree. Use when a new cryptographic key with a distinct purpose is needed.
---

Follow every step in order. Do not skip the documentation updates — stale key derivation trees cause security review failures.

**Before starting:** Read the current canonical key derivation tree in `docs/architecture/designs/authentication-and-session-management/design.md` to identify all allocated info strings. Every `info` string must be globally unique — reuse causes silent key collision.

## Step 1: Choose an info string

Format: `b"arx-runa-<purpose>"`. It must be unique, descriptive, and not a variation of an existing one. Verify against the canonical source before proceeding.

## Step 2: Define the key type as a named newtype with `ZeroizeOnDrop`

```rust
#[derive(ZeroizeOnDrop)]
pub struct NewPurposeKey(Secret<[u8; 32]>);

impl NewPurposeKey {
    pub fn expose_secret(&self) -> &[u8; 32] { self.0.expose_secret() }
}
```

> **Note:** `master_key` itself uses `Zeroizing<[u8; 32]>` (not `Secret`), because it is a transient intermediate that must be zeroed on all exit paths including early errors. Derived keys that persist in `SessionKeys` use `Secret<[u8; 32]>`. Follow the pattern used by the existing keys in the codebase.

## Step 3: Write the derivation function using `Zeroizing` for the intermediate buffer

```rust
fn derive_new_purpose_key(master_key: &MasterKey) -> Result<NewPurposeKey, KeyDerivationError> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"arx-runa-v1"), master_key.expose_secret());
    let mut key_bytes = Zeroizing::new([0u8; 32]);
    hkdf.expand(b"arx-runa-<purpose>", key_bytes.as_mut())
        .map_err(|_| KeyDerivationError::HkdfExpand)?;
    Ok(NewPurposeKey(Secret::new(*key_bytes)))
    // Zeroizing zeroes the stack buffer here on drop.
}
```

`Zeroizing` is required because `Secret::new(*key_bytes)` copies the bytes — without it, the intermediate `[u8; 32]` remains on the stack until the frame is released.

The fixed salt `b"arx-runa-v1"` is the domain separator used by all vault-key derivations (see crypto design).

## Step 4: Add the new key to `SessionKeys`

Add it in `src-tauri/src/auth/` and derive it alongside the existing keys. The `master_key` must be zeroed at the end of that same scope — it must never be stored.

## Step 5: Update the canonical source

1. Add the new key to `docs/architecture/designs/authentication-and-session-management/design.md` — key derivation tree section
2. Add the new key to `docs/architecture/designs/cryptographic-primitives/design.md` — HKDF-SHA256 Expansion table
3. Update `.claude/rules/auth.md` if needed
4. Run `/copilot-sync` to sync to GitHub instructions

## Step 6: Write a zeroize verification test

Write a zeroize verification test for the new key type (see the `/crypto-roundtrip-test` skill for the pattern).

After all changes, invoke the security-reviewer agent on every modified file.
