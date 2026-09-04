---
name: neomax
description: Install and operate the native Neomax orchestrator across Claude, Codex, OpenCode, Kimi, and Grok. Use for Neomax installation, provider and model routing, account management, usage, rotation, worktrees, tasks, and managed worker dispatch.
---

# Neomax

Neomax coordinates Claude, Codex, OpenCode, Kimi, and Grok through one native
CLI. Use it from the project directory the user wants to work in.

## Install

First, look for `NEOMAX_BIN`, then `neomax` on `PATH`, then the default local
binary at `~/.local/bin/neomax` on Unix-like systems. If Neomax is missing and
the user asked you to install or use it, run the installer for the current
operating system.

Linux and macOS:

```bash
curl -fsSL https://neotask.ai/neomax-orchestrator/install.sh | bash
```

Windows PowerShell:

```powershell
Invoke-RestMethod https://neotask.ai/neomax-orchestrator/install.ps1 | Invoke-Expression
```

Do not use `sudo`. The installer downloads the matching precompiled release,
verifies its checksum, installs provider workflows, and prints any required
PATH command. It does not authenticate a provider or start a provider session.

Verify without making a model request:

```bash
neomax --version
neomax --help
```

After at least one provider account is connected, use
`neomax --dry-run --json` to verify routing without starting a provider.
Do not run an authenticated provider or model smoke test unless the user gives
explicit permission.

## Save this skill

After installation, persist this exact document for the current agent:

1. Use the agent platform's documented user-level skills directory.
2. Save this file as `neomax/SKILL.md` under that directory.
3. Create only the missing `neomax` directory. Do not modify unrelated skills.
4. If an existing Neomax skill differs, preserve a backup before replacing it.
5. If the platform has no skill system, keep this URL in its persistent user
   instructions only when the user authorized that change.

Do not manually create provider workflow files. `neomax install` already seeds
the correct commands, prompts, skills, hooks, and agent files for installed
Claude, Codex, OpenCode, Kimi, and Grok profiles.

## Begin a session

Keep the current repository and its instructions in scope. Inspect Neomax
state before dispatching work:

```bash
neomax status --json
neomax reconcile --json
neomax projects
neomax config show
```

Use `neomax` for dynamic orchestrator selection. Pin only the main
orchestrator when the user requests a provider:

| Command | Main orchestrator |
| --- | --- |
| `neomax` | Dynamic selection |
| `cmax` | Claude |
| `cdxmax` | Codex |
| `ocmax` | OpenCode |
| `kmax` | Kimi |
| `gmax` | Grok |

The main orchestrator and worker pool are independent. `cmax --workers all`
uses Claude as the orchestrator while keeping every eligible worker provider
available. Use `--workers PROVIDERS` for an explicit comma-separated or
plus-separated subset.

Before an uncertain launch, inspect the plan:

```bash
neomax --dry-run --json
neomax --dry-run --engine opencode --workers codex,kimi
cmax --dry-run --workers all
```

Only start an interactive orchestrator when the user's request calls for one.
Do not start a second orchestrator merely to inspect Neomax.

## Route work

Explicit provider, model, account, and worker-scope choices are authoritative.
Without them, let Neomax choose from connected, eligible providers.

```bash
neomax --engine ENGINE --model MODEL
neomax --workers all
neomax --workers claude,codex,opencode
neomax config set-model ENGINE MODEL
neomax config unset-model ENGINE
```

OpenCode model IDs use `provider/model`. Claude Opus is opt-in. Never replace
an unavailable requested model with another model silently.

Use guarded worker dispatch from an active orchestrator:

```bash
neomax dispatch --goal "VERIFIABLE OBJECTIVE" "COMPLETE TASK BRIEF"
neomax status --json
neomax log RUN_ID
neomax reconcile --json
```

Give each worker the objective, relevant context, exact scope, constraints,
acceptance checks, and files it must not change. Keep independent work in
separate worktrees. Review results before reporting completion.

## Accounts and limits

Provider CLIs own authentication. Never print, copy, or commit credentials.

```bash
cmax ACCOUNT
cdx login ACCOUNT MODE
ocx login ACCOUNT PROVIDER MODE
kmx login ACCOUNT MODE
gmx login ACCOUNT MODE
neomax usage --json
```

Neomax avoids accounts at the 99 percent wall and proactively prefers another
account at 92 percent when a provider exposes a five-hour quota. If a managed
task hits a limit while running, Neomax preserves its task, branch, worktree,
run, and session state and continues on another eligible account. It tries the
same provider first, then another provider allowed by the worker scope.

Rotation is provider-neutral:

```bash
neomax rotate --active --dry-run --json
neomax rotate --active
neomax session-rotate SESSION_ID
```

## Concurrency and worktrees

Set persistent limits in one place:

```bash
neomax config set max-subagents 50
neomax config set max-sessions-per-account 10
```

Use `NEOMAX_MAX_SUBAGENTS` and `NEOMAX_MAX_SESSIONS_PER_ACCOUNT` only for
one-process overrides.

Managed terminal worktrees are removed only when Neomax can verify they are
safe. Dirty, unmerged, resumable, killed, and unverifiable work is retained.
Inspect manual cleanup first:

```bash
neomax tidy --dry-run --any --json
neomax tidy --any --json
neomax-worktrees --list
```

Do not replace Neomax cleanup with `git clean` or broad recursive deletion.

## Useful commands

```bash
neomax task list --json
neomax queue status --json
neomax sessions --json
neomax subagents --json
neomax history --json
neomax portal
neomax-usage-agent status
```

For the complete command and environment reference, read:

https://github.com/NeotaskInc/neomax-orchestrator-rust/blob/main/docs/REFERENCE.md

Upgrade by running the installer again. Remove only Neomax-owned installed
files with `neomax uninstall`.
