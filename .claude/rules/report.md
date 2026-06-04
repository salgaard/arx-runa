---
paths:
  - "docs/report/**"
---

# Rapport-regler: Arx Runa Bachelorrapport

Disse regler gælder i alle sessioner der arbejder på rapporten i `docs/report/arx-runa-bachelorrapport.md`.

---

## Rolle og ansvar

- Claude er skrivepartner, ikke ghostwriter. Claude leverer sektionsudkast; brugeren ejer stemmen og godkender.
- Claude trækker altid fra eksisterende docs i `docs/`, aldrig fra hukommelse alene.
- Alle tekniske påstande knyttes til en kilde (RFC, paper, design-doc). Manglende kilde markeres `[KILDE: ...]`.
- Fakta der kræver bekræftelse fra brugeren markeres `[BEKRÆFT: ...]`.

---

## Dansk typografi

**Forbudt:**
- Em-streg `—` (ASCII 0x2014): et engelsk tegn, overbrugt af AI. Bruges ikke i dansk prosa.

**Erstatninger:**
| Situation | Brug i stedet |
|-----------|--------------|
| Indskudt forklaring | Parentes: `artefaktet (det udviklede system) er...` |
| Introduktion af definition | Kolon: `contract surfaces: de bindende grænseflader` |
| Ledsætning/tilføjelse | Komma: `justeringer, uden at binde processen` |
| Talområde / årstal-interval | En-streg uden mellemrum: `10–20`, `2024–2025` |

**Øvrige regler:**
- Anførselstegn: `»tekst«` eller `„tekst"` (ikke `"tekst"`, engelske anførselstegn)
- Decimalseparator: komma (`3,14`), ikke punktum
- Ingen Oxford-komma: "rødt, grønt og blåt" (ikke "rødt, grønt, og blåt")

---

## AI-mønstre der ikke passer i dansk

**Forbudt:**
- Semikolon som sætningssamler: `"Systemet krypterer filer; cloud-udbyderen ser kun blobs"` (to separate sætninger med punktum). Semikolon er gyldigt på dansk men bruges sparsomt; kun når to sætninger er tæt forbundne og begge er korte.
- Overdrevne formelle konnektorer: "Ydermere", "Endvidere", "Således", "Hermed", "I forlængelse heraf". Disse lugter af AI. Brug almindelige overgange ("Desuden", "Derudover") sparsomt eller skriv en ny sætning uden overgang.
- Substantiv-kapitalisering midt i sætning: `"Vault-Nøglen"`, `"Design-Dokumentet"` (forkert på dansk). Kun første bogstav i sætning og egennavne kapitaliseres.
- Inkonsistent listepunktsegnsætning: vælg enten fuld sætning med punktum på hvert punkt, eller fragment uden punktum, aldrig blandet i samme liste.
- `"Dette"` som fyldpronomen: `"Dette betyder at..."`, `"Dette viser at..."` - erstat med `"Det betyder..."` eller skriv sætningen om så pronomenet undgås helt.
- Pilenotation i prosa (`→`, `->`, `=>`) til at beskrive dataflow eller rækkefølger. Brug i stedet et mermaid-diagram (`flowchart TD` eller `sequenceDiagram`) jf. `.claude/rules/mermaid.md`. Pile er tilladte *inde i* mermaid-kodeblokke som diagramsyntaks, men aldrig i løbende tekst eller som erstatning for et diagram.
- Em-streg (`—`, U+2014) i prosa. Brug bindestreg (`-`) i sammensatte ord, kolon til definitioner og komma til indskudte sætninger.
- Skjult punktlisteform: gentagen brug af mønsteret `**Term** verb forklaring: yderligere forklaring.` én gang pr. egenskab i samme afsnit. Det er reelt en punktliste med kolon som separator. Brug i stedet enten (a) en eksplicit tabel med billedtekst der *analyseres* i den omgivende prosa, eller (b) løbende prosa med varierede sætningskonstruktioner hvor koblingerne mellem egenskaberne er eksplicitte. Tabellen foretrækkes når sammenligningen er central for analysen.
- Skjult tre-liste i prosa: mønsteret `"For det første... For det andet... For det tredje..."` eller tre parallelle sætninger der begynder identisk (fx "Nøglerotation pr. fil er mulig... Eksponeringsradius er begrænset... Fildeling understøttes..."). Det er en skjult punktliste. Tre parallelle elementer skrives enten som (a) eksplicit punktliste eller (b) prosa med varierede sætningskonstruktioner der knytter elementerne til hinanden. To parallelle elementer er acceptable.
- Kolon som sætningssmelter i prosa: mønsteret `[påstand]: [uddybning af samme påstand]` er overbrugt AI-stil på linje med em-stregen. Eksempel: `"Princippet er begrænset eksponering: kompromittering af én DEK eksponerer..."`. Brug i stedet to sætninger med punktum eller en ledsætning med `fordi`, `hvilket betyder at` eller `så`. Kolon er kun tilladt i tre situationer: (1) teknisk term defineres ved første forekomst (`AAD (Additional Authenticated Data): ...`), (2) introduktion af en eksplicit nummereret eller punktformet liste, (3) introduktion af en opgørelse på formen `Nøgler der aldrig forlader enheden: master_key, key_encryption_key, ...`.
- Emoji og statusikoner i løbende tekst og tabeller: ✅, ⚠️, ❌, 🔒, og lignende Unicode-symboler er AI-mønstre der ikke hører hjemme i en akademisk rapport. Erstat med dansk tekst: "Bekræftet", "Ikke bekræftet", "Dokumenteret begrænsning", "Se §X.Y". I tabeller bruges kolonnen til at bære resultatet i ord, ikke et ikon der erstatter vurderingen.

---

## Skrivestil: Direkte og præcis

**Forbudt:**
- AI-fyld: "Det er vigtigt at bemærke...", "I denne sammenhæng...", "Som nævnt ovenfor...", "Det er interessant at...", "Man kan argumentere for...", "Dette afsnit vil..."
- Første person: "vi", "jeg", "projektet har valgt", "teamet besluttede", "vi valgte"
- Tomme overgange: "Derudover skal det nævnes at...", "På baggrund af ovenstående..."

**Foretrukket:**
- Deklarativ eller passiv: "XChaCha20 blev valgt fordi...", "Systemet anvender...", "Analysen viser...", "Resultatet er..."
- Varieret sætningslængde med bias mod kortere. Ét argument pr. sætning som udgangspunkt.
- Overgange: simple og funktionelle.
- Tekniske termer på engelsk (XChaCha20, HPKE, AEAD, HKDF, Argon2id). Defineres ved første forekomst.
- ikke for akademisk tunge sætninger.

**Eksempel på god stil:**
> XChaCha20-Poly1305 blev valgt som AEAD-primitiv fremfor AES-256-GCM af to primære årsager: nonce-sikkerheden ved 192 bits eliminerer risikoen for kollisioner selv ved høj volumen, og ydeevnen på systemer uden AES-NI er markant bedre.

---

## Logisk kæde gennem rapporten

Rapporten følger denne kæde konsekvent (rød tråd):

**Problemformulering → Use Cases → Kravindsamling → Design → Implementering → Evaluering**

- §2 etablerer problemet og underspørgsmålene (UQ1–5)
- §5.1 Use Cases viser de konkrete scenarier der motiverer kravene
- §5.2 Systemkrav (fra `requirements.md`) viser hvad systemet skal kunne
- §6–10 analyserer om designbeslutningerne imødekommer kravene
- §11–13 evaluerer, diskuterer og konkluderer mod problemformuleringen

---

## Struktur-regler

- **Kapitelstruktur:** §6–10 er selvstændige top-niveau kapitler (ikke samlet under ét "Analyse"-kapitel). 1-til-1 mapping underspørgsmål→kapitel er intentionel og må ikke ændres (UQ1=§6, UQ2=§7, UQ3=§8, UQ4=§9, UQ5=§10).
- **Rød tråd:** Hvert kapitel åbner med en sætning der forankrer det i problemformuleringen, fx "Dette kapitel undersøger underspørgsmål 1:..."
- **Delkonklusioner:** Hvert analysekapitel (§6–10) afsluttes med en delkonklusion (max 150 ord) der direkte besvarer det pågældende underspørgsmål og refererer tilbage til §2. Label: `Delkonklusion - Underspørgsmål N` (bindestreg, ikke em-streg).
- **Figurer og tabeller:** Altid ledsaget af billedtekst + analyse i brødteksten. Ikke bare "se figur X".
- **Anbefalinger i §11:** Skal være teoriforankrede: citér kilde eller design-princip, ikke personlig vurdering.
- **Use cases i §5.1:** Alle 5 UC'er præsenteres som tabel (UC-ID, scenarie, primær sikkerhedsegenskab, krav-domæner). UC-1 og UC-4 nævnes kort i §1 som konkrete eksempler.
- **Requirements.md:** Bruges som intern reference i UQ-kapitlerne (fx REQ-CRYPTO-001 som belæg). §5.2 indeholder kun domæneoversigtstabellen (6 rækker). Fuldt kravkatalog (101 krav) er i Bilag E.
- **Trusselsmodel:** §5.4 etablerer adversary-model, trust boundaries og out of scope. §6–10 refererer til den: "Jf. trusselsmodellen (§5.4)..."

---

## Kildeformat (APA 7)

- RFC'er: `IETF. (år). RFC XXXX: Titel. https://...`
- NIST: `NIST. (år). Dokument-nummer: Titel.`
- Papers: Efternavn, I. (år). Titel. *Tidsskrift*, vol(nr), sider.
- Konkurrenters hjemmesider: kun sekundære kilder.

Prioriterede kildetyper (høj til lav): RFC'er → NIST-dokumenter → peer-reviewed papers → officielle specs → konkurrent-whitepapers.

---

## Citationspraksis: kritisk for alle afsnit

**Grundregel: Ethvert faktapåstand om en tredjepart, et produkt, en standard eller et juridisk instrument kræver en inline-citation direkte ved den pågældende påstand.** En kilde i litteraturlisten uden inline-reference er usynlig for censor.

### Placering

Citationen placeres *umiddelbart efter* den konkrete påstand, før punktum:

```
Tresorit anvender RSA-4096 med OAEP til nøgledeling (Tresorit, u.å.).
```

Ikke samlet i slutningen af et langt afsnit medmindre *alle* påstande i afsnittet stammer fra præcis én kilde.

### Hvad der kræver citation

| Påstandstype | Kræver citation? |
|-------------|-----------------|
| Teknisk egenskab ved konkurrent ("Cryptomator bruger scrypt") | Ja - konkurrentens egen dokumentation |
| Fraværet af en egenskab ("Cryptomator mangler hardware MFA") | Ja - dokumenteret fravær i arkitekturdokumentation |
| Juridisk instrument ("CLOUD Act forpligter...") | Ja - primærkilde (lovtekst) |
| RFC-specifikation ("HPKE er defineret i RFC 9180") | Ja - den pågældende RFC |
| Egne arkitekturbeslutninger ("Arx Runa anvender XChaCha20") | Nej - intern kilde, dokumenteres via kravs-reference (REQ-CRYPTO-001) |
| Analytisk konklusion afledt af citerede påstande | Nej - men præmisserne skal være citeret |

### Tabeller med sammenligninger

Tabeller der sammenligner produkter eller egenskaber skal have en caption der eksplicit nævner kildegrundlaget for hvert produkt:

```
*Tabel X.Y: ... Kildegrundlag: Cryptomator (u.å.); Tresorit (u.å.); Proton AG (u.å.).*
```

✗ i en sammenligningstabel betyder "ikke dokumenteret i den pågældende kilde" (ikke "vi antager det ikke eksisterer"). Formulér altid tabelnoten præcist.

### Manglende kilder: løs dem på stedet

Når et claim skrives om en tredjepart, et produkt eller et juridisk instrument: **find kilden med det samme via `WebFetch`**, skriv ikke claimet og gå videre.

Fremgangsmåde:
1. Identificér det specifikke claim (teknisk egenskab, fravær, juridisk instrument).
2. Brug `WebFetch` mod den mest sandsynlige kilde (officiel produktside, RFC-editor, lovtekst).
3. Verificér at siden rent faktisk indeholder det claim. En kilde der ikke understøtter det konkrete claim er ikke valid, selv om den er relevant generelt.
4. Tilføj kilden til litteraturlisten med `*(verificeret ÅÅÅÅ-MM-DD)*` og skriv inline-citationen ved claimet.

`[KILDE: ...]` som placeholder er kun tilladt hvis kilden **ikke kan lokaliseres i den aktuelle session** (fx bag login, ikke-offentlig). Sæt i så fald markøren ved claimet og notér præcis hvad der mangler. Samling af manglende kilder et andet sted i dokumentet er ikke tilstrækkeligt.

Fraværs-claims ("X er ikke dokumenteret") citeres mod den officielle arkitekturdokumentation for produktet, ikke mod en produktside der heller ikke nævner det. Formulér som "X er ikke dokumenteret i [kilde]", ikke "X har ikke denne egenskab".

### Verificering

Brug `WebFetch` til at verificere at URL'er i litteraturlisten faktisk indeholder de claims de bruges til at bevise. En kilde der ikke understøtter det specifikke claim er ikke en valid citation for det claim, selv om den er relevant generelt.

---

## Samarbejdsproces

- Claude leverer **sektionsudkast** á 300–600 ord pr. underafsnit baseret på `docs/`.
- Brugeren reviewer og justerer stemme. Bed om stilfeedback tidligt, det låser samarbejdet.
- Arbejd sektion for sektion i denne rækkefølge:
  1. §3 Metode
  2. §4 Relaterede systemer (ren related work, ingen scoreboard) → §5 Kravanalyse og systemramme (UC-tabel, systemkrav, arkitektur, trusselsmodel)
  3. §6 UQ1 → §7 UQ2 → §8 UQ3 → §9 UQ4 → §10 UQ5
  4. §11 Test → §12 Diskussion → §13 Konklusion
  5. §1 Indledning (sidst, skrives bedst når argumentet er færdigt)
  6. Forside (tegntælling indsættes ved aflevering), sidenummerering, referencer, AI-deklarering

---

## Dokumentnavigation

Brug `jdocmunch-mcp` til at finde sektioner i docs. Primære kildefiler (prioriteret):

1. `docs/how-it-works/security-model.md` + `docs/guides/security-model.md` (måske de ikke indeholder det samme, er ikke sikker): trust boundaries, threat model
2. `docs/architecture/design-invariants.md`: tværgående kontrakter
3. `docs/architecture/designs/*/design.md`: fase 1–5 design-docs
4. `docs/research/`: kryptografisk rationale, file sharing, recovery
5. `docs/use-cases/`: UC-1 til UC-5 med success criteria
6. `docs/architecture/requirements.md`: 101 krav med traceabilitet (UC → krav → design)
7. `docs/report-log/`: designbeslutninger og problemformulering-session
8. `docs/how-it-works/`: brugervenlige forklaringer der kan parafraseres
9. `docs/notes/`: noter af forskellige ting gemt løbende igennem processen
10. `docs/guides/glossary.md`: liste med begreber
11. `docs/README.md`: introduktion til projektet, bruges som forside på github pages

---

## Kodenavigation

Brug `jcodemunch-mcp` til al kodenavigation (`search_symbols`, `get_symbol_source`, `get_file_outline`, `find_references`). Læs kun filer du er ved at redigere.

Kodebasen er opdelt i to crates:

### `src/`: Leptos-frontend (Rust/WASM)

| Sti | Indhold |
|-----|---------|
| `src/app.rs` | Rod-komponent; router og globale providers |
| `src/auth.rs` | Login/opret-vault-skærme |
| `src/vault.rs` | Vault-browser (fil- og mappevisning) |
| `src/vault_picker.rs` | Vælg/opret vault ved opstart |
| `src/shares.rs`, `src/contacts.rs` | Delings- og kontaktskærme |
| `src/destinations.rs` | Cloud-destinations-skærme |
| `src/settings.rs` | Indstillinger |
| `src/state/` | Global frontend-tilstand: `session_context`, `sync_context`, `vault_context` |
| `src/components/` | Delte UI-komponenter (knapper, modaler, toast, spinner m.m.) |
| `src/ipc_types/` | Typer der spejler backend-IPC; `requests.rs` indeholder alle kommando-signaturer |
| `src/invoke.rs`, `src/ipc_channel.rs` | Tauri IPC-bro til backend |

### `src-tauri/src/`: Tauri-backend (Rust)

| Sti | Indhold |
|-----|---------|
| `auth/` | Vault-oprettelse, unlock, nøgleafledning (Argon2id), sessionsstyring og ceremonier (create, unlock, recover, rotate, change_password) |
| `auth/ceremonies/` | Én fil pr. ceremony: startpunkt for autentificeringsflows |
| `auth/session/` | `manager.rs` (sessionstilstand og timeout) · `keys.rs` (sessionsnøgler afledt fra master key) |
| `crypto/` | Kryptografiske primitiver: `encrypt_chunk`, `decrypt_chunk`, `hkdf`, `nonce`, `wrap_key`, `recovery_wrap` |
| `crypto/types/` | Newtype-wrappers for nøgler og krypterede buffere |
| `memory/` | `secure_buffer.rs` (mlock-beskyttet buffer) · `platform/` (OS-specifik lås: Windows/Unix) |
| `sharing/` | `hpke.rs` (HPKE-nøgleindkapsling) · `packages.rs` (opret/importer share-pakker) · `identity.rs` · `revocation.rs` · `b2_api.rs` + `gdrive_api.rs` |
| `storage/` | Hoved-lag for persistens |
| `storage/sqlcipher.rs` | SQLCipher-database: centrale CRUD-operationer for alle entiteter |
| `storage/schema.rs` | Skema-migrationer |
| `storage/pipeline/` | `encrypt_file`, `decrypt_file`, `exif` (EXIF-stripping), `chunk_size` |
| `storage/cloud/` | Cloud-transport (`CloudTransport`-trait) · `rclone.rs` · `sync.rs` (push/pull-logik) · `vault_header.rs` · `wizard.rs` |
| `storage/vault_ops/` | Højniveau-operationer: `upload_file`, `download_file`, `delete_file`, `epoch_flush`, `routing` |
| `ui/` | Tauri IPC-kommandohandlere: `auth_commands`, `file_commands`, `sync_commands`, `sharing_commands`, `destination_commands` |
| `ui/state.rs` | Delt Tauri-applikationstilstand (session, transport, kanaler) |
| `ui/types/` | Svar-typer serialiseret til frontend |
| `platform/permissions.rs` | Filrettighedsoperationer (owner-only ACL, Win/Unix) |
| `tests/scenarios_*.rs` | Integrationsscenarier: auth, sync, backup, destinations, sharing, real cloud |

**Typisk søgestrategi:**
- Kryptografisk primitiv → `src-tauri/src/crypto/`
- IPC-kommando → `src-tauri/src/ui/*_commands.rs`
- Database-operation → `src-tauri/src/storage/sqlcipher.rs`
- Sync-logik → `src-tauri/src/storage/cloud/sync.rs`
- Frontend-skærm → `src/` (samme navn som domæne, fx `vault.rs`, `auth.rs`)
- Ceremony-flow → `src-tauri/src/auth/ceremonies/`

---

## Bedømmelsesparametre (UCL §9)

Censorer vurderer efter disse ni kriterier - skriv mod dem:

| Parameter | Hvad det betyder for denne rapport |
|-----------|-----------------------------------|
| **Problemstilling** | Skarphed i problemformulering (§2) og afgrænsning - underspørgsmålene skal være præcise |
| **Teoretisk fundament** | RFC'er, NIST, kryptografiske papers er teorifundamentet, ikke blot beskrivelse af valg |
| **Analytisk dybde** | Teori anvendt på empiri: alternativanalyse i §6–10 er dette. Undgå beskrivende afsnit uden analyse |
| **Argumentation** | Faglig tyngde, særligt diskussionens kvalitet (§12) - personlige holdninger erstattes af faglige begrundelser |
| **Rød tråd** | Kohærens: problemformulering → use cases → krav → analyse → konklusion |
| **Produkt** | Kvalitet, funktionalitet, kompleksitet - produktet bedømmes ved mundtlig eksamen |
| **Struktur** | Logisk og professionel opbygning - sidenummerering på alle sider, figurtekster, overgangssætninger |
| **Kildevalg** | Valide og anerkendte kilder - RFC'er og NIST-docs er stærkere end konkurrenters hjemmesider |
| **Sproglig fremstilling** | Præcision, stavning, tegnsætning - vægter 10% af den samlede karakter |

---

## AI-deklarering (UCL §8.5)

UCL har eksplicitte regler for brug af AI i bachelorprojektet. Det er den studerendes ansvar at følge de gældende retningslinjer på mitucl.dk og deklarere AI-brug korrekt ved aflevering. Claude bruges som skrivepartner og kildenavigatør - den studerende ejer og godkender al tekst.

---

## Sidebudget (30 normalsider = ~72.000 tegn)

Tegnbudget: §1–§13 inklusiv, ekskl. kodeblokke, mermaid-diagrammer, forside/TOC, §14 Litteraturliste og Bilag. Mål: ≤72.000 tegn (max). Aktuelt niveau: ~71.800 tegn (2026-05-27).

| Sektion | Sider | Tegnbudget |
|---------|-------|-----------|
| §1 Indledning | 2 | 3.700 |
| §2 Problemformulering | 1 | 2.100 |
| §3 Metode | 2,5 | 4.800 |
| §4 Relaterede systemer | 1 | 1.500 |
| §5 Kravanalyse og systemramme (use cases, krav, §5.3 arkitektur, §5.4 trusselsmodel) | 2,5 | 4.700 |
| §6 UQ1 - Kryptering (inkl. Realisering) | 4 | 9.000 |
| §7 UQ2 - Autentifikation (inkl. Realisering) | 4 | 9.600 |
| §8 UQ3 - Chunking & Sync (inkl. Realisering) | 3 | 9.600 |
| §9 UQ4 - Zero-Trace (inkl. Realisering) | 3 | 7.300 |
| §10 UQ5 - Fildeling (inkl. Realisering) | 3,5 | 9.000 |
| §11 Test og evaluering | 2,5 | 4.800 |
| §12 Diskussion | 1 | 3.400 |
| §13 Konklusion | 1 | 2.800 |
| **Total (tæller mod 30 sider)** | **~30 sider** | **≤72.000** |
| §14 Litteraturliste | ikke talt med | — |
| Bilag A–F | ikke talt med | — |

### Tegntælling (verificér inden aflevering)

Kør i projektets rodmappe — giver samlet prosa-tegnantal for §1–§13 ekskl. kodeblokke og bilag:

```python
python3 -c "
import re
c = open('docs/report/arx-runa-bachelorrapport.md', encoding='utf-8').read()
c = re.sub(r'\x60\x60\x60[\s\S]*?\x60\x60\x60', '', c)
s = re.search(r'^## 1\.', c, re.M)
e = min((m.start() for p in [r'^## 14\.', r'^## Bilag', r'^## Litteratur'] if (m := re.search(p, c, re.M))), default=len(c))
print(len(c[s.start():e]))
"
```

Eller som PowerShell-oneliner:

```powershell
python3 -c "import re; c=open('docs/report/arx-runa-bachelorrapport.md',encoding='utf-8').read(); c=re.sub(r'\x60\x60\x60[\s\S]*?\x60\x60\x60','',c); s=re.search(r'^## 1\.',c,re.M); e=min((m.start() for p in [r'^## 14\.',r'^## Bilag',r'^## Litteratur'] if (m:=re.search(p,c,re.M))),default=len(c)); print(len(c[s.start():e]))"
```
