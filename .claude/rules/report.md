---
paths:
  - "docs/report/arx-runa-bachelorrapport.md"
---

# Rapport-regler: Arx Runa Bachelorrapport

Disse regler gælder i alle sessioner der arbejder på rapporten i `docs/report/arx-runa-bachelorrapport.md`.

---

## Rolle og ansvar

- Claude er skrivepartner, ikke ghostwriter. Claude leverer sektionsudkast; brugeren ejer stemmen og godkender.
- Claude trækker altid fra eksisterende docs i `docs/` — aldrig fra hukommelse alene.
- Alle tekniske påstande knyttes til en kilde (RFC, paper, design-doc). Manglende kilde markeres `[KILDE: ...]`.
- Fakta der kræver bekræftelse fra brugeren markeres `[BEKRÆFT: ...]`.

---

## Skrivestil — Direkte og præcis

**Forbudt:**
- AI-fyld: "Det er vigtigt at bemærke...", "I denne sammenhæng...", "Som nævnt ovenfor...", "Det er interessant at...", "Man kan argumentere for...", "Dette afsnit vil..."
- Første person: "vi", "jeg", "projektet har valgt", "teamet besluttede", "vi valgte"
- Tomme overgange: "Derudover skal det nævnes at...", "På baggrund af ovenstående..."

**Foretrukket:**
- Deklarativ eller passiv: "XChaCha20 blev valgt fordi...", "Systemet anvender...", "Analysen viser...", "Resultatet er..."
- Varieret sætningslængde med bias mod kortere. Ét argument pr. sætning som udgangspunkt.
- Overgange: simple og funktionelle.
- Tekniske termer på engelsk (XChaCha20, HPKE, AEAD, HKDF, Argon2id). Defineres ved første forekomst.

**Eksempel på god stil:**
> XChaCha20-Poly1305 blev valgt som AEAD-primitiv fremfor AES-256-GCM af to primære årsager: nonce-sikkerheden ved 192 bits eliminerer risikoen for kollisioner selv ved høj volumen, og ydeevnen på systemer uden AES-NI er markant bedre.

---

## Logisk kæde gennem rapporten

Rapporten følger denne kæde konsekvent — rød tråd:

**Problemformulering → Use Cases → Kravindsamling → Design → Implementering → Evaluering**

- §2 etablerer problemet og underspørgsmålene (UQ1–5)
- §4.X Use Cases viser de konkrete scenarier der motiverer kravene
- §4.X Systemkrav (fra `requirements.md`) viser hvad systemet skal kunne
- §5–9 analyserer om designbeslutningerne imødekommer kravene
- §10–11 evaluerer og konkluderer mod problemformuleringen

---

## Struktur-regler

- **Rød tråd:** Hvert kapitel åbner med en sætning der forankrer det i problemformuleringen — fx "Dette kapitel undersøger UQ1:..."
- **Delkonklusioner:** Hvert UQ-kapitel (§5–9) afsluttes med en delkonklusion (max 150 ord) der direkte besvarer underspørgsmålet og refererer tilbage til §2.
- **Figurer og tabeller:** Altid ledsaget af billedtekst + analyse i brødteksten. Ikke bare "se figur X".
- **Anbefalinger i §10:** Skal være teoriforankrede — citér kilde eller design-princip, ikke personlig vurdering.
- **Use cases i §4.X:** Alle 5 UC'er præsenteres som tabel (UC-ID, scenarie, primær sikkerhedsegenskab, krav-domæner). UC-1 og UC-4 nævnes kort i §1 som konkrete eksempler.
- **Requirements.md:** Bruges som intern reference i UQ-kapitlerne (fx REQ-CRYPTO-001 som belæg) — ikke som selvstændig sektion i rapporten.

---

## Kildeformat (APA)

- RFC'er: `IETF. (år). RFC XXXX: Titel. https://...`
- NIST: `NIST. (år). Dokument-nummer: Titel.`
- Papers: Efternavn, I. (år). Titel. *Tidsskrift*, vol(nr), sider.
- Konkurrenters hjemmesider: kun sekundære kilder.

Prioriterede kildetyper (høj til lav): RFC'er → NIST-dokumenter → peer-reviewed papers → officielle specs → konkurrent-whitepapers.

---

## Samarbejdsproces

- Claude leverer **sektionsudkast** á 300–600 ord pr. underafsnit baseret på `docs/`.
- Brugeren reviewer og justerer stemme. Bed om stilfeedback tidligt — det låser samarbejdet.
- Arbejd sektion for sektion i denne rækkefølge:
  1. §3 Metode
  2. §4 Teknisk kontekst (inkl. §4.X Systemkrav med UC-tabel)
  3. §5 UQ1 → §6 UQ2 → §7 UQ3 → §8 UQ4 → §9 UQ5
  4. §10 Diskussion → §11 Konklusion
  5. §1 Indledning (sidst — skrives bedst når argumentet er færdigt)
  6. Forside, tegntælling, referencer

---

## Dokumentnavigation

Brug `jdocmunch-mcp` til at finde sektioner i docs. Primære kildefiler (prioriteret):

1. `docs/how-it-works/security-model.md` — trust boundaries, threat model
2. `docs/architecture/design-invariants.md` — tværgående kontrakter
3. `docs/architecture/designs/*/design.md` — fase 1–5 design-docs
4. `docs/research/` — kryptografisk rationale, file sharing, recovery
5. `docs/use-cases/` — UC-1 til UC-5 med success criteria
6. `docs/architecture/requirements.md` — 106 krav med traceabilitet (UC → krav → design)
7. `docs/report-log/` — designbeslutninger og problemformulering-session
8. `docs/how-it-works/` — brugervenlige forklaringer der kan parafraseres

---

## Sidebudget (30 normalsider = ~72.000 tegn)

| Sektion | Sider |
|---------|-------|
| §1 Indledning | 2 |
| §2 Problemformulering | 1 |
| §3 Metode | 2,5 |
| §4 Teknisk kontekst (inkl. §4.X Systemkrav) | 3,5 |
| §5 UQ1 — Kryptering | 4 |
| §6 UQ2 — Autentifikation | 4 |
| §7 UQ3 — Chunking & Sync | 3 |
| §8 UQ4 — Zero-Trace | 3 |
| §9 UQ5 — Fildeling | 3,5 |
| §10 Diskussion | 2 |
| §11 Konklusion | 1 |
| Referencer | ~1,5 |
| **Total** | **~31 sider** |
