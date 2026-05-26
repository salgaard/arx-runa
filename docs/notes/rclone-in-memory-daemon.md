# Rclone in-memory konfiguration via RC daemon

> Formål: Dokumentere den arkitektoniske vej til at eliminere rclone.conf fra disk fuldstændigt.
> Status: Ikke implementeret. Kræver fuld omskrivning af `CloudTransport`-laget.

---

## Baggrund — hvorfor rclone.conf er på disk i dag

Den nuværende implementation skriver en `rclone.conf` til en temporær, owner-restricted mappe ved sessionstart og sletter den ved sessionsafslutning (`src-tauri/src/auth/session/manager.rs` → `destroy_rclone_conf()`).

Disk-filen er uundgåelig i den nuværende arkitektur af én årsag: OAuth2-destinationer (Google Drive, OneDrive) kræver at rclone kan skrive opdaterede access tokens *tilbage* til konfigurationsfilen efter token-refresh. Statiske credentials (S3, Backblaze B2) kunne i princippet formidles via miljøvariabler (`RCLONE_CONFIG_<REMOTE>_<KEY>`), men en fælles løsning kræver disk.

**Tilbageværende risiko:** Forceret procesafslutning (kill, strømafbrud) efterlader filen på disk frem til næste opstart, hvor en opstartsroutine rydder forældreløse mapper. Se `src-tauri/src/lib.rs` setup-closure for begge mekanismer.

---

## Løsningen: rclone RC daemon (`rclone rcd`)

Rclone kan køre som en langlivet daemon med `rclone rcd`. Daemonen eksponerer et lokalt HTTP-API (standard `localhost:5572`) og holder al konfiguration i hukommelsen.

### Nøgle-API-endpoints

```
POST /config/create        — opret remote in-memory
POST /config/update        — opdater remote (fx ny token)
GET  /config/get           — læs remote-konfiguration
POST /config/delete        — slet remote

POST /operations/copyfile  — upload/download enkelt fil
POST /operations/deletefile
POST /sync/copy            — fuld directory sync
POST /operations/list      — list remote indhold
```

Fuld API-reference: https://rclone.org/rc/

### Token-refresh uden disk

Ved OAuth-refresh skriver rclone tokenet til in-memory konfigurationen i stedet for en fil. Arx Runa-koden der i dag læser filen tilbage (`tokio::fs::read_to_string(&self.session_config_path)`) erstattes af et GET-kald mod `/config/get`.

---

## Arkitektonisk ændring

### Hvad der skal skrives om

| Komponent | I dag | Med RC daemon |
|---|---|---|
| `RcloneTransport::new()` | Tager `session_config_path: PathBuf` | Tager `daemon_url: String` + `remote_name: String` |
| `base_args()` | `["--config", path, "--retries", "3"]` | Fjernes — ingen subprocess pr. operation |
| Alle `run_rclone_command()` kald | Spawner subprocess | Sender HTTP POST til daemon |
| `build_session_rclone_conf()` | Skriver fil | POST `/config/create` pr. destination |
| Token-læsning (linje 306, 460) | `read_to_string(session_config_path)` | GET `/config/get` |
| `destroy_rclone_conf()` | Overskriv + slet fil | POST `/config/delete` + kill daemon-process |
| `create_session_rclone_dir()` | Opretter `%TEMP%/arx-runa-<hex>/` | Ikke nødvendig |

### Session-livscyklus med daemon

```
authenticate()
  → spawn: rclone rcd --rc-addr=localhost:<ephemeral port> --no-auth --config=""
  → POST /config/create pr. destination (fra SQLCipher)
  → gem daemon PID + port i SessionManager

lock()
  → POST /config/delete pr. remote (zeroizer tokens in-memory)
  → kill daemon-process
  → ingen fil at slette
```

Ephemeral port: bind til `:0` og læs tildelt port fra daemon-output.

### `--no-auth` og lokal binding

Daemonen bindes til `127.0.0.1:<ephemeral port>` med `--rc-addr`. `--no-auth` er acceptabelt fordi:
- Kun localhost er tilgængeligt
- Porten er ephemeral og ukendt for andre processer
- Daemon-lifetime er bundet til sessionens lifetime

Alternativ: `--rc-user` / `--rc-pass` med tilfældig adgangskode genereret ved sessionstart og holdt i mlocked memory.

---

## Hvad der ikke ændrer sig

- `CloudTransport`-trait'en (`upload_blob`, `download_blob`, `delete_blob`, `list_blobs`) — grænsefladen er uændret
- `RcloneRunner`-trait'en kan udvides til at sende HTTP i stedet for at spawne subprocess — eller erstattes af en HTTP-klient direkte i `RcloneTransport`
- SQLCipher-skemaet for destination sessions er uændret
- Startup orphan sweep kan fjernes (ingen temp-mapper at rydde)

---

## Estimeret omfang

- `src-tauri/src/storage/cloud/rclone.rs` — fuld omskrivning (~600 linjer)
- `src-tauri/src/storage/cloud/destination_session.rs` — fjern `build_session_rclone_conf`, `create_session_rclone_dir`, `destroy_session_rclone_conf`
- `src-tauri/src/auth/session/manager.rs` — erstat `rclone_conf_path`-feltet med `daemon_handle`
- `src-tauri/src/ui/auth_commands.rs` — tilpas `try_build_and_swap_rclone_transport()`
- `src-tauri/src/lib.rs` — fjern startup orphan sweep + exit handler for rclone
- Tests — HTTP-mock i stedet for fil-baserede fixtures

Samlet: medium-til-stor refaktorering. Ingen ændringer i kryptering, SQLCipher-skema eller frontend.
