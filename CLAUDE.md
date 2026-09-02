# Neomax Rust development guide for Claude Code

`AGENTS.md` is the canonical provider-neutral development contract for this
repository. Read it before making changes and follow it in full. This file is
only the Claude Code entry point; it must not create a separate set of product
rules.

Neomax is a portable multi-harness control plane for Claude, Codex, OpenCode,
Kimi, and Grok. The universal `neomax` launcher can select among eligible
providers, while `cmax`, `cdxmax`, `ocmax`, `kmax`, and `gmax` pin the main
orchestrator without restricting the independent worker pool. Plans can route
different parts to different providers and models. Account selection uses the
92 percent proactive threshold where a five-hour window is available and the
99 percent live continuation wall. Account selection, quota protection,
rotation, durable runs, usage, the portal, worktrees, and agent tools are shared
domains in `neomax-core`.

Routing is task-aware and can use the repository stack or scheduler area,
dependency state, project and session history, active subagents, and current
usage or quota evidence. A plan can therefore mix providers and models at the
same time, while explicit provider and model choices remain authoritative.
Durable run, session, transcript, history, and usage records feed recovery,
rotation, the portal, and later routing decisions.

Keep the implementation modular and keep executable packages as composition
roots. Use `neomax_core::registry` for domain ownership, preserve the provider
interface, and split tests by responsibility. Every agent and provider must
receive the canonical Neomax tool manifest and the shared concurrency setting.
Do not add provider calls, credentials, personal paths, or live validation to
tests.

The Rust CLI has a `--dry-run` plan path, a native root interactive path, and a
guarded headless worker path. The plan path reports the resolved mode, scope,
account routing, models, and injected tool environment without starting an
upstream CLI. A root interactive launch builds typed provider-native arguments,
inherits terminal input and output, and registers live ownership in
`OrchestratorStore`. A guarded worker dispatch uses the durable `RunStore`,
worker coordinator, supervision, continuation, and failover services. Keep
repository tests hermetic and separate any operator-authorized live validation;
complete compile, test, formatting, and package gates determine release
readiness.

Kimi 0.38 root sessions use the installed `--agent-file`. Its `-p` and
`--prompt` options are headless only, so an explicit initial task uses a
bounded bootstrap to create the session first; Neomax then opens the same
session interactively with `-S ... --auto`. The final root process remains
interactive and receives no positional task or prompt flag.

For pull requests, use the blank `WORKLOG.md` template on the contribution
branch. Record user-visible behavior, affected areas, exact verification, and
remaining risk without credentials, account details, provider transcripts,
private project names, or machine paths. Run every gate listed in `AGENTS.md`
before review.
