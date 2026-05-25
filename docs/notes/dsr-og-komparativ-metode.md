# DSR, Hevner og komparativ analyse — metodenote til rapporten

> **Formål:** Reference til mundtlig eksamen og til at skrive/forsvare §3 i bachelorapporten.
> Dækker: hvad DSR er, Hevner et al.s 7 guidelines, komparativ analyse, og hvordan Arx Runas hybride model passer ind.

---

## Design Science Research (DSR)

DSR er en videnskabelig metode inden for informationssystemer (IS) og softwareengineering.
Kerneprincippet er at viden kan produceres ved at *designe og evaluere artefakter*, ikke kun ved at observere og forklare eksisterende fænomener.

Et artefakt i DSR-forstand kan være:

- Et software-system (Arx Runa)
- En model eller arkitektur
- En metode eller proces
- Et design-mønster

Artefaktet er selve svaret på forskningsspørgsmålet. Evalueringen af om artefaktet løser det identificerede problem er vidensproduktionen.

**Hvornår er DSR den rigtige metode?**
Når problemformuleringen spørger "hvordan *kan* X designes/bygges/implementeres?" frem for "hvorfor sker X?" eller "hvad er sammenhængen mellem X og Y?". Det første er et designspørgsmål; de to sidste er forklarende spørgsmål der hører til empiriske eller positivistiske metoder.

Arx Runas problemformulering: *"Hvordan kan en softwareløsning [...] designes og implementeres..."* — præcis et designspørgsmål.

---

## Hevner et al. (2004) — de 7 guidelines

**Kilde:** Hevner, A. R., March, S. T., Park, J., & Ram, S. (2004). Design science in information systems research. *MIS Quarterly*, 28(1), 75–105.

Papiret definerer syv retningslinjer for hvad der konstituerer god DSR. De tre mest relevante for Arx Runa:

| Guideline | Hvad det betyder | Arx Runas opfyldelse |
|-----------|-----------------|----------------------|
| **1. Design som artefakt** | Forskningen skal producere et levedygtigt artefakt | Arx Runa er et fungerende krypteret cloud-storage-system |
| **2. Problemrelevans** | Problemet skal være virkelighedsnært og relevant for praksis | Zero-trust cloud storage er et reelt, uløst brugerproblem (dokumenteret via UC-1–5) |
| **3. Designevaluering** | Artefaktet skal evalueres rigorøst mod klart definerede kriterier | Use cases + kravdomæner (REQ-*) + fire testlag (unit, scenario, integration, E2E) |
| **4. Forskningsbidrag** | Ny viden, ikke blot en ny implementering | Den specifikke kombination: hardware-MFA + zero-trace + provider-agnostisk storage |
| **5. Forskningsstrenghed** | Designet skal anvende solide metoder og teorier | Kryptografisk rationale fra RFC'er og NIST-standarder |
| **6. Design som søgeproces** | Design er iterativt og kræver udforsk-/tilpasningscyklusser | Hybridmodellen (se nedenfor) |
| **7. Kommunikation af forskning** | Resultater kommunikeres til både teknisk og ledelsesmæssigt publikum | Rapporten dækker begge lag: kryptografisk dybde og brugsscenarier |

Til mundtlig eksamen: kend guideline 1, 2 og 3 udenad. De tre øvrige er bonus.

---

## Komparativ analyse som evalueringsredskab

Komparativ analyse bruges i §5–9 til at begrunde designvalg. Strukturen er:

```
Use case → Kravdomæne → Evalueringsparametre → Sammenligning af alternativer → Begrundet valg
```

**Eksempel — §5 (kryptering):**

| Parameter | AES-256-GCM | ChaCha20-Poly1305 | XChaCha20-Poly1305 |
|-----------|-------------|-------------------|-------------------|
| Nonce-sikkerhed | 96 bit (kollisionsrisiko ved høj volumen) | 96 bit | 192 bit (sikker ved høj volumen) |
| Timing-robusthed | Afhænger af AES-NI | Konstant tid | Konstant tid |
| Platformsydelse (uden HW-accel.) | Langsom | Hurtig | Hurtig |

Valgt: **XChaCha20-Poly1305** — opfylder REQ-CRYPTO-001 (nonce-sikkerhed) og REQ-CRYPTO-003 (platformsuafhængig ydeevne).

**Hvad komparativ analyse *ikke* er:**
Det er ikke en forudlavet kravspecifikation der styrede implementeringen kronologisk. Kravdomænerne er den analytiske linse rapporten anvender til at evaluere om de trufne designbeslutninger samlet set opfylder brugernes behov.

**Validitetsbegrænsning:**
Analysens validitet afhænger af at evalueringsparametrene dækker de reelle krav. Risikoen reduceres ved at parametrene er udledt direkte af kravdomænerne, som er funderet i use casene.

---

## Den hybride udviklingsmodel og dens placering i DSR

Hevners guideline 6 beskriver design som en *søgeproces*: man itererer mellem problem- og løsningsrum. Den hybride model Arx Runa anvender er en konkret realisering af dette:

### Fase 1 — Upfront systemdesign (vandfaldsinspiret)

*Hvad:* Hele systemet (Phase 0–6) designes på forhånd. Kryptografiske invarianter og contract surfaces fastlægges inden implementering begynder.

*Hvorfor nødvendigt:*
Kryptografiske invarianter propagerer på tværs af faser. Et fejlbehæftet nøglehierarki i Phase 1 er en strukturel fejl der påvirker autentificering (Phase 2), chunking (Phase 3) og fildeling (Phase 5). Denne type fejl kan ikke rettes iterativt uden at redesigne hele systemet. Upfront design reducerer denne risiko.

*DSR-perspektiv:* Hevner et al. kalder dette "awareness of problem" og "suggestion" — de tidlige faser i DSR-cyklussen hvor problemet afgrænses og løsningsrummet kortlægges.

### Fase 2 — Parallel implementering (agil-inspireret)

*Hvad:* Implementering på tværs af alle syv faser kører parallelt. Design-dokumenter opdateres løbende. Unit tests verificerer enkeltmoduler. Systemtest efter UI-færdiggørelse afdækker fejl på tværs af lag og udløser iterationer.

*Hvorfor nødvendigt:*
Implementeringens kompleksitet lader sig ikke forudsige fuldt ud i designfasen. Iterativ tilpasning er uundgåelig.

*DSR-perspektiv:* Svarer til Hevners "development", "evaluation" og "conclusion"-faser der gentages i cyklusser.

### Hybridformens begrundelse

Sikkerhedskritisk software stiller to modstridende krav:

1. Kryptografiske invarianter *skal* designes samlet og forstås på tværs af alle lag (kræver upfront)
2. Implementeringens detaljer *kan ikke* forudsiges fuldt ud (kræver iterativ tilpasning)

Ren vandfaldsmodel ville ignorere (2). Ren agil model ville risikere at ugyldiggøre (1). Hybridmodellen løser spændingen ved at adskille *hvad der skal designes upfront* (kryptografisk arkitektur) fra *hvad der kan itereres* (implementeringsdetaljer).

---

## Hurtig opsummering til eksamen

| Spørgsmål | Svar |
|-----------|------|
| Hvad er DSR? | En metode til at producere viden ved at designe og evaluere artefakter |
| Hvad er Hevner et al.? | Det kanoniske DSR-paper (MIS Quarterly 2004) med 7 guidelines |
| Hvorfor er DSR valgt her? | Problemformuleringen spørger "hvordan designes og implementeres" — et designspørgsmål |
| Hvad er komparativ analyse? | Alternativer vurderes mod evalueringsparametre udledt af use cases og krav |
| Hvad er hybridmodellen? | Upfront kryptografisk arkitektur + iterativ implementering |
| Hvorfor hybrid? | Kryptografiske invarianter kræver samlet design; implementeringsdetaljer kræver iterativ tilpasning |
