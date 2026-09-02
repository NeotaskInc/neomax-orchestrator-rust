---
description: Fix verified Neomax issues and prepare reviewable changes
---

Run one bounded issue-fixing batch for the current project. `$ARGUMENTS` may name a specific issue key.

1. Claim work atomically with `neomax issue next --all --json` or `neomax issue claim <key>`. Stop when no issue is available.
2. Set claimed issues to fixing, inspect their evidence, and map every affected repository and shared contract.
3. Check `neomax queue status` before dispatching. Reserve capacity, send complete briefs to eligible providers with `neomax auto`, and keep independent areas isolated.
4. Review every diff. Run focused tests plus cross-repository contract checks. Do not accept a worker's success claim without evidence.
5. Release queue capacity in every outcome. Link verified branches or pull requests to their issue keys, leave issues in review state, and never merge the default branch without approval.

Report each issue, its verification, and any blocker. Keep this pass bounded and repeat it only when needed.
