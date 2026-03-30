---
name: Pending decisions
description: Open design questions not yet resolved
type: project
---

- USB key file: fixed filename/format vs. arbitrary-looking file (plausible
  deniability trade-off vs. fingerprinting risk) — open question
- Chunk size: 4MB vs 8MB — balance storage overhead against anonymisation
  (padding waste on small files must be quantified in report). Downstream
  effects: AAD chunk_index range, upload latency, Rclone parallelism
