# Zero-Trace Manual Verification

How to manually verify the application upholds the Zero-Trace principle. Covers all claims: no plaintext on disk, keys zeroized on lock, no sensitive data in IPC/logs, no persistent frontend state.

---

## 1. Frontend storage — no persistent browser state

Open the Tauri app, unlock a vault, browse some files. Then:

1. Open DevTools inside Tauri: in dev mode `Ctrl+Shift+I`, or add `"devtools": true` to `tauri.conf.json`
2. Go to **Application** tab and check:
   - `localStorage` → should be empty
   - `sessionStorage` → should be empty
   - `IndexedDB` → should have no Arx Runa entries
   - `Cookies` → none
3. Lock the vault. Verify all vault-related reactive state disappears from the UI (file list clears, vault name clears, etc.)

---

## 2. No plaintext disk writes during operation

Use **Sysinternals Process Monitor** (`procmon`):

1. Launch Procmon, add filter: `Process Name is arx-runa.exe` + `Operation is WriteFile`
2. Unlock vault, upload a file, download a file, lock vault
3. Inspect every write — all writes outside the encrypted vault folder are suspect
4. Specifically check for writes to:
   - `%TEMP%`
   - `%LOCALAPPDATA%\Temp`
   - Windows thumbnail cache: `%LOCALAPPDATA%\Microsoft\Windows\Explorer\`
   - Any `.tmp` files anywhere

Expected: writes only to the vault's cloud/local storage path, and only encrypted blobs.

---

## 3. No sensitive data in logs

Run the app with logging enabled:

```powershell
$env:RUST_LOG = "debug"
./arx-runa.exe 2>&1 | Tee-Object -FilePath session.log
```

After a full session (create vault, unlock, upload, lock), search the log:

```powershell
Select-String -Path session.log -Pattern "password|master_key|file_key|sqlcipher|secret|key_enc"
```

Expected: no matches containing actual values — only operational messages.

---

## 4. IPC responses contain no key material

In DevTools console, intercept IPC responses:

```javascript
const orig = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = async (cmd, ...args) => {
  const result = await orig(cmd, ...args);
  console.log(cmd, JSON.stringify(result));
  return result;
};
```

Then perform: authenticate, list files, download a file, trigger an error. Inspect every logged response for key material, internal paths, or stack traces.

---

## 5. Memory cleared on lock (key zeroization)

Use **Process Hacker** (free):

1. Unlock the vault with a known password (e.g., `TestPassword123`)
2. In Process Hacker: right-click the process → **Properties** → **Memory** → search memory for the password string
3. It should be found (in memory while active — expected)
4. Lock the vault
5. Search memory again for the same string
6. Expected: not found

---

## 6. Keys not in pagefile (mlock verification)

Indirect check via working set size:

```powershell
Get-Process arx-runa | Select-Object WorkingSet, PagedMemorySize, NonpagedSystemMemorySize
```

Direct verification requires reading `C:\pagefile.sys` raw (e.g., with WinHex or Autopsy) and searching for known strings — only practical if the pagefile is unencrypted. The real guarantee comes from `VirtualLock` calls in `src-tauri/src/memory/`.

---

## 7. Run the built-in audit tests

The project has security audit tests that programmatically cover key zeroization and IPC sanitisation:

```powershell
cargo test ui::security --manifest-path src-tauri/Cargo.toml
```

---

## Summary

| Rule | Manual tool |
|---|---|
| No localStorage/sessionStorage | DevTools → Application tab |
| No temp file writes | Sysinternals Procmon |
| No keys in logs | `RUST_LOG=debug` + grep |
| No keys in IPC responses | DevTools console + invoke patch |
| Keys zeroized on lock | Process Hacker memory search |
| mlock working | Indirect — rely on audit tests |
