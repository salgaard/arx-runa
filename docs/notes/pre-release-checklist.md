# Pre-Release Manual Testing Checklist (Q3)

Human-driven exploratory testing performed before each release. Covers flows that cannot be automated (Tier 2 key file picker, recovery phrase UI) and those where a human eye adds value beyond what the E2E suite verifies (first-run experience, error message clarity, cross-platform behaviour).

**Who:** Developer or designated tester.  
**When:** Before tagging a release. One pass per platform if the release targets all three.  
**How:** Check each item after verifying it manually. Leave a note next to any item that fails.

Mark each item `[x]` when verified, or `[!]` when a problem was found (add a note inline).

---

## 1. First-run experience

- [ ] App launches without a loading spinner that never resolves
- [ ] Vault picker is empty and the "Create vault" entry point is clearly visible
- [ ] No error dialogs appear before any user action
- [ ] The app title and version in the window chrome are correct

---

## 2. Tier 1 vault — creation and unlock

> Partially covered by E2E (`loading_states.spec.js`, `file_operations.spec.js`). Verify the human experience, not just functional correctness.

- [ ] Create a Tier 1 vault with a realistic name and password
- [ ] Recovery phrase modal appears after creation
- [ ] "Remind me later" dismisses the modal cleanly without leaving a lingering overlay
- [ ] Vault unlocks immediately after creation without requiring a second password entry
- [ ] File browser is visible after unlock
- [ ] Lock button is clearly visible and works
- [ ] Vault card appears in the picker after locking
- [ ] Unlock with correct password succeeds
- [ ] Unlock with wrong password fails with a clear, non-technical error message — form stays on screen
- [ ] The app does not crash or hang after a failed unlock

---

## 3. Tier 2 vault — creation and unlock (key file)

> **Not covered by any automated layer.** The native file-picker cannot be driven by WebDriver. This section is the primary reason this checklist exists.

- [ ] Create a Tier 2 vault — key file picker opens when Tier 2 is selected
- [ ] Selecting a directory from the picker is handled gracefully (the app appends `arx-runa.key` rather than rejecting the path)
- [ ] Key file is written to the selected location after creation
- [ ] Recovery phrase modal appears after creation
- [ ] Vault unlocks with correct password **and** correct key file
- [ ] Vault **does not** unlock with correct password but wrong key file — error is clear
- [ ] Vault **does not** unlock with wrong password but correct key file — error is clear
- [ ] Key file from one vault cannot unlock a different vault (if a second Tier 2 vault exists)
- [ ] Moving the key file to a different directory and selecting the new path still unlocks correctly

---

## 4. Recovery phrase

> **Not covered by E2E.** High-stakes path — a usability failure here causes permanent data loss.

- [ ] "Set up recovery" flow is discoverable from the vault settings or prompt
- [ ] Recovery phrase is displayed clearly — readable font, sufficient contrast, no truncation
- [ ] Phrase can be written down (no copy-prevention, no auto-dismiss before the user is ready)
- [ ] Recovery restore flow accepts the correct phrase and unlocks/resets access
- [ ] Recovery restore with an incorrect phrase fails with a clear message and does not corrupt the vault
- [ ] After a successful recovery, the vault can be unlocked with the new password

---

## 5. Password change

> Covered by scenario tests at the Rust level. Verify the UI flow feels correct.

- [ ] Password change is discoverable from the vault settings
- [ ] Entering the wrong current password is rejected with a clear message
- [ ] New password and confirmation field mismatch is caught before submission
- [ ] After a successful change, the old password no longer unlocks the vault
- [ ] After a successful change, the new password unlocks the vault

---

## 6. File operations

> Basic presence/absence covered by E2E. Verify actual behaviour with real files.

- [ ] Upload a small file (< 1 MB) — appears in the file list
- [ ] Upload a large file (> 50 MB) — progress is visible and the app does not appear frozen
- [ ] Upload a file with a long name and special characters in the filename
- [ ] Download a file — content matches the original (spot-check)
- [ ] Delete a file — it disappears from the list immediately
- [ ] Upload and download a zero-byte file (edge case)
- [ ] Upload two files with the same name — app handles the collision without data loss

---

## 7. Cloud sync

> Covered by integration and scenario tests at the Rust level. Verify the UI flow.

- [ ] Configuring a cloud destination is discoverable
- [ ] Sync button is disabled while a sync is in progress (E2E covers this; verify visually)
- [ ] A completed sync does not leave a lingering loading indicator
- [ ] Synced files are accessible from a second device / fresh install (if infrastructure is available)

---

## 8. Zero-trace / security spot-checks

> `zero_trace.spec.js` covers localStorage, sessionStorage, file-list, and URL. Check these additional items manually.

- [ ] After locking, opening DevTools shows no vault UUIDs, file names, or key material in `localStorage` or `sessionStorage`
- [ ] After locking, the page title does not contain the vault name
- [ ] After locking, the URL bar does not retain a vault-specific path or query parameter
- [ ] The process does not write plaintext file content to any temp directory visible during an upload

---

## 9. Error states and resilience

- [ ] What happens if the app is force-closed mid-upload? Relaunch, unlock, verify the vault is not corrupted
- [ ] What happens if the key file is deleted while the vault is locked? Error on next unlock attempt is clear and actionable
- [ ] What happens if disk is nearly full during a file upload? The app surfaces an error rather than silently failing

---

## 10. Platform-specific

Run at least items marked **[all]** on every platform. Items marked with a platform name are specific to that OS.

- [ ] **[all]** App window opens at a reasonable default size — no clipped UI elements
- [ ] **[all]** Native file picker opens correctly for key file selection and upload
- [ ] **[Windows]** Paths with spaces and non-ASCII characters work for the key file location
- [ ] **[Windows]** App runs without requiring administrator privileges
- [ ] **[macOS]** Gatekeeper / notarisation does not block launch
- [ ] **[macOS]** App has necessary entitlements — no permission dialogs that shouldn't appear
- [ ] **[Linux]** App runs on the target distributions without missing `.so` dependencies

---

## Notes

_Record any failures, observations, or edge cases found during this pass:_

```
Date:
Platform:
Build:
Tester:

Findings:
-
```
