# Behavioral parity contract

The Rust release is not complete until all public behavior from the reference implementation is
accounted for and tested.

## Surfaces

- Universal and provider-pinned launchers, account helpers, model overrides, and dry-run behavior.
- Worker dispatch, supervision, detachment, retry, resume, failover, kill, cleanup, PR delivery,
  scheduler, shepherd, issue ledger, admission queue, projects, tasks, and coordinated worktrees.
- Authentication discovery, pauses, cooldowns, usage windows, account selection, rotation, handoff,
  reconciliation, history, sessions, native subagents, exact diffs, status, usage, and portal data.
- Native workflows and reversible installation for Claude, Codex, OpenCode, Kimi, and Grok.
- Existing `~/.neomax` JSON, JSONL, SQLite, logs, worktrees, and history remain readable.

## Proof

Each surface requires a native unit or integration test and a sanitized differential fixture covering
success, malformed state, missing dependencies, rate limits, interruption, and recovery where those
states apply. No test may depend on a real login or make a model request.

