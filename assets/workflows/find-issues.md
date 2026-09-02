---
description: Find verified defects in the current Neomax project
---

Run one bounded, read-only defect pass for the current project. `$ARGUMENTS` may scope the pass to a repository, subsystem, or defect class.

1. Confirm the project with `neomax projects`. Reconcile the queue with `neomax issue reconcile` and `neomax issue list --json`.
2. Inspect independent areas in parallel when useful. Check error handling, races, security, performance, resource cleanup, tests, and cross-repository contract drift.
3. Verify each finding with a concrete code path, reproduction, or test. Drop speculation, duplicates, and style preferences. Do not edit product code in this mode.
4. Record surviving findings with `neomax issue open`, including the affected repository, file and line, impact, evidence, and a focused suggested fix. Use the issue system's deduplication and project scope.
5. Report created or deduplicated issue keys, then stop. Run another bounded pass only when requested.
