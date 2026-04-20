# Bachelor Report Requirements

## Format

- **Length**: Maximum 30 pages (excluding appendices)
- **Language**: English
- **Register**: Academic, objective — avoid first person and subjective claims
- **Structure**: Problem, Method, Analysis, Discussion, Conclusion

## Report sections mapping

| Section | Content | Primary sources |
|---------|---------|-----------------|
| Problem | Problem formulation, research questions | `docs/report-log/*problem*` |
| Method | Technical approach, design decisions | ADRs, design documents, `/report-note` entries tagged `method` |
| Analysis | Implementation results, measurements | Test results, `/report-note` entries tagged `analysis` |
| Discussion | Trade-offs, limitations, alternatives | `/report-note` entries tagged `discussion`, security reviews |
| Conclusion | Summary, future work | Final synthesis |

## Auto-capture mechanisms

1. **Post-commit hook** (`.githooks/post-commit`):
   - Creates stub entries when `src-tauri/src/` modules change
   - Staged for next commit — expand with `/report-note` or delete

2. **`/report-note` skill**:
   - Manually invoke for significant decisions, pivots, discoveries
   - Tags entries with report sections for later aggregation

3. **Report-log index** (`docs/report-log/INDEX.md`):
   - Tracks all entries with date, type, title, sections, file link

## Compilation workflow

1. Entries accumulate in `docs/report-log/` throughout development
2. Run `/report-note compile` to aggregate into structured outline
3. Each roadmap phase documents which report sections it contributes to
4. Phase 7 (Documentation and Report Completion) consolidates final report

## Quality standards

- Flag claims needing citations with `<!-- CITE: suggested source -->`
- Security claims must reference established standards (NIST, OWASP, RFCs)
- Quantitative claims require supporting data or measurements
- Design decisions require documented rationale (ADRs or report-log entries)

## Key deliverables

The report demonstrates:
1. Understanding of zero-knowledge encryption principles
2. Secure implementation of client-side cryptography
3. Trade-off analysis (security vs. usability vs. performance)
4. Comparison with existing solutions (Cryptomator, cloud providers)
