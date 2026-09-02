# Neomax Orchestrator

Neomax is a native Rust control plane for coordinating coding-agent harnesses,
accounts, sessions, models, worktrees, and durable task state. Source, issues,
and pull requests are hosted at
[NeotaskInc/neomax-orchestrator-rust](https://github.com/NeotaskInc/neomax-orchestrator-rust).
Release readiness is determined by the complete runtime and distribution
verification gates below.

The product has six launch surfaces: the universal `neomax` launcher and one
provider-pinned launcher for each supported provider. The provider adapters are
Claude, Codex, OpenCode, Kimi, and Grok. A plan can use several adapters at
once, and each plan part can select its own provider and model. Neomax routes
work by an explicit user choice, a provider-pinned mode, a task or plan part,
measured account eligibility, recent project affinity, or a dynamic selection
policy.

The routing context can include the task, the repository stack or scheduler
area, dependency state, prior project and session history, active subagents,
and current usage or quota evidence. That lets one task use different
providers and models from the next task, or lets one plan use several of them
at the same time. Durable run, session, subagent, transcript, history, and
usage records remain available to recovery, rotation, the portal, and later
routing decisions. Explicit model and provider choices always remain
available; dynamic routing only uses the evidence and scope the caller allows.

## Runtime status

The native runtime has two distinct launch paths. A root interactive launch
resolves the provider, account, model, and worker scope, builds typed
provider-native arguments, registers live ownership in `OrchestratorStore`,
and starts the selected provider with inherited terminal input and output. It
does not create a durable `RunStore` task merely to open an interactive
provider session.

A guarded headless worker dispatch resolves a worker profile, persists a
`RunStore` record, starts the provider through the worker coordinator, and
applies supervision, usage, continuation, account rotation, and failover
policies. `--dry-run` reports the routing, account, model, worker scope, and
tool-environment decision without starting a provider process.

The command shapes in this document describe the current CLI surface. Release
readiness is determined by the complete workspace compile, test, formatting,
and package gates. Repository tests use fixtures, injected runners, and fake
executables. They never make authenticated model requests.

The core owns the durable contracts used by managed worker execution and
lifecycle operations:

- provider adapters, profile discovery, authentication-state detection, model
  discovery, event parsing, and provider-neutral worker requests
- account inventory, quota windows, pauses, cooldowns, reservations, live
  workers, and deterministic dynamic selection
- durable runs, event journals, process supervision, failover, session identity,
  history, and recoverable pending writes
- project registration from the launch directory, task state, FIFO agent
  admission, scheduler plans, dependency graphs, area locks, and reconciliation
- usage ledgers, transcript normalization, OpenCode database collection,
  session and subagent telemetry, and portal data aggregation
- provider-neutral agent tools with a canonical command manifest, executable
  resolution, policy checks, recursion guards, and injected environment

## Automatic quota survival

Neomax uses account and usage state when it selects work. It prefers an
authenticated, unpaused, non-cooled account with usable headroom, avoids
stacking live orchestrators on one profile, accounts for live load, and uses
the configured provider priority as a deterministic tie-breaker. If a provider
does not expose a numeric quota window, its account remains eligible and is
ranked using the evidence that is available instead of invented percentages.

These admission and continuation guarantees apply to managed headless worker
tasks. A root interactive provider session is registered as live orchestrator
ownership in `OrchestratorStore`; it is not represented as a durable
`RunStore` task until it dispatches managed work.

For a provider that exposes a numeric five-hour window, 92 percent is the
proactive admission threshold: automatic new work is sent to an account with
more headroom when one is available. The hard wall is 99 percent. Weekly
windows use the 99 percent wall. Providers that expose only reactive usage
signals are not assigned invented percentages; their limit events and local
usage evidence drive the same policy.

A profile at the hard wall is not selected for new automatic work. If a managed
worker task started before the hard wall, including an explicitly selected
account at or above the proactive threshold, and the provider reports a limit
while it is running, the task remains durable. Neomax records the limit, cools
the affected account, preserves the run, branch, worktree, and session
metadata, and continues on another eligible account. It tries another account
from the same provider first. If that provider has no eligible account, a
quota event or maintenance tick can use another provider inside the configured
worker scope; manual rotation does not cross that scope.

The rotation policy is universal. `rotate` resolves the provider that owns the
current session and applies that provider's safe mechanism. Claude and Codex
can rotate authentication in place when their credential transaction supports
it. OpenCode, Kimi, and Grok use a same-provider handoff that preserves worker
scope, task state, and model selection. If no same-provider target remains,
quota-triggered or maintenance rotation can fall back to another provider in
the allowed scope. There is one rotation surface, not a separate command for
each provider. Rotation state uses locked, atomic claims and cooldown records
so two sessions cannot take the same handoff.

Usage collection never sends a model prompt. The usage agent reads local
provider records and normalizes token and rate-limit evidence into the shared
ledger. Its Claude quota path can make a bounded request to Claude's usage API
and refresh an expired OAuth token; provider discovery can also execute bounded
status or model-list commands whose own network behavior belongs to that
provider CLI. The agent can drive the maintenance rotation tick between
sweeps. See `docs/USAGE-AGENT.md` for the exact endpoints, payloads, timeouts,
storage, and offline controls.

## Automatic worktree cleanup

Neomax limits disk growth from bulk managed worktrees. When a
managed run reaches a safe terminal state (`Done` or `Error`), an unchanged
run worktree is removed automatically. A killed run or a worktree that
contains source changes is retained. `--pr` prevents immediate whole-worktree
removal so the branch remains available for review.

Retained worktrees may still contain generated dependencies, build output, or
tool caches. The lifecycle cleaner removes only recognized regenerable
artifacts after Git confirms that each path is ignored and contains no tracked
files. The recognized directory names are `node_modules`, `target`, `.next`,
`.nuxt`, `.svelte-kit`, `.turbo`, `.parcel-cache`, `.vite`, `coverage`,
`.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `__pycache__`, `.gradle`,
`dist`, `build`, and `out`. It never performs a broad `git clean`, follows a
symlink, or removes an ignored environment file or other unverified local data.
Source files, commits, dirty state, unmerged state, and anything that cannot be
verified are preserved. Any ignored content left after the recognized artifact
pass blocks automatic whole-worktree removal.

The background usage agent runs the safe
`neomax tidy --automatic --any --json` sweep every 600 seconds by default.
That sweep applies the same artifact checks and removes a retained whole
worktree only after its branch is verified merged and clean. Automatic mode
also excludes killed and resumable run states.
Set `NEOMAX_WORKTREE_TIDY_EVERY` before installation, then reinstall the
usage-agent service to change the interval in seconds; set it to `0` to
disable only the periodic sweep. Terminal cleanup and explicit `neomax tidy`
remain available. Use
`neomax tidy --dry-run --any --json` to inspect candidates before a manual
sweep. See [docs/USAGE-AGENT.md](docs/USAGE-AGENT.md) for the maintenance
state and service behavior.

## Dynamic and explicit routing

`neomax` is the universal entry point. Its dynamic selection considers every
connected provider that has an available binary and an eligible profile. A
machine with only one provider still works. A machine with OpenCode and Kimi
can use both. A machine with all five providers can dispatch a mixed worker
fleet.

The routing contract has these precedence rules:

1. An explicit provider selection wins.
2. A requested account or dedicated orchestrator profile is honored when it is
   eligible; a known paused, cooled, unauthenticated, or 99 percent profile is
   rejected for automatic selection.
3. A resume request reuses the provider recorded for the project when that
   provider is still eligible.
4. Otherwise the selector ranks measured quota pressure, live load, recent
   selection, and provider priority.
5. A scheduler plan can assign `engine`, `model`, dependencies, and affected
   `area` to each part. Parts become ready only after their dependencies are
   done and their area locks are acquired.

The core and handoff protocol accept provider scopes such as `all`,
`codex+opencode,kimi`, or a single provider. A pinned launcher identifies the
main orchestrator; it does not inherently restrict the worker pool to that
provider. The selected scope is passed to the orchestrator and every worker
handoff. The local launch-plan command prints the resolved scope and all five
effective model entries so a caller can inspect the decision without starting
a provider.

## Launchers and aliases

| Command | Role |
| --- | --- |
| `neomax` | Dynamically select an eligible orchestrator and worker scope |
| `neomax-cli` | Compatibility alias for the universal launcher |
| `cmax` | Pin Claude as the main orchestrator |
| `cdxmax` | Pin Codex as the main orchestrator |
| `ocmax` | Pin OpenCode as the main orchestrator |
| `kmax` | Pin Kimi as the main orchestrator |
| `gmax` | Pin Grok as the main orchestrator |
| `cdx` | Codex account helper |
| `ocx` | OpenCode account helper |
| `kmx` | Kimi account helper |
| `gmx` | Grok account helper |

The aliases are symlinks or platform-appropriate copies of the `neomax`
multicall executable. The executable selects its mode from the invocation name.
`cmax` is only the Claude-pinned launcher. Shared state, tools, portal data,
usage data, and project helpers use the `neomax` namespace.

`portal`, `rotate`, and `usage` are universal Neomax subcommands. They are
available from `neomax` and every provider-pinned orchestrator, while the
worker scope remains independent of the pinned main provider. The standalone
`neomax-portal`, `neomax-usage-agent`, and `neomax-worktrees` executables are
also universal: they never select or assume a single provider. There is no
provider-prefixed worktree or usage-agent executable.

Claude account selection stays part of the pinned `cmax` launcher rather than
turning `cmax` into a generic account helper. `cmax N` opens the numbered Claude
profile, creating its managed profile directory when needed. `cmax N /login`
passes `/login` into that session so the provider CLI can authenticate it.
`cmax orchestrator` or `cmax orch` opens the dedicated `.claude-orch` profile and also leaves
the provider's `/login` flow available. The reserved automatic mode remains
`cmax --orchestrator`.

### Pair any orchestrator with any worker providers

The orchestrator and worker pool are independent choices. Use `all` for every
eligible connected provider, one provider for a provider-only scope, or a
comma/plus-separated subset for a mixed scope:

```text
cmax --dry-run --workers all
ocmax --dry-run --workers kimi,codex
kmax --dry-run --workers opencode+grok
neomax --dry-run --engine opencode --workers claude,kimi,grok
```

These forms are accepted by the launch parser and carried into the handoff
plan. Add `--dry-run` when you want to inspect the pairing without starting a
provider. Without it, the selected provider is started as the main interactive
orchestrator. Its configured worker scope remains available to that
orchestrator for guarded headless dispatch.

### Launch and plan commands

The following launch interfaces are available from the built artifact. Use
`--dry-run` when you want to inspect routing without starting a provider:

```text
neomax --help
neomax --version
neomax --dry-run [--json] [INITIAL_TASK...]
neomax --dry-run --engine ENGINE [--model MODEL] [INITIAL_TASK...]
cmax --dry-run [--json] [OPTIONS] [INITIAL_TASK...]
cdxmax --dry-run [--json] [OPTIONS] [INITIAL_TASK...]
ocmax --dry-run [--json] [OPTIONS] [INITIAL_TASK...]
kmax --dry-run [--json] [OPTIONS] [INITIAL_TASK...]
gmax --dry-run [--json] [OPTIONS] [INITIAL_TASK...]
cmax resume [SESSION_ID] [OPTIONS]
cdxmax resume [SESSION_ID] [OPTIONS]
ocmax resume [SESSION_ID] [OPTIONS]
kmax resume [SESSION_ID] [OPTIONS]
gmax resume [SESSION_ID] [OPTIONS]
cmax --resume SESSION_ID [OPTIONS]
cdxmax --resume SESSION_ID [OPTIONS]
ocmax --resume SESSION_ID [OPTIONS]
kmax --resume SESSION_ID [OPTIONS]
gmax --resume SESSION_ID [OPTIONS]
```

`--dry-run` reports the selected mode, worker engines, routing metadata,
effective models, and the environment that gives the launched agent access to
Neomax tools. A non-dry main launch starts the selected provider's typed
interactive command and registers live orchestrator ownership. A guarded
worker dispatch uses the durable run store, worker coordinator, provider
adapter, supervision, and continuation policy. The runtime status section
distinguishes these paths. Release readiness remains subject to the complete
verification gates.

Kimi 0.38 root sessions use the installed `--agent-file`. Kimi's `-p` and
`--prompt` options are headless, so `kmax [OPTIONS] [INITIAL_TASK...]` first
uses a bounded headless bootstrap to create the session, captures its resume
ID, and then opens that same session with `-S ... --auto`. The final
user-facing Kimi process remains interactive and receives no positional task
or prompt flag. A launch without an initial task opens Kimi directly; when
dynamic `neomax` selection chooses Kimi, the same bootstrap-and-resume path is
used. A resume launch cannot combine a new task with an existing session.

Supported launch options include:

```text
--dry-run [--json]
--engine ENGINE
--workers SCOPE
--model MODEL
--claude-model MODEL
--codex-model MODEL
--opencode-model PROVIDER/MODEL
--kimi-model MODEL
--grok-model MODEL
--prefer ENGINES | --priority ENGINES
--account ACCOUNT
--orchestrator | --dedicated
--goal TEXT
--base REF
--session-id ID
--resume
--max-turns N
--wait | --foreground | --fg
--detach
--plan | --no-worktree | --pr | --brief
--opus
-u
-n
-e EFFORT
-t MINUTES
-s MINUTES
```

The account helper surfaces recognize local operation metadata such as
`login`, `logout`, `models`, `status`, `run`, and `orchestrator`. Without
`--dry-run`, the selected provider CLI owns the authentication or account
operation. Authentication and live model tests require a separate
operator-authorized validation step and are not part of repository tests.

## Command reference

All commands below are subcommands of `neomax`. The `cmax`, `cdxmax`,
`ocmax`, `kmax`, and `gmax` names accept the same command surface while
pinning the main orchestrator. `--json` is supported wherever a command
returns a structured report. Provider commands operate on local profiles and
do not bypass the provider's own authentication rules.

### Launch, dispatch, and attachment

```text
neomax [OPTIONS] [INITIAL_TASK...]
cmax|cdxmax|ocmax|gmax [OPTIONS] [INITIAL_TASK...]
kmax [OPTIONS] [INITIAL_TASK...]
```

The universal form selects an eligible orchestrator dynamically. A pinned
alias selects only the orchestrator; its worker pool remains independent and
defaults to all eligible providers. Use `--engine` with `neomax` to pin the
orchestrator explicitly, and use `--workers all` or a comma/plus-separated
scope such as `codex+opencode,kimi` to control workers. `--account` selects a
profile and `--orchestrator` or `--dedicated` prefers a reserved profile.

```text
neomax --dry-run [--json] [OPTIONS] [INITIAL_TASK...]
neomax --engine ENGINE --workers SCOPE [OPTIONS] INITIAL_TASK...
neomax --goal OBJECTIVE [OPTIONS] INITIAL_TASK...
neomax dispatch [OPTIONS] INITIAL_TASK...
neomax auto [OPTIONS] INITIAL_TASK...
```

`--dry-run` resolves routing, account, model, worker scope, and tool
environment without starting a provider. A real provider launch accepts an
optional initial task or `--goal` for Claude, Codex, OpenCode, and Grok. With
neither, those providers start in their normal interactive shape and retain
the Neomax orchestrator tool environment. Kimi accepts an initial task through
the headless bootstrap described above, then resumes the interactive session
with the installed `--agent-file` contract.

Root worktree behavior is conservative. A root interactive launch uses the
current project directory unless `--base` or `--pr` requests a managed
worktree. A guarded headless worker allocates a managed run worktree by
default. `--no-worktree`, or the equivalent `NEOMAX_NO_WORKTREE` setting,
keeps execution in the current project directory. A worktree is retained for
review when `--pr` is used. Managed worktree cleanup follows the safety rules
in [Automatic worktree cleanup](#automatic-worktree-cleanup). Unmerged review
work and unverifiable source are never removed automatically.

Root orchestrator launches stay attached by default. A guarded worker
dispatch defaults to detached startup when no attachment option is supplied.
Use `--wait`, `--foreground`, or `--fg` to keep a guarded worker attached, or
use `--detach` to start its supervisor and return after an atomic startup
handshake. The detached parent waits for a started or error handshake, prints
the durable run ID and supervisor PID, and then the run can be followed with
`status`, `log`, or `history`. `--detach` and `--foreground` or `--fg` are
mutually exclusive, and `--detach` is valid only for worker dispatch.

Headless worker dispatch is a separate, guarded path for a root orchestrator:

```text
NEOMAX_ROLE=opencode neomax auto INITIAL_TASK...
NEOMAX_ROLE=claude neomax 2 INITIAL_TASK...
NEOMAX_ALLOW_WORKER_DISPATCH=1 neomax auto INITIAL_TASK...
```

The `dispatch` and `auto` forms mark the run as a worker dispatch. On the
universal launcher, a numeric positional form such as `neomax 2 TASK` is also
worker shorthand. A numeric positional form on a pinned launcher, such as
`cmax 2 TASK`, selects that provider account for the root orchestrator; it does
not become a worker dispatch unless the guarded dispatch marker is present.
The universal numeric and all explicit worker forms are accepted only when
`NEOMAX_ROLE` or `NEOMAX_WORKER` is present, or when the operator explicitly
sets `NEOMAX_ALLOW_WORKER_DISPATCH=1`. A normal launch using neither dispatch
form remains a main orchestrator session. The root orchestrator uses the
canonical manifest to start this path; workers do not silently become
orchestrators.

`dispatch` is the canonical worker-dispatch command. `auto` is its public
convenience form for automatic account selection. Both require the same
worker authorization and use the same guarded coordinator path.

The root orchestrator receives the complete canonical Neomax tool manifest
through the injected environment and may inspect state, dispatch workers,
manage tasks and projects, rotate eligible profiles, and recover runs. A
provider worker is created by the coordinator with `NEOMAX_WORKER=1`, a
restricted worker policy, recursion limits, and the same manifest boundary.
Worker processes are headless and guarded; they are not a second user-facing
orchestrator entry point.

The launch flags are:

```text
--dry-run [--json]             inspect without provider execution
--engine ENGINE                claude|codex|opencode|kimi|grok
--workers SCOPE                all or comma/plus-separated providers
--model MODEL                  override the selected orchestrator model
--claude-model MODEL           override the Claude worker model
--codex-model MODEL            override the Codex worker model
-cm MODEL                      Codex alias for --codex-model
--opencode-model MODEL         override the OpenCode worker model
--kimi-model MODEL             override the Kimi worker model
--grok-model MODEL             override the Grok worker model
--prefer ENGINES               provider priority, comma/plus separated
--account ACCOUNT              select a connected account
--orchestrator | --dedicated   prefer a reserved orchestrator account
--goal TEXT                    attach a verifiable objective
--base REF                     use a Git base ref and managed worktree
--run-id ID                    use a fixed safe durable worker run ID
--tag TEXT                     attach a searchable tag preserved in status/history
--no-worktree                  run in the current project directory
--pr                           preserve the worktree for review
--plan                         guarded read-only worker scout in the current checkout
--brief                        request a concise provider context
--session-id ID                set or resume provider session identity
--resume                       resume the supplied session
--max-turns N                  cap provider correction rounds
--wait | --foreground | --fg   stay attached until completion
--detach                       detach a guarded worker after supervisor startup
-e LEVEL                       low|medium|high|xhigh|max
-t MINUTES                     wall-time limit
-s MINUTES                     stall-time limit
-n                             disable automatic account failover
-u                             request ultra mode
--opus                         explicitly select Claude Opus
--version | -V                 print the launcher version
--help | -h                    show launcher help
```

`--run-id` is intended for scheduler and detached-worker integrations. It
accepts only a bounded path-safe identifier so a run cannot escape the local
run store. `--tag` accepts one bounded printable searchable value; the tag is
written to the durable run record and remains available through status and
history after detach, resume, or retry.

Provider-pinned `resume [SESSION_ID]` and `--resume SESSION_ID` forms reopen a
native interactive provider session. Neomax searches the selected provider's
local session stores, resolves the owning profile, and uses that provider's
native resume command. The universal `neomax resume RUN_ID` form remains the
managed worker lifecycle operation; use it for durable run continuation.

Provider-specific flags are validated before execution:

| Flag | Claude | Codex | OpenCode | Kimi | Grok |
| --- | --- | --- | --- | --- | --- |
| `--opus` | Explicit Opus selection | Rejected | Rejected | Rejected | Rejected |
| `-u` | Maps to `xhigh` when no effort is supplied | Maps to `xhigh` | Rejected | Rejected | Rejected |
| `-e LEVEL` | Accepts the Claude CLI effort value | `low`, `medium`, `high`, or `xhigh` | Rejected | Rejected | Rejected |

`--plan` is a guarded worker execution mode, not a main orchestrator option or
a scheduler-plan command. It implies `--no-worktree` and uses the provider's
read-only plan boundary in the current checkout. Use `neomax dispatch --plan`
for one scout and `neomax run-all` for a durable scheduler plan with parts,
dependencies, and area locks.

`-e` and `-u` are provider-specific. Claude accepts `low`, `medium`, `high`,
`xhigh`, and `max`; Codex accepts `low`, `medium`, `high`, and `xhigh`. `-u`
maps to `xhigh` for Claude and Codex when `-e` is not supplied. OpenCode,
Kimi, and Grok reject these controls. For a dynamic `neomax` launch, the
selected engine is resolved before this validation, so an unsupported effort
flag fails closed instead of being silently ignored. The aliases are
`--priority` for `--prefer`, `--dedicated` for `--orchestrator`, `--fg` for
`--foreground`, `-V` for `--version`, and `-h` for `--help`.

### Configuration and projects

```text
neomax config show [--json]
neomax config models [--json]
neomax config set max-subagents N
neomax config set max-sessions-per-account N
neomax config set-model ENGINE MODEL
neomax config set model ENGINE MODEL
neomax config unset-model ENGINE
neomax projects
neomax project-register [OPTIONS]
neomax project-unregister NAME
```

`project-register` accepts `--name NAME`, `--root PATH`, `--prefix PREFIX`,
`--repos PATHS`, `--desc TEXT`, `--brain PATH`, `--agents PATH`,
`--orch-brain PATH`, `--opener PATH`, `--planning PATH`, and `--force`. The
default root is the current directory. `project-unregister` removes only the
registry entry and leaves files on disk.

Every provider also receives the `/project` workflow. It lists the registry
with `neomax projects`, focuses the session by changing into the selected
project root, and refreshes the selected project context with `neomax orient`
inside an orchestrator session. The workflow is provider-neutral and uses the
canonical agent-tool entries for `projects`, `project-register`, and `orient`.

Interactive root launches without an initial task receive a compact dynamic
orientation. It reports the selected orchestrator, worker scope, effective
models, concurrency limits, and the registered project's root-relative brain,
agent, orchestrator, and planning locations. A registered opener supplement is
read only when it is a bounded, safe text file inside that project. An explicit
initial task is preserved unchanged; Kimi sends it through the documented
bootstrap because its final root process cannot accept a prompt flag. See
`docs/PROJECT-ORIENTATION.md` for the read boundary and hook behavior.

### Run lifecycle and evidence

```text
neomax status [--json] [--engine ENGINE] [--status STATUS] [--limit N]
neomax list|ls [--json] [--engine ENGINE] [--status STATUS] [--limit N] [--hook]
neomax log RUN_ID [--json]
neomax history [RUN_ID] [--json] [--log] [--engine ENGINE] [--limit N]
neomax resume RUN_ID [CONTINUATION TEXT...] [--json]
neomax retry RUN_ID [ACCOUNT|auto] [PROMPT...] [--json]
neomax kill RUN_ID [--json]
neomax kill --all [--json]
neomax diff RUN_ID [--json] [--patch]
neomax subagent-diff AGENT_OR_SESSION_ID [--json] [--patch]
```

`status` reports active, orphaned, inbox, and historical run state. `list`
filters the run ledger, while `--hook` limits the result to unfinished work
for an interactive hook. `log` reads only Neomax-owned log directories.
`history` reads archived runs and can include an archived log. `resume` keeps
the recorded provider session; `retry` selects a different eligible account
when requested and increments the attempt. A live or orphaned run must be
killed before resume or retry. `diff` compares a managed run branch with its
base, and `subagent-diff` reads recorded child edits.

### Sessions, subagents, and usage

```text
neomax sessions [--days N] [--limit N] [--project NAME] [--engine ENGINE]
                [--active | --terminal] [--json]
neomax subagents [--days N] [--limit N] [--project NAME] [--engine ENGINE]
                  [--active | --terminal] [--json]
neomax usage [--days N | --since 90s|37m|2h|3d|1w | --all] [--json]
```

Session and subagent discovery reads local artifacts for every supported
provider. The default window is three days, with limits of 60 sessions and 80
subagents; `--active` and `--terminal` cannot be combined. `usage` reads the
local deduplicated ledger, defaults to 30 days, and reports input, output,
reasoning, requests, completions, errors, rate limits, cost, provider,
account, model, session, and agent dimensions.

### Orchestrator selection and account controls

```text
neomax orchestrators [--json]
neomax orch-register [--session ID] [--pid PID] [--engine ENGINE]
                     [--account ACCOUNT] [--dir PATH] [--model MODEL]
                     [--reserved] [--json]
neomax orch-unregister [--session ID] [--json]
neomax pick-orch [--engine ENGINE] [--dedicated] [--json]
neomax pick-neomax [--engine ENGINE] [--priority ENGINES]
                    [--cwd PATH] [--resume] [--dedicated] [--record] [--json]
neomax select [--engine ENGINE] [--priority ENGINES]
             [--cwd PATH] [--resume] [--dedicated] [--record] [--json]
neomax why [--engine ENGINE] [--priority ENGINES]
           [--cwd PATH] [--resume] [--dedicated] [--record] [--json]
neomax orch-on [--engine ENGINE] [--cwd PATH] [--json]
neomax modes [--json]
neomax pause ACCOUNT|all [--engine ENGINE] [--json]
neomax unpause ACCOUNT|all [--engine ENGINE] [--json]
neomax paused [--engine ENGINE] [--json]
```

`pick-orch` selects one provider-pinned orchestrator. `pick-neomax`, `select`,
and `why` use the dynamic selector and can report the selection reason;
`--record` persists the selected engine for the current project. Registry
commands track live orchestrator ownership and session identity. Pause state
removes an account from automatic dispatch until it is explicitly unpaused.

### Rotation and handoff

```text
neomax rotate [RUN_ID...] [--run RUN_ID] [--active] [--all]
              [--engine ENGINE] [--workers SCOPE] [--json] [--dry-run]
neomax rotate-tick [--active] [--all] [--engine ENGINE] [--workers SCOPE]
                   [--run RUN_ID] [--json] [--dry-run]
neomax session-rotate [RUN_ID|SESSION_ID] [--run ID] [--session ID]
                      [--session-id ID] [--engine ENGINE] [--workers SCOPE]
                      [--active] [--all] [--json]
neomax solo-rotate [--profile PATH] [--session ID] [--threshold PERCENT]
                   [--weekly-threshold PERCENT] [--prefer ENGINES]
                   [--arm | --claim | --disarm | --auto] [--json]
neomax solo-setup [--account ACCOUNT] [--json]
neomax rotate-auth DEST --from SOURCE [--engine ENGINE] [--reason TEXT]
                       [--swap] [--json]
neomax rotate-auth --restore DEST [--engine ENGINE] [--reason TEXT] [--json]
neomax rotate-auth --log [--engine ENGINE] [--json]
neomax handoff [--engine ENGINE] [--from SOURCE] [--to TARGET]
               [--source-account SOURCE] [--target-account TARGET]
               [--account TARGET] [--reason TEXT] [--kickoff TEXT]
               [--base PATH] [--workers SCOPE] [--model MODEL]
               [--claude-model MODEL] [--codex-model MODEL]
               [--opencode-model MODEL] [--kimi-model MODEL]
               [--grok-model MODEL] [--run RUN_ID] [--session ID]
               [--check] [--dry-run] [--json]
```

`rotate` handles eligible active runs. `rotate-tick` handles quota-limited
runs by default and active runs with `--active`; `session-rotate` selects a
run or session and stays within its provider. `solo-rotate` manages local
armed state. `rotate-auth` performs an explicit local profile transaction and
keeps an audit record. `handoff` resolves a source and target, preserves
scope, model overrides, and environment, and reports the launch plan. It does
not silently exchange credentials or widen the worker scope.

### Tasks, queue, and scheduler

```text
neomax task|tasks|backlog list [--all] [--all-projects] [--project NAME] [--json]
neomax task add TITLE... [--project NAME] [--note TEXT] [--status STATUS]
neomax task done|start|doing|block|blocked|drop|dropped|reopen|todo|merge|merged TASK_ID...
neomax task status TASK_ID STATUS
neomax task note TASK_ID TEXT...
neomax task link TASK_ID RUN_ID
neomax task rm TASK_ID...
```

Task statuses are `todo`, `doing`, `blocked`, `done`, `merged`, and `dropped`.
Mutations update the local durable task store; list is the task operation that
supports `--json`.

```text
neomax queue status [--json]
neomax queue reserve --task TASK_ID --agents N [--batch NAME] [--json]
neomax queue poll (--id ID | --task TASK_ID) [--json]
neomax queue release (--id ID | --task TASK_ID)
neomax queue set-budget [--agents N] [--tasks N]
```

The queue enforces the shared agent and task budgets, grants partial
reservations when capacity is limited, and reclaims stale sessions.

Run-all and guarded direct worker dispatch also share one atomic admission
ledger in `dispatch-admission.json` under `NEOMAX_HOME`. It reserves fleet,
task, provider, account-lane, and session capacity under one lock before a
worker is selected or launched. The lease covers the pre-PID window, is
released on terminal completion, and is reclaimed by owner liveness or the
configured TTL after a crash. A fleet cap of `0` denies every new managed
worker. Capacity is checked at the ledger boundary rather than from a stale
snapshot; the existing queue still preserves FIFO allocation. The scheduler's
`--max-live` value is a separate per-plan ceiling and must fit within the
shared admission limits.

```text
neomax run-all PLAN.json [--repo PATH] [--base REF]
                [--integration-branch BRANCH] [--plan-id ID]
                [--workers SCOPE] [--max-live N]
                [--max-stall-cycles N] [--max-attempts N]
                [--max-ticks N] [--wait] [--json]
neomax run-all attach PLAN_ID [RUNTIME OPTIONS] [--wait] [--json]
neomax run-all tick PLAN_ID [RUNTIME OPTIONS] [--json]
neomax run-all interrupt PLAN_ID [--error TEXT] [--json]
neomax run-all recover PLAN_ID [RUNTIME OPTIONS] [--json]
neomax run-all status [PLAN_ID] [--json]
neomax run-all list [--json]
```

`RUNTIME OPTIONS` means `--workers SCOPE`, `--max-live N`,
`--max-stall-cycles N`, `--max-attempts N`, and `--max-ticks N`.

The scheduler plan is JSON with a nonempty `parts` array. Each part can set
its provider, model, prompt, dependencies, and area lock. `run-all` starts a
detached scheduler unless `--wait` is supplied. `attach`, `tick`, `interrupt`,
`recover`, `status`, and `list` operate on the durable plan record.

### Review, pull requests, CI, and issues

```text
neomax shepherd [RUN_ID] [--repo PATH] [--branch BRANCH] [--base REF]
                [--expect SHA] [--merge] [--json]
neomax premerge [REPO] [--repo PATH] [--base REF] [--json]
neomax pr [RUN_ID] [--repo PATH] [--branch BRANCH] [--base REF]
           [--expect SHA] [--title TEXT] [--merge] [--json]
neomax ci-sync [--project NAME] [--apply] [--force] [--json]
```

`shepherd` reports ready, waiting, blocked, stopped, already-merged, or
nothing-ahead decisions for a branch. `premerge` refreshes
`origin/<base>` with a bounded `git fetch`, reports how many commits the local
base is behind, and lists other live orchestrators whose project paths overlap
the repository. A failed fetch is reported as unverified and never presented
as proof that the base is current. `premerge` does not stage, commit, push, or
merge anything.

`pr` opens or reuses a draft pull request through the configured adapter. With
`--expect SHA`, it reads the branch head before any push or GitHub operation;
when the SHA differs it reports `stopped` and performs no PR-side mutation.
The PR lifecycle may push the existing branch and open a draft PR, but it
never stages files or creates commits. `shepherd --merge` and `pr --merge`
remain intentionally fail-closed: they report an error and never invoke a
merge command. Merge approval and execution stay an explicit operator action.
`ci-sync` is a dry run by default; `--apply` writes the managed workflow and
`--force` permits replacing a hand-edited workflow.

```text
neomax issue open [TITLE] [--title TITLE] [--body TEXT] [--project NAME] [--repos PATHS]
                 [--severity LEVEL] [--fingerprint VALUE] [--all]
                 [--force-new] [--json]
neomax issue list [--project NAME] [--status STATUS] [--json]
neomax issue show KEY [--json]
neomax issue next [--project NAME] [--batch N] [--all] [--json]
neomax issue claim KEY [--json]
neomax issue release KEY [--any] [--json]
neomax issue set KEY --status STATUS [--json]
neomax issue link KEY [--run RUN_ID] [--pr REPOSITORY=URL] [--json]
neomax issue comment KEY TEXT... [--body TEXT] [--comment TEXT] [--json]
neomax issue close KEY [--comment TEXT] [--json]
neomax issue reconcile [--project NAME] [--json]
```

Issue state is local and cross-repository. `open` deduplicates by fingerprint
unless `--force-new` is used. Claims are tied to live session ownership;
`--any` is required to release another live owner's claim. Issue statuses are
`open`, `claimed`, `fixing`, `blocked`, `done`, and `closed`.

### Reconciliation, cleanup, and maintenance

```text
neomax reconcile [--project NAME] [--limit N] [--heal] [--max N]
                [--max-age-hours HOURS] [--allow-repeat] [--any] [--json]
neomax ack RUN_ID [--all] [--any] [--json]
neomax audit [RUN_ID] [--limit N] [--json]
neomax find KEYWORD... [--json]
neomax clean RUN_ID [--force] [--any] [--json]
neomax clean --done [--force] [--any] [--json]
neomax tidy [--project NAME] [--any] [--automatic] [--dry-run] [--json]
neomax orient [--hook] [--json]
neomax usage-watch [--once] [--rebuild] [--no-backfill] [--json]
neomax keepalive [--once] [--json]
neomax turn-hook [--json]
neomax model-guard [--json]
neomax usage-hook [--json]
```

`reconcile` finds unresolved, orphaned, unacknowledged, and changed work.
`--heal` drains pending history writes and performs bounded, durable repair of
stalled, interrupted, orphaned, errored, or rate-limited runs. A repair is
reserved in the locked `self-heal.json` ledger before its lifecycle action is
started, so a crash or repeated reconciliation cannot dispatch the same repair
without an explicit repeat request. The default is five repairs per run and
five repairs per invocation, with exponential backoff and a six-hour age
window. `--max`, `--max-age-hours`, and `--allow-repeat` adjust those bounds;
the attempt cap remains in force. Orphaned workers are terminated through the
normal kill control before their preserved run and session context is resumed.
No repair discards the run's worktree, branch, session metadata, or task
prompt. `ack` marks terminal runs as reviewed.
`audit` reads the event ledger, and `find` searches durable run metadata.
`clean` archives and removes acknowledged terminal runs after worktree safety
checks, applying the same verified artifact cleanup before a non-forced whole
cleanup. Explicit `clean --force` keeps its force semantics. `tidy` first
purges only verified Git-ignored generated artifacts from managed run
worktrees, then resolves runs that are verified merged and clean.
It never removes dirty, unmerged, or unverifiable source. Use `--dry-run` to
inspect artifact and whole-worktree candidates first. The background usage
agent runs the same safe sweep periodically with `--automatic`, which excludes
killed and resumable run states. `orient --hook`, `turn-hook`,
`model-guard`, and `usage-hook` are interactive orchestrator hooks and stay
quiet or fail closed in a worker context. `usage-watch` invokes the local
usage collector and `keepalive` checks local profiles. With `--json`, `tidy`
reports per-run artifact counts and bytes plus an `artifact_totals` aggregate;
an artifact verification failure is reported as skipped and blocks whole-run
removal for that run.

### Portal, account helpers, and installation

```text
neomax portal [PORTAL ARGS...]
neomax-portal [PORT] [--bind LOOPBACK:PORT] [--home PATH]
              [--state PATH] [--days N] [--max-artifact-bytes N]
neomax-portal --version | -V
neomax-portal --help | -h

cdx login ACCOUNT [oauth|device|api-key|access-token]
ocx login ACCOUNT [PROVIDER] [oauth|api-key]
kmx login ACCOUNT [oauth|device|api-key|choose]
gmx login ACCOUNT [oauth|device|api-key|choose]
cdx|kmx|gmx logout ACCOUNT
ocx logout ACCOUNT [PROVIDER]
cdx|ocx|kmx|gmx status
ocx models [ACCOUNT] [PROVIDER]
kmx|gmx models [ACCOUNT]
cdx|ocx|kmx|gmx whoami [ACCOUNT]
cdx|ocx|kmx|gmx run [ACCOUNT] [--model MODEL] TASK...
cdx|ocx|kmx|gmx --dry-run [--json] OPERATION [ARGS...]
cdx|ocx|kmx|gmx --version | -V
```

The account helpers also accept `orch` as the reserved account selector and
`--json` where the operation returns structured state. Authentication is
owned by the provider CLI. The optional auth mode is selected only when the
provider exposes it. Kimi accepts `global` and `mainland-cn` regions in
positional or option form, including `kmx login 1 oauth global` and
`kmx login 1 --region mainland-cn`.
For Codex, `cdx login ACCOUNT device` maps to the installed
`codex login --device-auth` flow. The device flow stores provider OAuth
credentials, so a completed profile is reported as OAuth-backed local
credential state. `cdx login ACCOUNT access-token` maps to the installed
`codex login --with-access-token` flow. The token is read from stdin, for
example `printf '%s' "$CODEX_ACCESS_TOKEN" | cdx login 2 access-token`, and is
never placed in process arguments or helper output. Codex `status` and
`whoami` expose only a sanitized local account label. They warn when separate
profiles resolve to the same account identity because one refresh-token family
can invalidate the other profile.
Kimi 0.38 supports managed Kimi Code OAuth through its device-code `login`
command and supports API-key providers through `config.toml`. `kmx login
ACCOUNT oauth` delegates the official device flow. `kmx login ACCOUNT api-key`
configures a private Kimi provider from `NEOMAX_KIMI_API_KEY` or `KIMI_API_KEY`
without putting the key in process arguments. A Kimi profile is eligible for
orchestrators and workers when its local credentials file contains a non-empty
OAuth access or refresh token, or its valid TOML configuration contains a
non-empty provider `api_key` or recognized provider environment fallback. Empty,
malformed, missing, service-only, and unrelated configuration values remain
unauthenticated. Neomax reads this local evidence only; it never prints or
copies credential values and never probes Kimi during profile discovery.
OpenCode uses `opencode auth login --provider PROVIDER` and supports both
API-key (`--method api`) and OAuth (`--method oauth`) login. Its `models`
operation reads the local provider registry, and `ocx logout ACCOUNT PROVIDER`
maps to `opencode auth logout PROVIDER`. The portal command
forwards arguments to the sibling
`neomax-portal` executable, which serves the combined provider view over
loopback.

`neomax-worktrees` is the standalone provider-neutral worktree utility. It is
not a Claude-only command, and no provider-prefixed worktree utility is
installed.

All account helpers accept `--dry-run`, `--json`, `--model MODEL` or `-m`,
`--version` or `-V`, and `--help` or `-h` where the operation parser permits
them. Claude account setup is the pinned `cmax ACCOUNT` form followed by
`/login`; it is not a `cmax login` command.

Portal options are `PORT`, `--bind LOOPBACK:PORT`, `--port PORT`, `--home PATH`,
`--state PATH` or `--neomax-home PATH`, `--days N`,
`--max-artifact-bytes N`, `--version` or `-V`, and `--help` or `-h`.

Portal action execution has a local trust boundary. `NEOMAX_BIN` is an
optional development override read only from the portal process environment;
it is never taken from an HTTP request. When it is set, it must be an absolute
regular executable file with no control characters and must not be a symlink.
An invalid override is rejected before an action is planned. When it is not
set, the portal resolves the installed Neomax executable beside the portal
binary or from the active installation paths. This keeps local development
overrides usable without allowing a relative name or an ambient `PATH` entry
to choose the action executable.

```text
neomax install [--force] [--json] [--no-usage-agent] [--package-root PATH]
               [--install-root PATH] [--bin-dir PATH] [--share-dir PATH]
neomax uninstall [--force] [--json] [--install-root PATH]
                 [--bin-dir PATH] [--share-dir PATH]
```

Install and uninstall are manifest-scoped. On supported platforms, a successful install also
invokes the newly installed `neomax-usage-agent install` command so the local usage collector,
rotation tick, and keepalive maintenance stay live. This service activation is bounded and
best effort. If the service manager cannot start it, the installer warns with the manual
recovery command while keeping the valid file installation. Use `--no-usage-agent` or set
`NEOMAX_NO_USAGE_AGENT=1` for managed or hermetic installs. The installer does not edit shell
profiles for this service step. `neomax uninstall` stops the owned usage service before removing
its executable when that service binary exists.

Install and uninstall never remove provider
profiles, credentials, Neomax state, or unrelated files. Use the precompiled
package for normal installation; the source build and Cargo commands are for
developers.

## Model policy

Defaults are explicit and provider-specific:

| Provider | Default model | Model discovery |
| --- | --- | --- |
| Claude | `claude-fable-5[1m]` | No Neomax model-list command; explicit local-CLI-supported IDs are accepted |
| Codex | `gpt-5.6-sol` | No Neomax model-list command; explicit local-CLI-supported IDs are accepted |
| OpenCode | `opencode/big-pickle` | Best-effort local registry discovery |
| Kimi | `kimi-code/k3` | Best-effort local CLI discovery |
| Grok | `grok-4.6` | Best-effort local CLI discovery |

Every provider accepts an explicit model ID that the selected local CLI
supports. Neomax validates basic shape and lets the provider validate the
provider-specific model. OpenCode IDs must use the qualified `provider/model`
form. Codex keeps `sol`, `terra`, and `luna` aliases. Kimi keeps `k3` and
`k2.7` aliases. Claude Opus is never implicit; select it explicitly when the
connected Claude CLI supports it.

Model overrides are stored separately from the main settings file so a
provider model change does not discard unrelated configuration:

```bash
neomax config show
neomax config models
neomax config set-model opencode opencode/big-pickle
neomax config set-model claude 'claude-fable-5[1m]'
neomax config unset-model opencode
```

The same model resolution applies to the orchestrator, worker pool, native
provider agents, and scheduler plan parts. The provider-specific
`--claude-model`, `--codex-model`, `--opencode-model`, `--kimi-model`, and
`--grok-model` options override individual worker providers, while `--model`
overrides the selected orchestrator. Resolution is deterministic:
command-line model, then persistent model configuration, then the provider
model environment variable, then the strict provider default.
`NEOMAX_DEFAULT_MODEL` is the global environment fallback for Claude when
`NEOMAX_CLAUDE_MODEL` is absent. There is no silent fallback to a different
model.

## Environment reference

Environment values are read when a process starts. A command-line option wins
over an environment value where both configure the same setting. Provider
credential variables are listed for discovery only; keep their values in the
provider's protected credential store or in a protected process environment,
never in a repository file.

### State, routing, and worker scope

| Variable | Effect |
| --- | --- |
| `NEOMAX_HOME` | Runtime state root. Defaults to `~/.neomax`. |
| `XDG_CONFIG_HOME` | Unix-like config root for `neomax/config.toml` and `models.toml`. Defaults to `~/.config`; Windows uses `%APPDATA%\neomax`. |
| `NEOMAX_PROJECTS_CONFIG` | Optional JSON project seed loaded in addition to the durable registry. A relative path is resolved from the launch directory. |
| `NEOMAX_ENGINE_PRIORITY` | Comma or plus-separated dynamic provider order. Unlisted providers are appended in the default order Claude, Codex, Kimi, Grok, OpenCode. `--prefer` or `--priority` takes precedence. |
| `NEOMAX_FLEET` | Inherited worker scope, such as `all` or `codex+opencode,kimi`. The requested `--workers` scope is intersected with it. |
| `NEOMAX_NO_WORKTREE` | Any non-empty value makes the launch default to the current project directory. `--no-worktree` is the explicit form. |
| `NEOMAX_ALLOW_WORKER_DISPATCH` | Set to `1` to authorize the guarded `dispatch` or `auto` worker path when no injected worker role is present. |
| `NEOMAX_CI_IGNORE_BILLING` | Shepherd merge readiness ignores non-running billing checks unless this is set to `0`; provider failures remain blocking. |

### Concurrency and budgets

The persistent source of truth is `[concurrency]` in one user config file. On
Unix-like systems it is `~/.config/neomax/config.toml`, or the equivalent
`XDG_CONFIG_HOME` path when that variable is set. On Windows it is
`%APPDATA%\neomax\config.toml`.
Defaults are `max_subagents = 50`, `max_tasks = 0` (unlimited),
`max_sessions_per_account = 10`, `lanes_per_account = 6`, and
`queue_ttl_seconds = 43200`.

| Variable | Effect |
| --- | --- |
| `NEOMAX_MAX_SUBAGENTS` | One-process override for the global subagent budget. |
| `NEOMAX_LIVE_CAP` | One-process override for the maximum live sessions per account. |
| `NEOMAX_MAX_LIVE` | Fleet-wide live-worker cap for managed admission, including `run-all` and guarded direct worker dispatch. It can lower, but never raise, the global subagent budget. `0` denies managed worker dispatch. |
| `NEOMAX_FLEET_CAP` | Compatibility input for the same fleet-wide managed-worker cap. It can lower, but never raise, the global subagent budget. `0` denies managed worker dispatch. It is not a subagent-budget alias. |
| `NEOMAX_MAX_TASKS` | One-process override for the concurrent task cap. It is also exported to child agents. Set `max_tasks` in config for the persistent source of truth. |
| `NEOMAX_MAX_SESSIONS_PER_ACCOUNT` | Canonical live-session cap passed to child agents. It may also be set directly; it takes precedence over the compatibility `NEOMAX_LIVE_CAP` name. |
| `NEOMAX_LANES_PER_ACCOUNT` | One-process override for lanes per account. It is also exported to child agents. Set `lanes_per_account` in config for the persistent source of truth. |
| `NEOMAX_QUEUE_TTL_SECONDS` | One-process override for the queue reservation TTL in seconds. It must be finite and positive and is exported to child agents. Set `queue_ttl_seconds` in `[concurrency]` for the persistent source of truth. |

`NEOMAX_AGENT_BUDGET` remains a compatibility input for the subagent budget.
Prefer `NEOMAX_MAX_SUBAGENTS` for new scripts. `NEOMAX_MAX_LIVE` and
`NEOMAX_FLEET_CAP` configure the distinct fleet-wide managed-worker cap; they
do not change `max_subagents` and cannot raise its global ceiling. The
scheduler's `--max-live` is a per-plan ceiling within those shared limits.
Queue reservations and run-all caps are
set with their command options and are persisted under `NEOMAX_HOME`.

The older `NEOMAX_TASK_BUDGET`, `NEOMAX_LANES_PER_ACCT`, and
`NEOMAX_QUEUE_TTL` variables remain accepted as aliases for
`NEOMAX_MAX_TASKS`, `NEOMAX_LANES_PER_ACCOUNT`, and
`NEOMAX_QUEUE_TTL_SECONDS`. The canonical name wins when both names are set.
Task budgets may be zero to mean unlimited; lane and TTL values must be
positive, and TTL values must be finite seconds.

`NEOMAX_MAX_SESSIONS_PER_ACCOUNT` is the canonical propagated form of the
per-account live-session cap. `NEOMAX_LIVE_CAP` remains the compatibility
process override for users who want to set that value at launch time. The
canonical name wins when both are present, which keeps a resolved parent
setting authoritative in child processes.

### Provider binaries, profiles, and models

Each provider has a binary override, a profile search override, an optional
reserved orchestrator profile override, a model override, and an upstream
configuration variable. Profile variables accept the platform path-list format.

| Provider | Binary | Profile roots | Orchestrator profile | Model | Upstream config |
| --- | --- | --- | --- | --- | --- |
| Claude | `NEOMAX_CLAUDE_BIN` | `NEOMAX_PROFILES`, default `.claude` | `NEOMAX_CLAUDE_ORCH`, default `.claude-orch` | `NEOMAX_CLAUDE_MODEL` | `CLAUDE_CONFIG_DIR` |
| Codex | `NEOMAX_CODEX_BIN` | `NEOMAX_CODEX_PROFILES`, default `.codex` | `NEOMAX_CODEX_ORCH`, default `.codex-orch` | `NEOMAX_CODEX_MODEL` | `CODEX_HOME` |
| OpenCode | `NEOMAX_OPENCODE_BIN` | `NEOMAX_OPENCODE_PROFILES`, default `.opencode` | `NEOMAX_OPENCODE_ORCH`, default `.opencode-orch` | `NEOMAX_OPENCODE_MODEL` | `XDG_DATA_HOME` |
| Kimi | `NEOMAX_KIMI_BIN` | `NEOMAX_KIMI_PROFILES`, default `.kimi-code` | `NEOMAX_KIMI_ORCH`, default `.kimi-code-orch` | `NEOMAX_KIMI_MODEL` | `KIMI_CODE_HOME` |
| Grok | `NEOMAX_GROK_BIN` | `NEOMAX_GROK_PROFILES`, default `.grok` | `NEOMAX_GROK_ORCH`, default `.grok-orch` | `NEOMAX_GROK_MODEL` | `GROK_HOME` |

Explicit profile and orchestrator paths may be symlinks to directories; Neomax
resolves those links before use. Derived account paths stay under the resolved
configured root and fail closed on parent traversal, broken links, or symlink
escapes.

The model variables participate after persistent `models.toml` and before the
strict defaults, as described in the model policy above. The provider CLIs
remain authoritative for the set of locally supported model IDs. Local
credential discovery recognizes `ANTHROPIC_API_KEY` or
`ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY` or `CODEX_API_KEY`,
`OPENCODE_API_KEY`, `OPENCODE_ZEN_API_KEY`, or `OPENAI_API_KEY`,
`KIMI_API_KEY` or `KIMI_MODEL_API_KEY`, and `XAI_API_KEY`, `GROK_API_KEY`,
or `GROK_DEPLOYMENT_KEY`. The Kimi API-key helper also accepts
`NEOMAX_KIMI_API_KEY`; the Grok API-key helper accepts `NEOMAX_GROK_API_KEY`.

### Installation and portal

| Variable | Effect |
| --- | --- |
| `NEOMAX_PACKAGE_ROOT` | Package directory used by `neomax install`; otherwise it is derived from the executable location. |
| `NEOMAX_INSTALL_ROOT` | Installation root. Defaults to `~/.local` on Unix-like systems and the user's local application directory on Windows. |
| `NEOMAX_INSTALL_BIN` | Override the installed command directory. |
| `NEOMAX_INSTALL_SHARE` | Override the installed asset directory. |
| `NEOMAX_NO_USAGE_AGENT` | Any value skips automatic usage-agent service activation during `neomax install`. |
| `NEOMAX_PORTAL_BIN` | Absolute path to the `neomax-portal` executable used by `neomax portal`. |
| `NEOMAX_PORTAL_NO_USAGE_AGENT` | Any value skips the portal's bounded usage-agent startup repair. |

The equivalent installer flags are `--package-root`, `--install-root`,
`--bin-dir`, and `--share-dir`. Portal state can also be selected with its
`--home` or `--state` options.

### Worktree and usage-agent tuning

| Variable | Effect |
| --- | --- |
| `NEOMAX_PROJECT_DIR` | Worktree project-root override. |
| `NEOMAX_REPOS` | Comma or whitespace-separated repository list. |
| `NEOMAX_BRANCH_PREFIX` | Branch prefix override. |
| `NEOMAX_WORKTREE_ROOT` | Coordinated worktree storage override. |
| `NEOMAX_DRY_RUN` | Any value forces `neomax-worktrees` to plan without changing Git or the filesystem. |
| `NEOMAX_USAGE_POLL` | Usage-agent sweep interval in seconds. Default 3, range 1 to 86400. |
| `NEOMAX_USAGE_RECENT_DAYS` | Recent-history scan window in days. Default 2, range 0 to 3650. |
| `NEOMAX_ROTATE_TICK` | Automatic rotation maintenance interval in seconds. Default 30, range 1 to 86400. |
| `NEOMAX_KEEPALIVE_EVERY` | Keepalive maintenance interval in seconds. Default 480, range 1 to 86400. |
| `NEOMAX_WORKTREE_TIDY_EVERY` | Background safe worktree cleanup interval in seconds. Default 600. Set to `0` to disable only the periodic tidy sweep. |
| `NEOMAX_WORKTREE_TIDY_TIMEOUT_SECS` | Timeout for one background tidy sweep. Default 300, range 1 to 3600. |
| `NEOMAX_MAINTENANCE_TIMEOUT_SECS` | Rotation and keepalive action timeout. Default 30, range 1 to 300. |
| `NEOMAX_USAGE_AGENT_BIN` | Usage-agent executable used by maintenance commands and installed services. |
| `NEOMAX_CLI_BIN` | `neomax` executable used by the usage agent and maintenance commands. |
| `NEOMAX_PROFILE` | Profile path used by `solo-rotate` when `--profile` is not supplied. |

The worktree command also accepts `--project-dir`, `--repos`,
`--branch-prefix`, `--worktree-root`, `--home`, `--base`, `--dry-run`, and
`--json`. Without overrides it uses the registered project, the current Git
root, or immediate child repositories, and stores coordinated worktrees under
`$NEOMAX_HOME/coordinated-worktrees/PROJECT`.

### Agent-managed variables

Neomax injects these variables into the root tool environment and guarded
workers: `NEOMAX_BIN`, `NEOMAX_TOOL_MANIFEST`, `NEOMAX_TOOL_POLICY`,
`NEOMAX_TOOL_INSTRUCTION`, `NEOMAX_TOOL_DEPTH`, and
`NEOMAX_TOOL_MAX_DEPTH`. `NEOMAX_ROLE`, `NEOMAX_ORCHESTRATOR`, and
`NEOMAX_WORKER` identify the launch role. They are process-contract values,
not ordinary user settings. Do not hand-edit them to bypass worker policy.
The sole deliberate exception is the documented, explicit full-policy opt-in
below. Use `NEOMAX_ALLOW_WORKER_DISPATCH=1` when an explicitly authorized
headless dispatch is required.

Other managed handoff values include `NEOMAX_ENGINE`, `NEOMAX_MODE`,
`NEOMAX_MODEL`, `NEOMAX_ORCHESTRATOR_INSTRUCTION`,
`NEOMAX_WORKERS`, `NEOMAX_PROJECT_ROOT`, `NEOMAX_PROJECT`,
`NEOMAX_BRANCH_PREFIX`, `NEOMAX_SESSION_ID`, `NEOMAX_ORCH_SESSION`,
`NEOMAX_ORCH_PID`, `NEOMAX_ORCH_RESERVED`, `NEOMAX_ACCOUNT`,
`NEOMAX_INVOKED_AS`, `NEOMAX_LAUNCH_HANDSHAKE`, and the internal
`NEOMAX_ATTACHED_CHILD` recursion marker. They are recorded or injected to
preserve ownership across a handoff and should not be treated as user
configuration.

## One-place concurrency configuration

The normal user setting is one file. On Unix-like systems use
`~/.config/neomax/config.toml`, or the equivalent `XDG_CONFIG_HOME` path when
that variable is set. On Windows use `%APPDATA%\neomax\config.toml`.

```text
Unix/macOS: ~/.config/neomax/config.toml
Windows:    %APPDATA%\neomax\config.toml
```

Example:

```toml
[concurrency]
max_subagents = 50
max_tasks = 0
max_sessions_per_account = 10
lanes_per_account = 6
queue_ttl_seconds = 43200
```

`NEOMAX_MAX_SUBAGENTS` overrides the persistent value for one shell, CI job,
or parent agent. The resolved value is injected into every orchestrator and
worker environment and enforced by the shared queue and admission policy.

```bash
neomax config show
neomax config set max-subagents 80
neomax config set max-sessions-per-account 12
NEOMAX_MAX_SUBAGENTS=120 neomax --dry-run --json
NEOMAX_MAX_SESSIONS_PER_ACCOUNT=12 neomax --dry-run --json
neomax queue status
neomax queue reserve --task TASK_ID --agents 4
neomax queue poll --task TASK_ID
neomax queue release --task TASK_ID
```

The queue persists reservations, grants, task caps, session identity, TTL
reaping, and crash recovery under `NEOMAX_HOME`. A worker cannot bypass the
global subagent budget by spawning another Neomax worker.

## Agent tool access

Root interactive provider sessions and headless workers receive the same
canonical Neomax tool manifest through the `NEOMAX_BIN`,
`NEOMAX_TOOL_MANIFEST`, `NEOMAX_TOOL_INSTRUCTION`, `NEOMAX_TOOL_DEPTH`, and
`NEOMAX_TOOL_MAX_DEPTH` environment variables. The manifest covers accounts,
configuration, dispatch, Git and worktrees, help, issues and CI, lifecycle,
orchestration, projects, queue, sessions, tasks, usage, and workers. It is
provider-neutral, so an OpenCode orchestrator can inspect status, manage
tasks, use the queue, coordinate worktrees, and hand off to any other eligible
provider. A Claude, Codex, Kimi, or Grok orchestrator gets the same surface.

The canonical manifest entries are grouped as follows. Account entries are
tool calls resolved by the Neomax executable; the provider account launchers
are documented separately below.

```text
help
config show | config set
dispatch
account status | account pause | account unpause | account rotate
pause | unpause
diff | subagent-diff | premerge | clean | tidy | ls
issue | ci-sync | ack | reconcile
resume | retry | kill | handoff | shepherd
orchestrators | modes | pick-orch | pick-neomax | orch-register
orch-unregister | orch-on | orient | select | why | run-all
rotate | rotate-tick | solo-rotate | solo-setup
projects | project-register | project-unregister
queue
sessions | session-rotate | subagents | portal
tasks | task
usage | usage-watch | keepalive
log | audit | find | history | status | paused | rotate-auth | pr
turn-hook | model-guard | usage-hook
install | uninstall
```

The root interactive session receives the orchestrator policy and inherited
terminal I/O. Headless workers receive the restricted worker policy and are
started by the guarded worker coordinator. Both paths use the same manifest
boundary, but a worker cannot silently become another user-facing
orchestrator.

The default policies are least privilege. Workers can inspect and mutate
project state but cannot dispatch more workers, perform external delivery, or
run destructive cleanup. Orchestrators can dispatch and perform other
external coordination but cannot run destructive commands. To intentionally
grant every canonical command to a trusted local session, set both variables
in the same shell or local environment file before starting Neomax:

```bash
NEOMAX_TOOL_POLICY=full
NEOMAX_ALLOW_FULL_TOOL_POLICY=1
neomax
```

The full policy includes destructive cleanup, installation, uninstallation,
and external operations. It is rejected unless the explicit opt-in is set,
and an unknown policy name always fails closed. The selected policy and its
opt-in are validated at each child boundary and carried through an approved
handoff, so an ordinary worker cannot escalate itself. Auxiliary commands
such as `neomax-portal`, `neomax-usage-agent`, and `neomax-worktrees` remain
separate local utilities; agents use the canonical `portal`, `usage-watch`,
and Git/worktree commands through `NEOMAX_BIN` and never need to guess an
executable name.

Tool authorization is policy based. Read-only inspection, mutating project
operations, external delivery, and destructive operations are separate command
classes. Worker processes receive the restricted worker policy; the parent
orchestrator controls escalation. Recursion depth and manifest validation are
enforced before a child process is started.

## Project state and portability

The directory where the launcher starts is the default project root. Neomax
does not contain built-in customer repositories, usernames, home paths, or
machine-specific project definitions. The portable project registry can
discover the current repository and immediate child repositories, while local
project seeds remain outside the tracked product surface.

Runtime state lives under `~/.neomax` by default. Set `NEOMAX_HOME` to relocate
it. The main settings file uses `XDG_CONFIG_HOME` on Unix-like systems and
falls back to `~/.config/neomax/config.toml`; on Windows it uses
`%APPDATA%\neomax\config.toml`. Authentication remains in the upstream
provider profile locations and is never copied into the repository.

## Portal

`neomax-portal` is a loopback dashboard over the same universal status model.
It does not maintain a second provider-specific state model. The portal always
includes every supported provider in one view, whether a provider has no
profile, one profile, or several profiles. GET endpoints provide status,
history, usage, sessions, subagents, projects, tasks, queue, logs, and diffs.
Guarded same-origin POST actions can connect an account, pause or unpause an
account, and request run resume, retry, acknowledgement, kill, or cleanup.
Destructive actions require explicit confirmation, and action inputs are
validated before the local Neomax command is spawned.

```bash
neomax-portal
neomax-portal 8787
neomax-portal --bind 127.0.0.1:8787 --days 30
neomax-portal --state /path/to/neomax-state
```

The dashboard and JSON endpoints cover account eligibility, authentication
state, quota and telemetry, cooldowns, pauses, active orchestrators, runs,
history, projects, tasks, queue state, usage, sessions, subagents, logs, and
run diffs. It binds to loopback by default, rejects cross-origin local actions,
and keeps read endpoints separate from mutating requests.

Portal startup launches the bounded `neomax-usage-agent ensure` repair path in
the background. It reuses an already active service and only asks the platform
service manager to install or start a missing one. On macOS, an already loaded
launchd service is reused. The portal remains usable
for read-only requests if that child cannot start. Set
`NEOMAX_PORTAL_NO_USAGE_AGENT` when the collector is managed separately.

## Usage agent

`neomax-usage-agent` collects local usage evidence for all five providers,
including provider transcripts, Kimi wire records, Grok session updates, and
OpenCode's local database. Ledger ingestion, normalization, deduplication, and
date-partitioned writes are local. A `once` or `run` cycle can also make the
bounded Claude quota request documented in `docs/USAGE-AGENT.md` and can run
bounded provider CLI discovery such as `--version`, OpenCode `models`, Kimi
`provider list`, or Grok `models`. A provider CLI may perform its own network
operation for one of those discovery commands. Neomax never sends a model
prompt or transmits ledger records, transcripts, prompts, or model output.
It writes the shared date-partitioned ledger with deduplication and preserves
partial files for the next sweep.

```bash
neomax-usage-agent status
neomax-usage-agent ensure
neomax-usage-agent once
neomax-usage-agent once --rebuild
neomax-usage-agent once --no-backfill --json
neomax-usage-agent run
neomax-usage-agent run --rebuild --no-backfill --once --json
neomax-usage-agent install
neomax-usage-agent uninstall
neomax-usage-agent --version | -V
neomax-usage-agent --help | -h
```

`status` and `ensure` accept `--json`. `once` accepts `--rebuild`,
`--no-backfill`, and `--json`. `run` accepts those same collection options
plus `--once`.

On supported systems, `install` creates a user service and `status` reports its
state. macOS uses launchd, Linux uses a user systemd unit, and Windows uses a
per-user Task Scheduler task. Other platforms can run `once` or `run` directly
until a native service integration is added. Ledger parsing and quota
maintenance do not send inference requests. Startup catalog discovery can run
bounded local provider status or model-list commands, and the Claude quota
path can make its documented bounded usage request; neither path runs an
authenticated model prompt or sends ledger data.

## Coordinated worktrees

`neomax-worktrees` creates one task worktree per discovered Git repository in a
project. It validates repository labels, branch names, symlinks, path ancestry,
existing worktrees, and dirty state before changing anything.

```bash
neomax-worktrees TASK --dry-run
neomax-worktrees TASK
neomax-worktrees --list
neomax-worktrees --remove TASK --dry-run
neomax-worktrees --remove TASK
neomax-worktrees TASK feature/TASK --base main
neomax-worktrees TASK --repos path/to/repo-a,path/to/repo-b
neomax-worktrees TASK --json
neomax-worktrees --version | -V
neomax-worktrees --help | -h
```

Removal is refused for a dirty, detached, unregistered, or unmerged worktree.
Committed changes ahead of the selected base are preserved. Creation and
removal preflight all repositories and roll back already completed Git changes
when a later operation fails. Review the JSON plan before a destructive
operation.

The public options are `--list`, `--remove TASK`, `--dry-run`, `--json`,
`--project-dir PATH`, `--repos REPOS`, `--branch-prefix PREFIX`,
`--worktree-root PATH`, `--home PATH`, `--base REF`, `--version` or `-V`, and
`--help` or `-h`.

With no project override, the command uses the registered project containing
the current directory, the current Git repository root, or the current
directory when no Git root is available. A project that is itself a Git
repository contributes that repository; otherwise immediate child
repositories are discovered. The default branch prefix is the registered
prefix or the first four characters of the project name, with `proj` as the
fallback. The default storage root is
`$NEOMAX_HOME/coordinated-worktrees/PROJECT`.

## Install and distribution

### Install a release

Precompiled releases do not require Rust or Cargo. The bootstrap installers
select the host package, download it from GitHub Releases, verify it against
the published `SHA256SUMS`, reject unsafe archive layouts, and invoke only the
packaged `neomax install` command. They do not edit shell profiles, read
provider credentials, or invoke a provider.

Linux and macOS bootstrap:

```bash
curl --fail --silent --show-error --location https://neotask.ai/neomax_orchestrator_rust/install.sh -o install.sh
bash install.sh
```

Windows PowerShell bootstrap:

```powershell
Invoke-WebRequest -Uri https://neotask.ai/neomax_orchestrator_rust/install.ps1 -OutFile install.ps1
.\install.ps1
```

Set `NEOMAX_VERSION=0.1.0` or `$env:NEOMAX_VERSION = '0.1.0'` for an exact release. The default
is the latest release. Set `NEOMAX_BASE_URL` for an offline mirror whose `vVERSION` directories
contain the archive and `SHA256SUMS`; use `NEOMAX_LATEST_URL` when latest metadata also comes from
that mirror. The installers print the exact PATH command and do not modify a profile. Rerun them
with a new `NEOMAX_VERSION` to upgrade. Run `neomax uninstall` to remove the owned installation.

### Build and run from source

Source builds require Git, Rust 1.85 or newer, Cargo, and a native linker.
Rustup installs Rust and Cargo together. Follow the official
[Rust installation guide](https://www.rust-lang.org/tools/install). On Linux
or macOS, the official rustup command is:

```bash
curl --proto '=https' --tlsv1.2 --silent --show-error --fail https://sh.rustup.rs | sh
```

Ubuntu and Debian users can install the source-build prerequisites with:

```bash
sudo apt update
sudo apt install -y build-essential git curl
```

macOS source builds require the Xcode command-line tools, available with
`xcode-select --install`. Other Linux distributions require Git, curl, and a C
toolchain such as GCC or Clang. On Windows, install Rustup and the Visual Studio
C++ build tools from the official Rust installer.

Clone, build, test, and run the universal CLI from the checkout:

```bash
git clone https://github.com/NeotaskInc/neomax-orchestrator-rust.git
cd neomax-orchestrator-rust
cargo build --workspace --locked --release
cargo test --workspace --locked
./target/release/neomax --help
```

On Windows, run `.\target\release\neomax.exe --help`. A source build leaves
the executables under `target/release`; it does not modify the user account or
install provider workflows. Use the verified release installer for the full
multicall command surface and provider workflow installation. Developers can
build a complete package from source with the commands in
[dist/README.md](dist/README.md).

The normal install also leaves shell startup files unchanged. On Unix-like systems using zsh,
enable the optional dynamic account shortcuts explicitly:

```bash
NEOMAX_SHARE="${NEOMAX_INSTALL_SHARE:-$HOME/.local/share/neomax}"
sh "$NEOMAX_SHARE/shell/neomax-shell-shortcuts.sh" install \
  --profile "$HOME/.zshrc" \
  --asset "$NEOMAX_SHARE/shell/neomax-aliases.zsh"
```

This adds `claudeN`, `codexN`, `opencodeN`, `kimiN`, and `grokN` functions that rediscover
profiles in each new shell and call the canonical installed helpers. Remove only the managed
profile block with `sh "$NEOMAX_SHARE/shell/neomax-shell-shortcuts.sh" uninstall --profile "$HOME/.zshrc"`.
Run that explicit uninstall before removing Neomax if the profile still contains the managed
block; `neomax uninstall` does not edit user shell startup files.

The archive contains the `neomax` multicall executable, provider aliases,
the `neomax-cli` compatibility alias, `neomax-portal`, `neomax-usage-agent`,
`neomax-worktrees`, the OpenCode policy, the optional zsh account-shortcut assets, the installation guide, the license,
the canonical `/neomax`, `/project`, `/rotate`, `/find-issues`, and `/fix-issues` workflow sources, and the
README. The installer renders those workflows into Claude commands, Codex prompts, OpenCode
commands, Kimi skills, and Grok commands for every discovered profile. Unix aliases are relative symlinks to the
multicall executable. Windows packages contain executable copies so symlink
privileges are not required.

Install the user command surface from an extracted package with:

```bash
./bin/neomax install
```

The native installer records its owned files, makes upgrades transactional, and
refuses to replace modified or unrelated files unless `--force` is explicit.
`neomax uninstall` removes only files recorded by that manifest and leaves
provider profiles, credentials, state, unrelated files, and unrelated Claude hooks alone. Claude
settings hooks are merged structurally and are removed only when Neomax owns the exact command.
New profiles created through Neomax are seeded with the same workflows. See
[docs/INSTALLATION.md](docs/INSTALLATION.md) for installation paths and
rollback behavior.

The release tooling supports macOS, Linux, and Windows targets. After the
workspace gates are green, package and verify an archive with:

```bash
cargo build --workspace --locked --release --target aarch64-apple-darwin
bash dist/package.sh --target aarch64-apple-darwin
bash dist/check-package.sh \
  --archive target/dist/neomax-v0.1.0-aarch64-apple-darwin.tar.gz \
  --version 0.1.0 --target aarch64-apple-darwin
bash dist/verify-install.sh \
  --archive target/dist/neomax-v0.1.0-aarch64-apple-darwin.tar.gz \
  --version 0.1.0 --target aarch64-apple-darwin
```

The install verifier uses a temporary home and fake provider executables. It
checks version, help, configuration, archive safety, manifest hashes, and all
aliases, then exercises native install and scoped uninstall without invoking a
provider or reading an existing login.

See [docs/INSTALLATION.md](docs/INSTALLATION.md) for source builds, release
installs, upgrades, rollback, and scoped uninstall behavior.

## Development and contribution

Read [AGENTS.md](AGENTS.md) for the provider-neutral development contract,
[CLAUDE.md](CLAUDE.md) for the Claude Code pointer, and
[CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) before
opening an issue or pull request.
Use the
[GitHub issue tracker](https://github.com/NeotaskInc/neomax-orchestrator-rust/issues)
for public reports and its [private security advisory
channel](https://github.com/NeotaskInc/neomax-orchestrator-rust/security/advisories/new)
for vulnerabilities.
The implementation is intentionally modular. `neomax-core` owns domain
behavior, each executable package is a composition root, and
`neomax_core::registry` records the responsibility owner for every public
domain. Tests are split by responsibility and use hermetic fixtures.

For a pull request, add a concise product-safe entry to `WORKLOG.md` on the
branch. Include the user-visible behavior, affected areas, exact verification,
and remaining risk. Do not include credentials, account identities, local
paths, private project names, provider transcripts, or machine-specific state.
The `main` branch keeps `WORKLOG.md` as a blank template so each incoming
change carries its own review record.

## License

Neomax is distributed under the MIT License. See [LICENSE](LICENSE).
