# Neomax Orchestrator

Neomax is a native Rust control plane for Claude, Codex, OpenCode, Kimi, and
Grok. It can select an orchestrator dynamically, route workers across any
connected provider set, use different models for different tasks, rotate work
away from exhausted accounts, track usage, and clean generated worktree data.

## Connect multiple accounts, launch by email

Use the same command to sign in and launch. An authenticated account opens
immediately; an account without credentials starts the provider's login flow
and opens after login succeeds. For example:

```bash
neomax codex 1
neomax codex 2
neomax codex status
```

Choose an orchestrator account by its saved email or number:

| Provider | Launch by email | Launch account 2 |
| --- | --- | --- |
| Claude | `neomax claude developer@example.com` | `neomax claude 2` |
| Codex | `neomax codex developer@example.com` | `neomax codex 2` |
| OpenCode | `neomax opencode developer@example.com` | `neomax opencode 2` |
| Kimi | `neomax kimi developer@example.com` | `neomax kimi 2` |
| Grok | `neomax grok developer@example.com` | `neomax grok 2` |

The account number is a separate argument. Neomax does not replace the native
`codex`, `claude`, or other provider commands. Old launcher and account-helper
aliases remain available for existing scripts.

Email lookup stays within the selected provider. A new email starts login in
an unused profile, and Neomax checks the saved identity before launching.
Ambiguous emails stop with an error. If the provider does not expose an email,
use the account number. Saved profile directory names also work as selectors.
Put initial task text after `--`, for example
`neomax codex 2 -- "Review this project"`.

Rotate a numbered profile without moving its session files:

```bash
neomax codex 2 rotate --dry-run --json
neomax codex 2 rotate --with 4
neomax claude 2 rotate --with developer@example.com
neomax doctor --json
```

Rotation swaps OAuth credentials between eligible accounts of the same
provider. Without `--with`, Neomax selects a replacement using quota,
cooldown, pause, and live-work evidence. Email selectors follow the credentials
after a swap; numbers continue to identify profile directories. Open providers
may need to reload authentication or resume. Swapping credentials alone does
not prove that a running provider continued its task.

`doctor` reads local credential health, quota freshness, configuration, and
tool-manifest evidence. It does not log in, refresh tokens, run providers, or
repair files. A refresh token's presence means refresh may be possible, not
that the provider has accepted it.

## Install

[Latest release](https://github.com/NeotaskInc/neomax-orchestrator-rust/releases/latest)
includes precompiled packages for macOS (Apple Silicon and Intel), Windows
(x64), and Linux (x64 and ARM64, including Ubuntu).

### Give Neomax to your coding agent

Paste this into Claude Code, Codex, OpenCode, Kimi, Grok, or another coding
agent:

```text
Read and follow https://neotask.ai/neomax-orchestrator/SKILL.md
```

The skill installs Neomax, saves itself in the agent's supported skill
location, verifies the CLI without making a model request, and explains the
commands the agent can use.

### Install it yourself

Linux and macOS:

```bash
curl -fsSL https://neotask.ai/neomax-orchestrator/install.sh | bash
```

Windows PowerShell:

```powershell
Invoke-RestMethod https://neotask.ai/neomax-orchestrator/install.ps1 | Invoke-Expression
```

The installer selects the correct precompiled package, verifies its SHA-256
checksum, installs the command aliases and provider workflows, and starts the
local usage service when the operating system supports it. It does not require
Rust, use `sudo`, edit shell profiles, read provider credentials, or start a
provider session.

Verify the installation without starting a provider:

```bash
neomax --version
neomax --help
```

After connecting at least one provider account, `neomax --dry-run --json`
checks routing without starting a provider.

If your shell cannot find `neomax`, run the PATH command printed by the
installer. See [Installation](docs/INSTALLATION.md) for pinned versions,
upgrades, rollback, source builds, and uninstall instructions.

## Start using Neomax

Run Neomax from the project directory you want it to work in:

```bash
cd path/to/project
neomax
```

`neomax` opens the terminal workspace. Choose Launch to start an orchestrator
inside it, or use a direct command:

| Command | Main orchestrator |
| --- | --- |
| `neomax orchestrator` | Dynamic selection, without the TUI |
| `neomax claude` | Claude |
| `neomax codex` | Codex |
| `neomax opencode` | OpenCode |
| `neomax kimi` | Kimi |
| `neomax grok` | Grok |

Pinning the main orchestrator does not restrict its worker pool. These are all
valid:

```bash
neomax orchestrator --workers all
neomax claude --workers codex,opencode,kimi
neomax opencode --workers claude+grok
neomax kimi --model kimi-code/k3 --workers all
```

Inspect a launch before starting it:

```bash
neomax --dry-run --json
neomax --dry-run --engine opencode --workers codex,kimi
cmax --dry-run --workers all
```

A dry run resolves the provider, account, model, worker scope, and agent-tool
environment without starting a provider process.

## Routing and models

Neomax can route by explicit provider or model choices, task and repository
context, scheduler part, connected accounts, current usage, live load, recent
project selection, and provider priority. Explicit choices win.

| Provider | Default model | Launch or sign in |
| --- | --- | --- |
| Claude | `claude-fable-5-1[1m]` | `neomax claude [ACCOUNT]` |
| Codex | `gpt-6-astra` | `neomax codex [ACCOUNT]` |
| OpenCode | `opencode/big-pickle` | `neomax opencode [ACCOUNT]` |
| Kimi | `kimi-code/k3` | `neomax kimi [ACCOUNT]` |
| Grok | `grok-4.6` | `neomax grok [ACCOUNT]` |

Every provider accepts model IDs supported by its local CLI. OpenCode model
IDs use `provider/model`. Claude Opus is opt-in. Codex uses GPT-6 Astra as its
default root model. Codex workers may be assigned a different supported model
explicitly or by the routing policy. The GPT-5.6
family remains available for explicit worker choices: Luna for lightweight
work, Terra for balanced coding work, and Sol for heavier general work.

Set a model for one launch:

```bash
neomax --engine opencode --model provider/model
cmax --model claude-model-id
cdxmax --model gpt-5.6-terra
neomax --codex-model gpt-5.6-luna --kimi-model kimi-model-id
```

Set persistent defaults:

```bash
neomax config models
neomax config set-model claude MODEL
neomax config set-model codex MODEL
neomax config set-model opencode PROVIDER/MODEL
neomax config set-model kimi MODEL
neomax config set-model grok MODEL
neomax config unset-model ENGINE
```

Resolution order is the command-line override, persistent model configuration,
provider model environment variable, then the provider default. Neomax does
not silently substitute another model.

## Accounts

Provider CLIs own authentication. Neomax keeps each account in its provider's
normal profile format and never prints or copies credential values.

```bash
# Sign in if needed, then open the selected account
neomax claude 2
neomax codex 2

# Choose a specific login method when needed
neomax codex login 2 oauth
neomax opencode login 2 PROVIDER oauth
neomax kimi login 2 oauth
neomax grok login 2 oauth

neomax codex status
neomax opencode status
neomax kimi status
neomax grok status
```

The older account helpers remain compatible:

| Helper | Login modes | Other operations |
| --- | --- | --- |
| `cdx` | `oauth`, `device`, `api-key`, `access-token` | `logout`, `status`, `whoami`, `run` |
| `ocx` | `oauth`, `api-key` | `logout`, `status`, `whoami`, `models`, `run` |
| `kmx` | `oauth`, `device`, `api-key`, `choose` | `logout`, `status`, `whoami`, `models`, `run` |
| `gmx` | `oauth`, `device`, `api-key`, `choose` | `logout`, `status`, `whoami`, `models`, `run` |

Use `orch` instead of a number for a reserved orchestrator profile. Run an
account command with `--dry-run --json` to inspect it without invoking the
provider.

Select an existing profile by its local account email or directory alias:

```bash
neomax codex person@example.com
neomax codex person@example.com --dry-run --json
neomax --engine codex --account person@example.com
neomax codex alias:.codex-acct2
```

Email lookup is case-insensitive and provider-specific. Duplicate matches
produce an error instead of choosing another account. Numbered
accounts still work. `alias:NAME` matches an existing profile directory name;
it does not create a new login. Providers must expose email metadata locally
for email selection to work. The new provider commands can start login for a
previously unknown email; legacy aliases still require an existing match.
The upstream `codex` command is unchanged.

Legacy email and alias selectors require Neomax 0.1.1 or newer. The unified
`neomax PROVIDER` commands and terminal workspace are new in 0.1.3.

## Automatic quota survival

Neomax prefers authenticated, unpaused accounts with usable headroom. When a
provider reports a five-hour percentage, 92 percent is the proactive threshold
for new automatic work and 99 percent is the hard wall. Weekly windows use the
99 percent wall. Providers without numeric quota data remain eligible based on
the evidence they expose.

If a managed task reaches a provider limit while running, Neomax preserves the
run, branch, worktree, session, and task state. It continues on another account
from the same provider when possible, then another provider allowed by the
worker scope. It does not restart the task from scratch.

```bash
neomax usage --json
neomax rotate --active --dry-run --json
neomax rotate --active
neomax session-rotate SESSION_ID
neomax pause ACCOUNT --engine ENGINE
neomax unpause ACCOUNT --engine ENGINE
```

`rotate`, `usage`, and `portal` are universal commands. They work regardless
of which pinned launcher started the orchestrator.

## Concurrency

Set the shared limits once:

```bash
neomax config set max-subagents 50
neomax config set max-sessions-per-account 10
neomax config show
```

The config file is `~/.config/neomax/config.toml` on Unix-like systems and
`%APPDATA%\neomax\config.toml` on Windows. One-run overrides include:

```bash
NEOMAX_MAX_SUBAGENTS=20 neomax
NEOMAX_MAX_SESSIONS_PER_ACCOUNT=4 neomax
NEOMAX_MAX_TASKS=8 neomax
```

The queue, direct worker dispatch, and scheduler use the same effective
capacity settings.

## Projects and worktrees

The current directory is the default project. Registration is optional:

```bash
neomax projects
neomax project-register --name my-project --root "$PWD"
neomax project-unregister my-project
neomax orient --json
```

Managed workers use isolated Git worktrees by default. A clean terminal
worktree is removed automatically. Dirty, unmerged, killed, resumable, or
unverifiable work is retained. The background usage service periodically
removes verified ignored build data such as `node_modules`, `target`, and
framework output before disk use grows unchecked.

```bash
neomax tidy --dry-run --any --json
neomax tidy --any --json
neomax-worktrees --list
neomax-worktrees TASK --dry-run
neomax-worktrees TASK
neomax-worktrees --remove TASK --dry-run
```

`neomax tidy` never runs a broad `git clean`, follows a symlink, or removes
unverified source. Set `NEOMAX_WORKTREE_TIDY_EVERY=0` before installing the
usage service to disable only its periodic sweep.

## Common commands

### Runs and evidence

```bash
neomax status --json
neomax list --json
neomax log RUN_ID
neomax history RUN_ID --log
neomax resume RUN_ID
neomax retry RUN_ID auto
neomax kill RUN_ID
neomax diff RUN_ID --patch
neomax reconcile --json
```

### Tasks, queue, and scheduling

```bash
neomax task list --json
neomax task add "Task title"
neomax task start TASK_ID
neomax task done TASK_ID
neomax queue status --json
neomax queue reserve --task TASK_ID --agents 3
neomax run-all PLAN.json --wait
neomax run-all status PLAN_ID --json
```

### Review and repository work

```bash
neomax shepherd RUN_ID --json
neomax premerge --repo . --base main --json
neomax pr RUN_ID --title "Pull request title"
neomax issue list --json
neomax issue next --json
neomax ci-sync
```

### Terminal workspace

Run `neomax` (or `neomax tui`) to open Launch. Press Enter on Start Neomax,
then Enter to confirm. Automatic routing chooses an eligible orchestrator and
opens its interactive terminal in Chat. Provider narrows the choice; More opens
account, model, worker, speed and initial-task settings.

Fleet shows native session history, including sessions opened outside Neomax.
Rows show the last activity date and a Neomax ownership marker when recorded.
Enter opens dates, task text, the last assistant message and tool call from
Claude/Codex transcripts, plus managed-run output when available. Native activity
is not proof of a live process. `v` filters the view; `d` changes its date range.

Accounts lists profiles by email when available, provider counts, authentication,
eligibility and cached shared/model-specific allowances. Use `[` / `]` to filter
providers and `l` to carry an eligible account into Launch. Usage shows recorded
model totals over the last 1, 7 or 30 days, not the calendar month. The persistent
header has its own rolling 24h/7d/30d spend window (`s`) and includes uncached
input, output, cache reads and cache writes across discovered local profiles.
Costs use recorded charges or standard model rates. Other devices, cloud-only
sessions and unrecorded pricing premiums are outside this local estimate;
subscription invoices are separate. Usage details explain coverage limits.

Use left/right or Tab to move through Launch, Chat, Fleet, Tasks, Accounts and
Usage. Up/down selects a row; Enter opens details or edits a launch choice.
While editing a choice, left/right changes its value and Enter saves it.
Esc finishes editing an initial task. `?` opens keyboard help.

Codex uses standard mode by default. Choose Codex speed on the Launch page
or pass `--codex-fast` to opt in for the new orchestrator and its Codex workers.
`--codex-standard` explicitly turns it off. `NEOMAX_CODEX_FAST=1` is the equivalent
process-level choice, including account-helper launches. Neomax does not change
the provider's saved configuration. An open Codex terminal also supports
`/fast on`, `/fast off`, and `/fast status`; see the
[Codex speed reference](https://learn.chatgpt.com/docs/agent-configuration/speed).

On Chat, Enter focuses keyboard input in the provider terminal.
Ctrl+] releases input focus so you can return to the dashboard while it runs.
Page Up/Down scrolls a bounded display history. Native session transcripts remain
the durable history. Quitting asks before stopping the orchestrator opened in
this TUI; sessions opened elsewhere are view-only.

### Local portal and usage service

```bash
neomax portal
neomax-portal
neomax-usage-agent status
neomax-usage-agent once --json
```

The portal binds to loopback by default and combines provider, account, model,
usage, session, subagent, run, scheduler, task, queue, and worktree state.

## Command map

| Area | Commands |
| --- | --- |
| Launch | `neomax`, `cmax`, `cdxmax`, `ocmax`, `kmax`, `gmax`, `dispatch`, `solo` |
| Selection | `select`, `why`, `pick-neomax`, `pick-orch`, `modes` |
| Accounts | `cdx`, `ocx`, `kmx`, `gmx`, `pause`, `unpause`, `paused` |
| Runs | `status`, `list`, `log`, `history`, `resume`, `retry`, `kill`, `diff` |
| Rotation | `rotate`, `rotate-tick`, `session-rotate`, `solo-rotate`, `handoff` |
| Projects | `projects`, `project-register`, `project-unregister`, `orient` |
| Tasks | `task`, `queue`, `run-all`, `reconcile`, `ack`, `audit`, `find` |
| Review | `shepherd`, `premerge`, `pr`, `issue`, `ci-sync`, `subagent-diff` |
| Maintenance | `usage`, `usage-watch`, `clean`, `tidy`, `keepalive` |
| Services | `tui`, `portal`, `neomax-portal`, `neomax-usage-agent`, `neomax-worktrees` |
| Product | `config`, `install`, `uninstall`, `help`, `--version` |

See [Complete command and environment reference](docs/REFERENCE.md) for every
accepted option, lifecycle rule, scheduler shape, environment variable, and
service command.

## Agent tools

Every supported orchestrator receives the installed Neomax workflows and a
canonical tool manifest. Claude gets commands, Codex gets prompts, OpenCode
gets commands, Kimi gets skills and an agent file, and Grok gets commands. An
orchestrator can inspect state, dispatch workers, manage projects and tasks,
rotate accounts, reconcile runs, and open the local portal through the same
Neomax CLI.

The public agent skill is available at
[`https://neotask.ai/neomax-orchestrator/SKILL.md`](https://neotask.ai/neomax-orchestrator/SKILL.md).

## Build from source

Source builds require Git, Rust 1.85 or newer, Cargo, and a native linker:

```bash
git clone https://github.com/NeotaskInc/neomax-orchestrator-rust.git
cd neomax-orchestrator-rust
cargo build --workspace --locked --release
cargo test --workspace --locked
./target/release/neomax --help
```

Use `xcode-select --install` for the macOS linker toolchain. Ubuntu and Debian
users can install `build-essential`, `git`, and `curl`. Windows source builds
use Rustup and the Visual Studio C++ build tools.

## Development

Read [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[Project orientation](docs/PROJECT-ORIENTATION.md) before changing the code.
The repository keeps provider behavior in `neomax-core` and executable crates
as thin composition roots.

Run the complete local gate before opening a pull request:

```bash
bash scripts/check-doc-style.sh
bash scripts/check-product-surface.sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
```

Pull requests must include a product-safe `WORKLOG.md` entry describing the
change, affected files, verification, and remaining risk. Issues and pull
requests are welcome at
[NeotaskInc/neomax-orchestrator-rust](https://github.com/NeotaskInc/neomax-orchestrator-rust).

## Documentation

- [Complete reference](docs/REFERENCE.md)
- [Installation and upgrades](docs/INSTALLATION.md)
- [Usage agent](docs/USAGE-AGENT.md)
- [Scheduler capacity](docs/SCHEDULER-CAPACITY.md)
- [Behavioral parity](docs/PARITY.md)
- [Distribution](dist/README.md)

## License

MIT. See [LICENSE](LICENSE).
