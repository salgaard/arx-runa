---
applyTo: "src-tauri/capabilities/**"
---

# Tauri capabilities — scoped rules

These rules apply to capability definitions in `src-tauri/capabilities/` and
security configuration in `src-tauri/tauri.conf.json`.

## Deny-by-default

- Start with zero permissions — add only what is strictly needed
- Every capability file must have a clear `description` explaining why these
  permissions are required
- Review capability grants during code review — treat them like security-sensitive
  code

## Window targeting

- Never use `"windows": ["*"]` with permissions that access sensitive resources
  (filesystem, shell, HTTP, clipboard)
- Each window should have its own capability file with minimal permissions
- VoidGate main window: only needs vault operations, no shell or arbitrary FS

## Remote URL access

- VoidGate is a local-only application — no remote capabilities allowed
- Never add `"remote": { "urls": [...] }` to any capability
- All Tauri commands operate on local encrypted data only

## Platform-specific capabilities

- When using `"platforms": [...]`, explicitly list each platform
- Do not assume a permission is safe on all platforms — WebView behaviour differs
- Test capabilities on all target platforms (Windows, macOS, Linux)

## Scope restrictions

- All filesystem permissions must use explicit scope patterns
- Deny sensitive system directories:
  ```toml
  [[scope.deny]]
  path = "$APPLOCALDATA/EBWebView/**"  # Windows WebView data
  ```
- Use `$APPDATA`, `$APPLOCALDATA`, `$DOCUMENT` — never absolute paths

## Schema usage

- Every capability JSON file must include `"$schema"` for IDE autocompletion:
  ```json
  {
    "$schema": "../gen/schemas/desktop-schema.json",
    "identifier": "main-capability",
    ...
  }
  ```
- Use platform-specific schemas: `desktop-schema.json`, `mobile-schema.json`

## Permission sets

- Group related permissions into named sets in `src-tauri/permissions/`
- Prefer small, focused permission sets over large combined ones
- Name permission sets descriptively: `vault-read-operations`, not `perms-1`

## VoidGate-specific restrictions

- Never grant `shell:*` permissions — Rclone is invoked via controlled Rust code
- Never grant `fs:write-all` or `fs:read-all` — use scoped paths only
- Clipboard access: deny by default (prevents key material leakage)
- HTTP plugin: not used — cloud sync is via Rclone subprocess

## Capability review checklist

Before merging any capability change:
1. Is this permission strictly necessary?
2. Is the scope as narrow as possible?
3. Does the window actually need this permission?
4. Could a compromised frontend abuse this permission?
5. Is the description accurate and complete?
