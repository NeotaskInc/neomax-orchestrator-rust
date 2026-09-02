# Neomax Rust development guide

This is the provider-neutral development contract for the native Rust
implementation of Neomax. Read it before changing source, tests, installers,
distribution files, or documentation. The repository must remain portable,
modular, privacy-safe, and behaviorally compatible with the Neomax contracts
described in `README.md`.

## Product boundaries

- Neomax coordinates Claude, Codex, OpenCode, Kimi, and Grok through one
  provider-neutral core. The universal launcher and the five provider-pinned
  launchers are separate public surfaces over the same domains. A pinned
  launcher selects the main orchestrator only; `--workers all` or an explicit
  provider subset controls the independent worker pool.
- `neomax-core` owns behavior. `neomax-cli`, `neomax-portal`,
  `neomax-usage-agent`, and `neomax-worktrees` are composition roots and must
  keep domain behavior in the core.
- Routing is task-aware. Plans may select different providers and locally
  supported models by task, repository stack or scheduler area, dependency
  state, project and session history, active subagents, and usage or quota
  evidence. Preserve those records so recovery, rotation, the portal, and
  later routing decisions can use them.
- Root interactive launches resolve a provider, account, model, and worker
  scope, build a typed provider-native command, inherit terminal input and
  output, and register live ownership in `OrchestratorStore`. They do not
  create a durable run merely to open the provider session. Guarded headless
  workers use the durable `RunStore`, worker coordinator, supervision,
  continuation, and failover services. Keep repository verification hermetic
  and never turn a unit test into an authenticated provider request.
- The current working directory is the default project. Do not add customer
  repositories, usernames, home-directory assumptions, private project seeds,
  account identities, or machine-specific paths to tracked files.
- Keep shared commands, state, environment variables, tools, and auxiliary
  binaries under the `neomax` namespace. `cmax` is reserved for the
  Claude-pinned launcher.

## Architecture and modularity

- Keep one responsibility owner for each domain and public compatibility
  symbol. Update `neomax_core::registry` whenever a public domain boundary is
  added or moved.
- Split production code and tests by stable responsibility and dependency
  boundary. Do not use an arbitrary line-count target, and do not rebuild a
  monolithic core or test suite.
- Keep binaries thin. Argument parsing, process wiring, rendering, and
  platform integration belong at the executable boundary; routing, state,
  persistence, provider behavior, and policy belong in their domain modules.
- Preserve typed boundaries between provider adapters and shared behavior.
  Provider command construction, environment isolation, event parsing, model
  resolution, authentication detection, and usage collection must remain
  injectable where they touch the process or filesystem.
- Kimi 0.38 root sessions are interactive and use the installed `--agent-file`.
  Because Kimi's `-p` and `--prompt` flags are headless, an explicit initial
  task is first sent through a bounded headless bootstrap; Neomax then resumes
  the returned session with `-S ... --auto` for the user-facing root session.
  The final interactive process must not receive a positional task or prompt
  flag, and a resume launch cannot combine a new task with an existing session.
- Preserve state compatibility. Unknown future fields must survive round trips;
  malformed optional records should degrade to an isolated warning or empty
  view where the contract permits; mutations must be atomic and locked.
- Keep comments rare. A comment is warranted only for a security boundary,
  compatibility rule, external protocol detail, or non-obvious invariant.

## Routing and model invariants

- Dynamic selection must consider every connected provider, not assume Claude
  exists, and must work with any non-empty connected subset.
- Automatic account selection uses a 92 percent proactive threshold for a
  reported five-hour window and excludes quota profiles at or above the 99
  percent wall. Weekly windows use the 99 percent wall. Exclude
  unauthenticated, paused, and cooled profiles, and prefer another account
  from the same provider before crossing providers when the worker scope
  permits.
- A quota event on a live task must preserve the task, run, branch, worktree,
  and session metadata so continuation can be handed to an eligible account.
  Do not silently restart work from scratch or dispatch known-exhausted
  profiles.
- Keep rotation provider-neutral at the command boundary. The provider owns
  whether its safe operation swaps credentials in place or starts a same-
  provider handoff. Continue with another same-provider account first; only a
  quota event or maintenance tick may fall back to another provider inside the
  worker scope. Do not create one global behavior that assumes every CLI has
  the same session model.
- Preserve the strict defaults: Claude `claude-fable-5[1m]`, Codex
  `gpt-5.6-sol`, OpenCode `opencode/big-pickle`, Kimi `kimi-code/k3`,
  and Grok `grok-4.6`.
- Every provider accepts explicit locally supported model IDs. OpenCode IDs
  must remain qualified as `provider/model`. Claude Opus is opt-in only.
  Never add silent model fallback. Record the effective model on every run,
  usage row, portal row, and scheduler part where that record exists.
- A scheduler part owns its engine, model, dependencies, and affected areas.
  Validate scope, reject dependency cycles, acquire all required area locks
  atomically, and release locks only for the owning run.

## Agent tools and concurrency

- Every orchestrator and worker receives the canonical agent-tool manifest and
  the `NEOMAX_BIN`, `NEOMAX_TOOL_MANIFEST`, `NEOMAX_TOOL_INSTRUCTION`,
  `NEOMAX_TOOL_DEPTH`, and `NEOMAX_TOOL_MAX_DEPTH` environment values.
- Keep tool command ownership in `neomax_core::agent_tools`. Any new command
  requires a canonical manifest entry, command class, policy decision,
  executable-resolution behavior, and hermetic coverage.
- Worker policy must not grant destructive or external authority by accident.
  Recursion limits, manifest completeness, absolute executable paths, and
  environment isolation are fail-closed checks.
- The shared concurrency setting is `~/.config/neomax/config.toml` under
  `[concurrency]`. `NEOMAX_MAX_SUBAGENTS` is the one-run override and must be
  exported to child agents. Queue reservations, task caps, account lanes,
  and live session caps must use the same effective settings.

## Privacy and security

- Never commit credentials, API keys, OAuth payloads, cookies, provider
  profile state, session databases, private logs, local project definitions,
  user names, machine paths, or proprietary source.
- Keep developer-specific privacy patterns only in the ignored
  `.neomax-privacy.local` file. Never commit that file or copy its values into
  product code, tests, scripts, or documentation.
- Never copy credentials between accounts outside the explicit, permission-
  preserving rotation transaction. Keep backups private and preserve file
  permissions.
- Tests must use temporary directories, sanitized fixtures, injected command
  runners, and fake provider executables. They must not read a real login or
  make an authenticated model request.
- The portal binds to loopback by default. Read endpoints remain separate from
  guarded same-origin local actions, and destructive actions require explicit
  confirmation. Escape all state-derived strings before rendering HTML or
  JavaScript.
- Do not add an authenticated probe or release artifact whose sole purpose is
  to exercise a provider. Live validation is a separate operator-authorized
  activity.

## Documentation and contribution workflow

- Use ASCII punctuation in Markdown. Em dashes and en dashes are not allowed.
- Use `Neomax` in prose and `neomax` or `NEOMAX_*` in commands and state.
- Keep `WORKLOG.md` entry-free on `main`. A pull request branch must replace the
  template with a concise product-safe record of behavior, affected files,
  exact verification, and remaining risk. Maintainers clear accepted entries
  when preparing the next `main` template.
- Every agent or human contribution that is submitted as a pull request must
  carry that branch work-log record. Do not omit it because the change is
  documentation-only, test-only, or generated from a provider workflow.
- Read `CONTRIBUTING.md` before opening an issue or pull request. Issues need a
  reproducible report with secrets and private paths removed. Pull requests
  need focused scope, hermetic tests, and proof of the relevant gates.

## Verification

Run the complete local gate before review:

```bash
bash scripts/check-doc-style.sh
bash scripts/check-product-surface.sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
```

For distribution changes, also build the target package and run
`dist/check-package.sh` and `dist/verify-install.sh`. The distribution verifier
must use a temporary home and fake provider executables and must prove that no
provider process was invoked. For compatibility changes, run the differential
fixture suite in addition to focused tests. Do not replace the full gate with a
single package check.
