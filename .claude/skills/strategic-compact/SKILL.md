---
name: strategic-compact
description: Run /compact at the right moment with a targeted summary instruction to preserve session state.
---

## When to use

Compact before these transitions (finish current atomic unit first):
- Exploration done, implementation starting
- Phase implementation complete, before review
- Switching major domains (UI ↔ crypto ↔ storage ↔ auth)
- Tool call count exceeded ~30
- About to launch multi-agent pipeline (`/implement-plan`, `/review-only`)

## Template

Fill in the fields, then run `/compact <instruction>`:

```
Keep: current phase [PHASE], open tasks [LIST], recent decisions [LIST],
blocking findings [LIST], last verified design invariants [LIST].
Drop: exploration results, file contents, intermediate tool outputs, resolved findings.
```

**Example:**
```
Keep: phase 6.1 storage sync, open task "implement retry on 503", decision
"use exponential backoff with jitter", blocking finding "manifest lock not
released on error path", invariant "cloud never holds plaintext".
Drop: grep results, file reads, resolved clippy findings.
```
