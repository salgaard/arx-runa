---
mode: ask
description: Plan an implementation without executing it — output plan only, wait for approval
---

Plan the implementation of: ${input:What should be planned?}

Structure your plan as follows:

1. **Goal** — what are we building or changing, in one sentence
2. **Context** — what exists today, what constraints apply
3. **Approach** — step-by-step implementation plan with file paths
4. **Security implications** — does this touch `src-tauri/src/crypto/`,
   `src-tauri/src/auth/`, or `src-tauri/src/storage/`?
   If yes, note what the `security-reviewer` agent should check afterward
5. **Testing strategy** — what tests are needed, what boundary cases matter
6. **Documentation impact** — which `docs/` files need creating or updating
   after implementation

Do NOT start implementing. Output the plan only. Wait for approval
before proceeding.
