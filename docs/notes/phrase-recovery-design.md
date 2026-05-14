# Phrase Recovery — How It Works

## Overview

Recovery phrase support is opt-in and can be added to an existing vault at any time via **Settings → Set Up Recovery Phrase**. It does not re-encrypt any files; it only adds a wrapped copy of the master key to the vault header.

## Key insight: the master key is the recovery anchor

Every file in the vault has its file key wrapped (AEAD-encrypted) by a KEK derived from the master key:

```
master_key → HKDF → KEK  →  AEAD-wrap(file_key)  →  stored in DB
master_key → HKDF → sqlcipher_key  →  DB encryption
```

The master key itself is never stored anywhere. It is re-derived on every unlock:

```
Argon2id(password [|| key_file_bytes], salt) → master_key
```

Because the master key fully determines the KEK and the sqlcipher key, whoever holds the master key can re-wrap all file keys and rekey the DB to any new credentials.

## Setup (`setup_recovery`)

1. Verifies the caller knows the current credentials (unwraps `vault_identity.wrapped_private_key` as proof).
2. Generates a BIP-39 24-word mnemonic.
3. Derives a *recovery key* from the mnemonic with its own Argon2id salt (stored per-slot):

   ```
   Argon2id(canonicalised_phrase, slot_salt) → recovery_key
   ```

4. Wraps the current master key under the recovery key with AEAD:

   ```
   AEAD-wrap(master_key, recovery_key, AAD="arx-runa recovery v1" || vault_id)
       → wrapped_master_key
   ```

5. Appends a `recovery_slot` to the vault header with `method = "bip39"`, the slot salt, Argon2 params, and `wrapped_master_key`.
6. Uploads the updated header to cloud and persists it locally.
7. Returns the mnemonic once (in a `Zeroizing<String>`); never stored anywhere.

This can happen any time after vault creation. Files already encrypted are unaffected — only the header changes.

## Recovery (`recover_with_phrase`)

1. Parses and canonicalises the user-supplied phrase.
2. Resolves the vault header (local copy if available; cloud download otherwise).
3. For each `bip39` slot in `recovery_slots`:
   - Re-derives the recovery key from the phrase + slot salt.
   - Attempts `AEAD-unwrap(wrapped_master_key, recovery_key, AAD=...)`.
   - On success: the original master key is recovered.
4. Derives the original session keys from the recovered master key → same KEK and sqlcipher key that were used when the vault was created / last rekeyed.
5. Opens the DB with the original sqlcipher key.
6. Derives a **new** master key from the new password (new random salt):

   ```
   Argon2id(new_password, new_salt) → new_master_key → new_KEK, new_sqlcipher_key
   ```

7. For every `file_key_wrapped` row in `nodes` and `vault_identity.wrapped_private_key`:
   - Unwrap with original KEK → file key plaintext.
   - Re-wrap with new KEK → store back.
8. `PRAGMA rekey` to change the SQLcipher encryption to `new_sqlcipher_key`.
9. Updates the header: new argon2 salt, new params, new `key_file_blake3` (Tier 2).
10. Re-wraps a new recovery slot under the **same** phrase so the phrase remains valid after the rekey.
11. Best-effort upload of the updated header to cloud (logged as warning on failure; caller persists locally, pushed on next sync).
12. Installs the new session.

## Recovery entry points

There are two UI entry points that invoke the `recover_with_phrase` ceremony, covering different device states:

### 1. Existing device — `RecoverWithPhrasePage` → `recover_vault_with_phrase`

Used when the vault is already registered on the device (appears in the vault list) but the user has forgotten their password. Reached from **LoginPage → "Forgot password?"**.

- No cloud transport is required.
- The local `vault-header.json` (which contains the recovery slot written by `setup_recovery`) is read from disk and passed directly to the ceremony as `vault_header: Some(h)`.
- The local DB is re-keyed in place — no cloud download needed.
- The updated header is persisted locally immediately; cloud upload is best-effort (pushed on next sync if transport is unavailable).

### 2. New device — `VaultRecoveryPage` (phrase tab) → `recover_vault_from_cloud_with_phrase`

Used when the vault does not exist on the device at all (new PC, replaced device). Reached from **"Recover Vault from Cloud" → "Recovery Phrase" tab**.

- The user supplies the cloud destination config (rclone stanza) + phrase + new password.
- A real `RcloneTransport` is built from the destination config before any ceremony call.
- The vault header is downloaded from cloud via the transport, parsed, and passed to the ceremony as `vault_header: Some(h)`.
- The manifest backup (encrypted DB) is downloaded from cloud via the transport (`db_exists = false` path).
- The vault directory is created locally, the DB is written and re-keyed, and the session is installed.
- The updated header is uploaded to cloud (transport is available at this point).

### Ceremony path summary

| Entry point | `vault_header` | `db_exists` | Header source | DB source | Cloud upload |
|---|---|---|---|---|---|
| `recover_vault_with_phrase` | `Some(local_h)` | `true` | Local disk | Local disk (rekey in place) | Best-effort |
| `recover_vault_from_cloud_with_phrase` | `Some(cloud_h)` | `false` | Cloud download | Cloud download | Yes (transport available) |
| Either (future / direct ceremony call) | `None` | `false` | Cloud download | Cloud download | Yes |

## After recovery

- The local `vault-header.json` is written immediately with the rekeyed header.
- If the cloud upload succeeded, cloud is already consistent.
- If it failed (NoOp transport / offline): the cloud header still has the **old** recovery slot pointing to the **old** master key. The local DB is now under the **new** master key. Once the user authenticates and syncs, the header is pushed. Until then the phrase still works against the old cloud header, but that header's wrapped master key no longer matches the live DB — so a cloud-based recovery attempt against the stale cloud header would fail at the DB rekey step (wrong sqlcipher key). No data loss; the next sync resolves it.

## What the phrase does NOT protect

- The vault header is plaintext (by design — needed to derive the master key before decryption). The phrase only protects the master key embedded in the recovery slot.
- If someone obtains the vault header AND the phrase, they can recover the master key and decrypt all files. Store the phrase offline, separately from the device.
