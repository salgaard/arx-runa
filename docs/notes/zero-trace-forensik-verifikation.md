# Zero-Trace Forensisk Verifikation — Procedure

> Formål: Empirisk bekræftelse af Zero-Trace-garantierne i §8.5 (kapitelrapporten).
> Resultater dokumenteres i **Bilag B** i rapporten.

## Hvad der skal verificeres

| Residue-type | Forventet fund | Testmetode |
|---|---|---|
| `localStorage` / `sessionStorage` | Tom efter vault-lock | Browser dev-tools |
| `IndexedDB` | Tom (CSP blokerer) | Browser dev-tools |
| DOM-tilstand | Fil-liste fjernet | Browser dev-tools |
| Vault-UUID i URL | Ryddet | Browser adressebar |
| Temp-filer fra filvisning | Ingen `.tmp`-filer | Filsystem-scan |
| `rclone.conf` på disk | Slettet | Filsystem-scan |
| Nøgler i pagefile | Ikke observerbart direkte | Notér som OS-afhængig begrænsning |
| Crash dump / hiberfil.sys | Potentielt til stede | Notér som dokumenteret undtagelse |

---

## Procedure (ca. 30–45 minutter)

### Step 1 — Forberedelse

1. Åbn Windows Sysinternals **Process Monitor** (procmon.exe) fra https://learn.microsoft.com/en-us/sysinternals/downloads/procmon

2. Sæt filtre op under **Filter → Filter…** inden optagelse startes:

   **Inkluder kun relevante processer/stier:**

   | Column | Relation | Value | Action |
   |---|---|---|---|
   | Process Name | is | `arx-runa.exe` | Include |
   | Path | contains | `AppData` | Include |

   **Ekskludér kendte ufarlige støjoperationer** (ingen af disse skriver data til disk):

   | Column | Relation | Value | Action |
   |---|---|---|---|
   | Operation | is | `CloseFile` | Exclude |
   | Operation | is | `ReadFile` | Exclude |
   | Operation | is | `LockFile` | Exclude |
   | Operation | is | `UnlockFileSingle` | Exclude |
   | Operation | is | `QueryStandardInformationFile` | Exclude |
   | Operation | is | `QueryInformationVolume` | Exclude |
   | Operation | is | `QueryAllInformationFile` | Exclude |
   | Operation | is | `QueryDirectory` | Exclude |
   | Operation | is | `QueryBasicInformationFile` | Exclude |
   | Operation | is | `QueryNetworkOpenInformationFile` | Exclude |
   | Operation | is | `QueryRemoteProtocolInformation` | Exclude |
   | Operation | is | `QuerySecurityFile` | Exclude |
   | Operation | is | `FileSystemControl` | Exclude |
   | Detail | contains | `Read Attributes` | Exclude |
   | Path | ends with | `vault.db-journal` | Exclude |
   | Path | ends with | `vault.db-wal` | Exclude |
   | Path | ends with | `.blob` | Exclude |

   > **Bemærk:** `Detail contains "Read Attributes"` fjerner vault-polling `CreateFile`-kald (som alle bruger `Desired Access: Read Attributes`) men bevarer reelle CreateFile-events med `Generic Write`, `Generic Read/Write`, eller `Disposition: Create`. `CloseFile` alene udgør typisk ~80% af optagelsens rækker og har nul forensisk relevans.

3. Aktivér: **File System Activity** + **Registry Activity**. Start optagelse (Ctrl+E).

4. Åbn **Arx Runa** og unlock en vault med en testfil.

5. Vis testfilen in-app (billede + video) for at trigge begge visningsstier.

### Step 2 — Gennemfør en vault-lock og scan

1. Lås vault (enten manuel lås eller lad timeout udløbe).

2. Åbn **browser dev-tools** (F12) i Tauri-vinduet:
   - Application-tab → Local Storage: bekræft tomt
   - Application-tab → Session Storage: bekræft tomt
   - Application-tab → IndexedDB: bekræft tomt
   - Console: kør `document.querySelector('[data-testid="file-list"]')` — skal returnere `null`

3. Tjek **adressebaren**: ingen vault-UUID synlig.

### Step 3 — Filsystem-scan

Kør i PowerShell efter vault-lock:

```powershell
# Temp-filer oprettet inden for den seneste time
Get-ChildItem -Path $env:TEMP -Recurse -File |
    Where-Object { $_.LastWriteTime -gt (Get-Date).AddMinutes(-10) } |
    Select-Object FullName, Length, LastWriteTime

# App-data — tjek om rclone.conf stadig eksisterer
Get-ChildItem -Path "$env:APPDATA\arx-runa" -Recurse -File -ErrorAction SilentlyContinue |
    Select-Object FullName, Length

# Alternativt: søg bredt efter rclone.conf
Get-ChildItem -Path $env:TEMP -Filter "rclone*.conf" -Recurse -ErrorAction SilentlyContinue
```

Forventet resultat: ingen `rclone*.conf`, ingen Arx Runa-relaterede temp-filer.

### Step 4 — Process Monitor-analyse

Stop optagelse i Process Monitor (Ctrl+E). Filtrene fra Step 1 er allerede aktive.

Hvad der skal bekræftes i det der er tilbage:

- **`Path contains rclone.conf`** — bekræft den fulde livscyklus:
  `CreateFile (Disposition: Create)` → `WriteFile` → `SetEndOfFile` → `DeleteFile`
  Mangler `DeleteFile`-linjen = rclone.conf ikke slettet = zero-trace-fejl.

- **`Operation = WriteFile`** — ingen sensitiv data skrevet til disk udenfor forventede stier (`vault.db`, krypterede `.blob`-filer i staging).

- **`Operation = RegSetValue`** — ingen nøgler eller passwords i registry.

- **`Path contains Temp` + `Operation = CreateFile` med `Disposition: Create`** — bekræft ingen uventede temp-filer oprettet under filvisning.

Gem resultatet som **PML-fil** til Bilag B.

### Step 5 — (Valgfrit) RAM-analyse med Strings

Kør før og efter vault-lock:

```powershell
# Kræver Sysinternals strings.exe tilgængeligt i PATH
# Søg efter vault-ID eller kendte nøglemønstre i proceshukommelse
# NB: kræver at du kender vault-UUID på forhånd
strings -pid (Get-Process arx-runa).Id 2>$null | Select-String "arx-runa"
```

**Alternativ uden Sysinternals:** Notér at pagefile-analyse kræver offline-adgang til `pagefile.sys` og er udenfor sessionens scope. Dokumentér dette som en arkitektonisk begrænsning (se §4.7).

---

## Hvad der skal dokumenteres i Bilag B

1. **Testdato og miljø**: OS-version, Arx Runa-version, testvault-navn
2. **Browser storage-resultater**: Screenshots fra dev-tools (localStorage, sessionStorage, IndexedDB — alle tomme)
3. **Filsystem-scan output**: PowerShell-output (ingen relevante filer fundet)
4. **Process Monitor-fund**: Skærmbillede af rclone.conf-sletningssekvensen (WriteFile → DeleteFile)
5. **Konklusion**: Hvilke residue-typer er bekræftet eliminerede, hvilke er ikke testbare

---

## Rapport-opdatering efter test

Opdatér §8.5 i rapporten:
- Erstat `[KILDE: SANS Institute, "Memory Forensic Acquisition and Analysis 101"]` med `(se Bilag B)`
- Erstat `[KILDE: SANS Institute, "Resident $DATA Residue in NTFS MFT Entries"; Belkasoft, ...]` med `(se Bilag B)`
- Fjern de fire [KILDE:]-placeholders fra litteraturlisten
- Skriv Bilag B-indholdet ind i rapporten baseret på fundene

---

## Hvad der IKKE behøver at blive testet

- Crash dumps: Dokumentér som »udenfor Arx Runas kontrol« — OS-niveau
- `hiberfil.sys` / Windows fast startup: Dokumentér som brugeransvar (anbefal at deaktivere)
- Pagefile-indhold: Kræver offline-analyse, udenfor scope for bachelorprojektet
