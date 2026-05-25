# Zero-Trace: Forensisk Analyse med Specialiserede Værktøjer

Supplement til `zero-trace-manual-verification.md`. Dækker dybere forensisk analyse med specialiserede tools — relevant som fremtidig forbedring af Zero-Trace-verificeringen og som dokumentation til rapporten (Bilag B).

Den manuelle tilgang i `zero-trace-manual-verification.md` er tilstrækkelig for funktionel verifikation. Forensiske tools er relevante hvis man vil producere reproducerbare, dokumenterbare fund til en security audit eller rapport.

---

## Diskanalyse — hvad efterlades efter vault-lås

### Sysinternals Strings
Hurtig scanning for klartekst-strenge i specifikke filer eller mapper:

```powershell
strings64.exe -n 8 C:\Users\<user>\AppData\Local\Temp > temp_strings.txt
Select-String -Path temp_strings.txt -Pattern "<kendt filnavn eller indhold>"
```

### Autopsy (open source)
Fuld disk-forensisk analyse. Relevant for:
- Scanning af `%TEMP%`, `%LOCALAPPDATA%` og vault-mappen for klartekst-fragmenter
- MFT-analyse: NTFS Master File Table viser metadata om slettede filer (filnavne overlever sletning)
- `$MFT`-residue: små filer (< ~700 bytes) gemmes direkte i MFT-posten og kan efterlade fragmenter selv efter sletning

Download: https://www.autopsy.com/download/

### FTK Imager (gratis version)
Lav et disk-image af vault-området og analyser offline:

```
File → Add Evidence Item → Logical Drive → vælg drev
File → Create Disk Image → E01-format
```

Analyser image med Autopsy eller WinHex for klartekst-residue.

---

## Hukommelsesanalyse — nøgler i RAM efter lås

### Volatility Framework
Memory forensics: dump processen og søg efter kendte strings (password, nøgle-fragmenter).

```bash
# Tag memory dump af kørende proces (Windows)
# Kræver admin + WinPmem eller lignende driver

volatility3 -f arx-runa.dmp windows.pslist
volatility3 -f arx-runa.dmp windows.dumpfiles --pid <pid>
volatility3 -f arx-runa.dmp windows.strings --pid <pid> | grep -i "password\|key"
```

Download: https://github.com/volatilityfoundation/volatility3

### Process Hacker (allerede dækket i manual guide)
Tilstrækkeligt til at verificere at kendte strenge forsvinder fra processen efter vault-lås — se `zero-trace-manual-verification.md §5`.

---

## Hvad man konkret leder efter

| Artefakt | Lokation | Acceptabelt? |
|----------|----------|--------------|
| Krypterede vault-blobs | Vault-mappe | Ja — forventet |
| Plaintext filindhold | `%TEMP%`, `%LOCALAPPDATA%` | Nej |
| Filnavne i klartekst | MFT, log-filer | Nej |
| Session-nøgle i RAM efter lås | Processen | Nej — zeroize skal have ryddet |
| Nøgle i pagefile | `C:\pagefile.sys` | Nej — mlock/VirtualLock skal forhindre dette |
| Browser localStorage/sessionStorage | Tauri WebView | Nej |

---

## Anbefalet fremgangsmåde for Bilag B

1. Udfør den manuelle verifikation fra `zero-trace-manual-verification.md` og dokumentér fund
2. Supplér med Strings-scanning af `%TEMP%` og `%LOCALAPPDATA%`
3. Optionelt: Process Hacker memory search for kendte password-strenge (før og efter lås)
4. Dokumentér: hvilke artefakter blev fundet, hvilke var acceptable, hvilke var ikke
