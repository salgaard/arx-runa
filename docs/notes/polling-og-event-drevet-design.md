# Polling og event-drevet design

> Formål: Dokumentere den nuværende polling-arkitektur, dens konsekvenser og vejen mod et event-drevet alternativ.

---

## Nuværende situation

Frontend poller to Tauri-kommandoer med 5-sekunders interval fra to uafhængige loops:

| Loop | Fil | Kommando | Formål |
|---|---|---|---|
| Session | `src/state/session_context.rs` | `get_session_status` | Opdage timeout, vault_tier, has_recovery_slot |
| Sync | `src/state/sync_context.rs` | `get_sync_status` | Holde `pending_changes` opdateret |

De to loops starter uafhængigt og er ikke synkroniserede, så de kan affyre næsten simultant.

### Hvad `get_session_status` laver i backenden

Pr. 2026-05-26 (efter caching-fix):
1. `session_manager.reset_timer()` — RwLock-write
2. `session_manager.state()` — atomisk read
3. `session_manager.active_vault_id()` — RwLock-read
4. `session_manager.remaining_seconds()` — beregning på Instant
5. `session_vault_info.read()` — RwLock-read (in-memory cache)

Disk I/O: ingen. Tilfredsstillende, men IPC-overhead og timer-reset sker stadig 12 gange pr. minut.

### Hvad `get_sync_status` laver i backenden

1. `session_manager.reset_timer()` — RwLock-write
2. `sync_status.read().clone()` — in-memory read
3. `db_store.get_epoch_buffer_count()` — én SQLite-forespørgsel

Lav overhead, men unødvendig når der ikke er sket noget nyt.

---

## Problemet med polling generelt

- **Tidsforsinkelse:** Tilstandsændringer bemærkes op til 5 sekunder for sent. Session-timeout vises op til 5 sekunder efter den faktisk skete.
- **Unødvendigt arbejde:** Langt størstedelen af pollene returnerer identisk tilstand. Reset af session-timeren sker 12 gange pr. minut bare for at holde sessionen i live via polling.
- **Timer-reset som bivirkning:** `reset_timer()` er et krav fra auth-reglerne — "reset_timer() must be called by IPC dispatcher on every Tauri command invocation while session is Active". Polling er i praksis den mekanisme der holder sessionen i live, fordi brugeren sidder og kigger på appen.
- **Skalering:** To backends der poller er to der kan rykkes til 10 hvis der tilføjes flere kontekster.

---

## Bedre design: event-drevet push fra backend

Tauri's `AppHandle::emit()` bruges allerede til `device-event` (USB-nøgle mount/unmount). Den samme mekanisme kan erstatte polling for session og sync.

### Hvilke events der skal pushes

| Event | Hvornår | Payload |
|---|---|---|
| `session-status-changed` | Lock, timeout, remaining\_seconds ændrer sig markant | `{ isUnlocked, vaultId, timeoutSeconds, vaultTier, hasRecoverySlot }` |
| `sync-status-changed` | Sync fuldført, `pending_changes` opdateret | `{ pendingChanges, lastSync, ... }` |

### Hvem emitter events

**Session:** `SessionManager` emitter `session-status-changed` ved:
- Tilstandsskift (`Idle → Active → Idle`)
- Hvert N sekund for timer-opdatering (kun mens aktiv — push hvert 30s er tilstrækkeligt til at vise nedtælling)

`AppHandle` gemmes allerede i `AppState.app_handle: OnceLock<tauri::AppHandle>` — den skal blot overføres til `SessionManager`.

**Sync:** Sync-kommandoerne (`sync_to_cloud`, `sync_backup`, etc.) emitter `sync-status-changed` når de ændrer `state.sync_status`.

### Frontend-siden

```rust
// I stedet for en poll-loop:
window.__TAURI__.event.listen("session-status-changed", (event) => {
    // Opdater Leptos-signal direkte
    set_state.update(|s| s.apply_status(event.payload));
});
```

I Leptos (Rust/WASM) via `tauri-sys`:
```rust
spawn_local(async move {
    let mut listener = tauri_sys::event::listen::<SessionStatus>("session-status-changed")
        .await
        .expect("event listener");
    while let Some(event) = listener.next().await {
        set_state.update(|s| s.apply_status(event.payload));
    }
});
```

### Timer-problemet

`remaining_seconds` ændrer sig kontinuert. Tre muligheder:

1. **Push hvert 30s fra backend** — lav overhead, timer unøjagtig (±30s)
2. **Frontend tæller ned lokalt** fra seneste kendte `timeoutSeconds` — nul backend-overhead, nøjagtig
3. **Behold poll kun for timer** — poll hvert 30s kun for `remaining_seconds`, ikke for tilstand

**Anbefaling:** Frontend modtager `timeout_at: Instant` (eller `timeoutSeconds`) ved unlock og tæller ned lokalt. Backend sender kun event ved tilstandsskift (lock/unlock/timeout).

---

## Cache-validitet: hvornår bliver `session_vault_info` forældet?

`SessionVaultInfo` cacher `vault_tier` og `has_recovery_slot` fra `vault-header.json`.

| Felt | Kan ændre sig under aktiv session? | Håndtering |
|---|---|---|
| `vault_tier` | Nej — sættes ved vault-oprettelse, skifter aldrig | Aldrig forældet |
| `has_recovery_slot` | Ja — `setup_recovery` tilføjer recovery slots | `setup_recovery` kalder `cache_session_vault_info` efter write |

Alle andre vault-header-ændringer (`change_password`, `rotate_key_file`) kræver at sessionen låses og genåbnes — cachen ryddes ved lock og genopbygges ved næste authenticate.

Konklusion: cachen er korrekt for alle nuværende command-flows.

---

## Estimeret omfang for event-drevet refaktorering

| Komponent | Ændring |
|---|---|
| `src-tauri/src/auth/session/manager.rs` | Gem `AppHandle`-reference; emit `session-status-changed` ved tilstandsskift og timer-ticks |
| `src-tauri/src/ui/sync_commands.rs` | Emit `sync-status-changed` efter `sync_to_cloud`, `sync_backup`, `pull_and_reconcile` |
| `src/state/session_context.rs` | Erstat poll-loop med event-listener; behold ét initial kald ved mount |
| `src/state/sync_context.rs` | Erstat poll-loop med event-listener |
| `src-tauri/src/ui/auth_commands.rs` | `get_session_status` og `get_sync_status` kan beholdes til initial sync ved mount |

`get_session_status` og `get_sync_status` kommandoerne beholdes som on-demand kald (ved mount, hot reload, manuel refresh) — de fjernes bare fra poll-loops.
