---
applyTo: "src-tauri/tauri.conf.json"
---

# Tauri configuration — scoped rules

These rules apply to `src-tauri/tauri.conf.json` — the main Tauri configuration
file that controls application security boundaries.

## Content Security Policy (CSP)

- CSP is REQUIRED — never ship without a restrictive CSP
- Minimum baseline:
  ```json
  "csp": {
    "default-src": "'self'",
    "connect-src": "ipc: http://ipc.localhost",
    "img-src": "'self' asset: http://asset.localhost blob: data:",
    "style-src": "'self'"
  }
  ```
- Never use `'unsafe-inline'` for `script-src` — enables XSS
- Never use `'unsafe-eval'` — enables code injection
- Tauri auto-generates nonces for bundled scripts — rely on this

## Development URL

- `devUrl` must be `http://localhost:<port>` — never a remote URL
- Do not commit a `devUrl` pointing to a public server
- For production builds, `devUrl` is ignored — `frontendDist` is used

## Dangerous settings (prohibited)

- `dangerousRemoteDomainIpcAccess`: NEVER enable — allows remote code to invoke
  Tauri commands
- `withGlobalTauri`: avoid in production — exposes `window.__TAURI__` globally,
  making it easier for injected scripts to abuse

## App identifier

- Use reverse-domain notation: `com.voidgate.app`
- Must match across all platforms (Windows, macOS, Linux)
- Do not change after release — breaks update continuity

## Security section

- Always define `security.capabilities` explicitly — do not rely on auto-discovery
- List capability identifiers by name:
  ```json
  "security": {
    "capabilities": ["main-window-capability"]
  }
  ```

## Bundle settings

- `bundle.active`: must be `true` for production builds
- Sign all release builds — unsigned apps trigger security warnings
- Windows: code signing certificate required
- macOS: notarisation required for distribution outside App Store

## Plugin configuration

- Only include plugins that are strictly necessary
- Each plugin added increases attack surface
- VoidGate should NOT use: `shell`, `http`, `clipboard` plugins
- VoidGate may use: `dialog` (file picker), `fs` (scoped), `process` (exit/restart)

## Tauri configuration checklist

Before release:
1. CSP is defined and restrictive
2. No `dangerousRemoteDomainIpcAccess`
3. `withGlobalTauri` is false or unset
4. All capabilities are explicitly listed
5. Bundle signing is configured
6. No unnecessary plugins
