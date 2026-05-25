# Kapitel 10 — Todo og beslutninger

> Status: `development` branch · Oprettet 2026-05-25

## Hvad er der (stærkt)

- §10.1 testlag-tabel: præcis og komplet
- §10.2 UC-traceabilitet: direkte mapping UC → fil → eksempel
- §10.3 CI-matrix: faktuel, platformsdækning forklaret
- §10.4 Q1 + Q2: korrekt dækket

---

## Hvad mangler (handlingsliste)

### 1. Faktuel fejl i Q3-cellen — skal rettes

**Problem:** Q3-rækken skriver `"Ingen dokumenteret checklist"` — men `docs/notes/pre-release-checklist.md` eksisterer med 10 sektioner og ~58 checkpunkter.

**Fix:** Erstat med reference til checklisten og beskriv dens afgrænsnende rolle.

**Akademisk nuance at tilføje (prosa under tabellen):**
E2E-tests er *ikke* Q3. De er Q2 (scripted, business-facing, supporting the team). Men E2E-laget reducerer Q3-byrden: alle Tier 1 UI-flows (vault-oprettelse, fil-upload, lås/oplås) er automatiseret via WebdriverIO. Det resterende Q3-rum er dermed afgrænset til de to flows der strukturelt ikke kan automatiseres:
- **Tier 2-oprettelse/-oplåsning** — native fil-picker kan ikke drives via WebDriver
- **Recovery phrase-gendannelse** — høj indsats, sjælden, irreversibel ved fejl

`pre-release-checklist.md` dokumenterer netop disse flows. Det er mere præcist at skrive "E2E indsnævrer Q3 til ikke-automatiserbare flows, dokumenteret i checklisten" end blot "uformel manuel test".

---

### 2. Q4-beslutning — hvad gøres vs. hvad noteres

**Nuværende:** Cargo audit + gitleaks + zero_trace ✅ — rest er gap.

**Beslutning:**

| Kandidat | Beslutning | Begrundelse |
|----------|-----------|-------------|
| `cargo bench` (Criterion) — Argon2-derivation, chunk-throughput | ✅ GJORT | Bilag C opdateret med reelle tal. Bench-fil: `src-tauri/benches/crypto_benchmarks.rs` |
| `cargo geiger` — unsafe-blok-sporing | ✅ GJORT | Bilag C opdateret. `arx-runa-tauri` markeret `!`; unsafe kun i `memory/` (mlock/VirtualLock). |
| `cargo-fuzz` — manifest-deserialize, vault header | **NOTERES som gap** | Høj indsats; akademisk ærlighed er tilstrækkeligt |
| Penetrationstest / dynamisk sikkerhedstest | **NOTERES som gap** | Bevidst udenfor scope; eksplicit afgrænsning i rapporten |

Når Criterion + geiger er kørt, opdateres:
- Q4-cellen i §10.4-tabellen (tilføj de to nye ✅)
- Bilag C med reelle tal

---

### 3. Manglende delkonklusion

Alle kapitler 5–9 slutter med `> **Delkonklusion — Underspørgsmål N:**`. Kapitel 10 mangler en.

Kapitlet dækker ikke ét underspørgsmål direkte, men fungerer som tværgående evidens for systemets validering. Formuleringen bør:
- Binde de fire testlag til validering af kravdomænerne (REQ-AUTH, REQ-CRYPTO osv.)
- Anerkende Q3 og Q4-gab eksplicit og æstetisk — ærlighed er styrke, ikke svaghed
- Holdes under 6 sætninger

---

### 4. Prosamæssig forbedring

Under §10.4-tabellen bør der komme 2–3 afsnit:
1. **Q3-afsnit:** E2E/Q3-distinktionen + pre-release-checklistens rolle
2. **Q4-afsnit:** Hvad er realiseret (med tal/resultater når Criterion er kørt), hvad er kendte huller og hvorfor

Nuværende tekst (Q3-gap + Q4-gap som bullet points) er skelet — det skal være løbende prosa.

---

## Verificering inden aflevering

- [ ] Q3-celle: ingen "Ingen dokumenteret checklist"
- [ ] Q3-prose: E2E = Q2, reducerer Q3; checklisten dækker rest
- [x] Q4-celle: Criterion + geiger tilføjet ✅
- [x] Q4-prose: reelle benchmarktal i Bilag C ✅
- [ ] Delkonklusion til sidst i §10.4
- [ ] Bilag C udfyldt (eller markeret "se CI-log")
- [ ] Kapitel 10 flyder naturligt ind i kapitel 11
