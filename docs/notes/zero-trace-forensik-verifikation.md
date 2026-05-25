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
   - Filter: Process name = `arx-runa.exe` OR path contains `AppData`
   - Aktivér filtre: File System Activity + Registry Activity
   - Start optagelse (Ctrl+E)

2. Åbn **Arx Runa** og unlock en vault med en testfil.

3. Vis testfilen in-app (billede + video) for at trigge begge visningsstier.

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
    Where-Object { $_.LastWriteTime -gt (Get-Date).AddHours(-1) } |
    Select-Object FullName, Length, LastWriteTime

# App-data — tjek om rclone.conf stadig eksisterer
Get-ChildItem -Path "$env:APPDATA\arx-runa" -Recurse -File -ErrorAction SilentlyContinue |
    Select-Object FullName, Length

# Alternativt: søg bredt efter rclone.conf
Get-ChildItem -Path $env:TEMP -Filter "rclone*.conf" -Recurse -ErrorAction SilentlyContinue
```

Forventet resultat: ingen `rclone*.conf`, ingen Arx Runa-relaterede temp-filer.

### Step 4 — Process Monitor-analyse

Stop optagelse i Process Monitor (Ctrl+E) og filtrer på:
- `Path contains .tmp` — tjek ingen skriv-operationer til temp-filer under filvisning
- `Path contains rclone.conf` — bekræft overskriv+slet-sekvens (WriteFile → SetEndOfFile → DeleteFile)

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
