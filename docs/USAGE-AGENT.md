# Neomax usage agent

This document records the native `neomax-usage-agent` contract. Ledger
ingestion, normalization, deduplication, and persistence are local. The agent
never sends a model prompt and never transmits ledger records, transcripts,
prompts, or model output. Its only Neomax-owned HTTP quota path is Claude and
that path can refresh Claude OAuth. Collection startup may invoke bounded
provider CLI discovery commands; any network behavior inside those commands is
owned by the provider CLI and is outside the agent's direct HTTP boundary.
Service install, uninstall, and status use local files and the platform service
manager without starting provider commands.

## Triggers and order

`neomax-usage-agent once` runs one cycle and exits. `run` performs one cycle,
then repeats at `NEOMAX_USAGE_POLL` seconds (default 3). `ensure` checks the
platform service state and installs or starts the service only when it is not
already active (or loaded under launchd). The check is idempotent and does not
run provider discovery. An installed user service starts `run` at login or user-service
startup and keeps it alive:

- macOS: launchd label `io.neomax.usagewatch`.
- Linux: the per-user `neomax-usage-agent.service` unit.
- Windows: the per-user `Neomax\UsageAgent` task.

Each cycle does the following:

1. Acquire the usage-state lock.
2. Load watch state and optionally rebuild or establish the initial baseline.
3. Scan local provider records and append new ledger records.
4. Save watch state atomically and release the lock.
5. Refresh numeric quota snapshots.
6. Run due local maintenance actions, each with its own timeout. This includes
   a safe worktree tidy sweep when `NEOMAX_WORKTREE_TIDY_EVERY` is nonzero.

## Product install and service activation

On macOS, Linux, and Windows, a successful native `neomax install` runs the
newly installed `neomax-usage-agent install` after the file transaction. It
passes the newly installed `neomax` and usage-agent paths through
`NEOMAX_CLI_BIN` and `NEOMAX_USAGE_AGENT_BIN`. Each activation or stop action
has a 15 second timeout. A failure of that action is reported as a warning,
does not roll back the file installation, and can be retried with
`neomax-usage-agent install`. Other platforms install the files but skip this
automatic service action.

The loopback portal starts `neomax-usage-agent ensure` as a detached,
provider-neutral startup repair. The portal does not wait for service-manager
work to finish and remains available for read-only requests if the child
cannot be started. Set `NEOMAX_PORTAL_NO_USAGE_AGENT` to skip this startup
repair when an operator intentionally manages the collector separately.

Only `once` and `run` perform provider discovery and quota collection.
`ensure`, `install`, `uninstall`, and `status` do not execute provider
discovery commands or make a Claude quota HTTP request.

Pass `neomax install --no-usage-agent`, or set `NEOMAX_NO_USAGE_AGENT` to any
value (including an empty value), to skip the automatic service action. This
opt-out controls only the native product install wrapper; it does not remove
the usage-agent binary, change collection, or make direct
`neomax-usage-agent install` calls offline. On an upgrade it also does not
stop a service that was already running; use `neomax uninstall` or direct
`neomax-usage-agent uninstall` to disable that service.

Native `neomax uninstall` invokes the installed usage agent's `uninstall`
before removing product files, when the binary exists on a supported platform.
If stopping the service fails, uninstall warns and continues removing the
product files. A retained agent binary, or a reinstall with
`--no-usage-agent` followed by the direct agent command, is needed to retry
the stop when the service manager can be used.

The initial cycle backfills recent history by default. `--no-backfill` records
current file/database positions without importing existing history. `--rebuild`
clears date-partitioned ledger JSONL files and scans from the beginning unless
combined with `--no-backfill`. Neither option disables quota refresh.

The collector marks a cycle as rate-limited when a parsed record contains a
rate-limit signal. A rate-limited cycle calls quota refresh with `force=true`.
Other cycles use `force=false`. Due maintenance runs `neomax rotate-tick
--active` every `NEOMAX_ROTATE_TICK` seconds (default 30) and
`neomax keepalive --once` every `NEOMAX_KEEPALIVE_EVERY` seconds (default 480).
It also runs `neomax tidy --automatic --any --json` every
`NEOMAX_WORKTREE_TIDY_EVERY` seconds (default 600). Set
`NEOMAX_WORKTREE_TIDY_EVERY=0` to disable only the periodic tidy action. These
commands are local control-plane actions; they do not send model prompts. A
tidy sweep uses `NEOMAX_WORKTREE_TIDY_TIMEOUT_SECS` (default 300 seconds),
separate from the 30-second timeout used by the other maintenance actions.
Set either value before service installation, or reinstall the usage-agent
service after changing it.

The tidy action is deliberately conservative. It may remove recognized
regenerable artifacts from a managed run worktree only after Git confirms that
the path is ignored and contains no tracked files. The recognized directory
names are `node_modules`, `target`, `.next`, `.nuxt`, `.svelte-kit`, `.turbo`,
`.parcel-cache`, `.vite`, `coverage`, `.pytest_cache`, `.mypy_cache`,
`.ruff_cache`, `__pycache__`, `.gradle`, `dist`, `build`, and `out`. It never
follows symlinks, uses a broad `git clean`, or removes source, commits, dirty
state, unmerged state, or unverifiable paths. Residual ignored content outside
the recognized artifact set blocks whole-worktree removal. A whole retained
worktree is removed only after its branch is verified merged and clean. An
unchanged terminal worktree may also be removed immediately by the run lifecycle
finalizer; `--pr`, killed, changed, and unverified worktrees remain available
for review or recovery. Automatic tidy excludes killed and resumable run
states even when their branches appear merged.

Before the command runs, configuration discovery checks every provider binary
with `--version`. It also asks OpenCode for `models`, Kimi for `provider list
--json`, and Grok for `models`. These subprocesses have a 5 second timeout,
128 KiB stdout/stderr caps, and a cleared environment containing only safe
terminal variables, `HOME`, and the provider config variable. They are not
model prompts. A provider CLI may choose to contact its own service while
handling one of these discovery commands; the usage agent does not define or
inspect that provider-owned endpoint or payload.

## Local sources and transmitted usage fields

The profile catalog supplies the profile paths. Environment profile overrides
are honored; otherwise the default profile directories are `.claude`,
`.codex`, `.opencode`, `.kimi-code`, and `.grok` below the selected home.
The collector reads only usage-bearing local records:

| Provider | Local source | Parsed usage fields |
| --- | --- | --- |
| Claude | `projects/**/*.jsonl` | input, output, cache creation, cache read, model, session, timestamp, rate-limit signal |
| Codex | `sessions/**/*.jsonl` | cumulative input, cached input, output, model, session, timestamp, rate-limit signal |
| Kimi | `sessions/**/wire.jsonl` | input-other, output, cache creation, cache read, model, session, agent, timestamp, rate-limit signal |
| Grok | `updates.jsonl` paired with a local summary | input, output, reasoning, cache read, cache creation, model, session, cost, requests, completions, errors |
| OpenCode | the local SQLite database | input, output, reasoning, cache read, cache creation, model, session, agent, cost, requests, completions, errors, rate limits |

Records are normalized into `LedgerRecord` values and date-partitioned JSONL
files. The usage agent never sends transcript lines, database rows, ledger
records, prompts, model output, account names, or local paths over HTTP.

## HTTP behavior

The application-controlled Claude request fields are:

| Operation | Endpoint | Headers/body | Trigger |
| --- | --- | --- | --- |
| Usage read | `GET https://api.anthropic.com/api/oauth/usage` | `Authorization: Bearer <account access token>`; `anthropic-beta: oauth-2025-04-20`; `anthropic-version: 2023-06-01` | Claude cache is not fresh, or a rate-limited cycle forces refresh |
| OAuth exchange | `POST https://platform.claude.com/v1/oauth/token` | Headers `Content-Type: application/json`, `Accept: application/json`, `User-Agent: claude-cli/2.1.56 (external, cli)`; JSON body `grant_type`, `refresh_token`, `client_id` | The selected Claude access token is absent or expired and a refresh token is available |

The OAuth client ID is the fixed Claude CLI client ID in the source. The
refresh-token request transmits only the three JSON body fields listed above.
The usage request transmits only the account access token in its application
header. The HTTP library may add ordinary transport headers such as `Host`.
Both requests have an 8 second timeout. Successful responses must be JSON and
are capped at 2 MiB.

Codex quota is read from the newest local rollout JSONL file. OpenCode, Kimi,
and Grok have no numeric quota endpoint in this agent, so they never cause an
HTTP quota request. Their local usage and reactive rate-limit evidence remain
available to the ledger and routing policy. Provider discovery commands can
still perform provider-owned network work; Neomax does not treat that output
as a model request and does not send the resulting ledger data over HTTP.

## Refresh and failure handling

Claude quota cache entries are fresh for 60 seconds when their source is
`claude-api` and the cached account identity matches the profile's local
identity UUID. Codex rollout cache entries are fresh for 12 seconds. A fresh
entry avoids its provider read unless the cycle is forced. A live Claude
response stores `five_hour.utilization`, `five_hour.resets_at`,
`seven_day.utilization`, and `seven_day.resets_at`, plus `source`,
`observed_at`, and the local account UUID. Reset times accept numeric epochs
or RFC3339 values.

When a Claude token is expired, the agent first tries the local refresh token.
If the exchange returns `access_token`, it also accepts replacement
`refresh_token` and `expires_in`, updates the profile's `.credentials.json`,
and then requests usage with the new access token. Credential persistence is
atomic and limited to that profile file; a keychain-only credential is read
but is not rewritten by this agent.

There is no separate refresh-attempt cooldown or live-session skip in the
native quota refresher. Cache freshness is the normal-cycle gate; a forced
rate-limit refresh intentionally bypasses it and can retry each numeric
profile in that cycle.

An HTTP, token, parse, or empty-window failure does not discard a usable cache:
an observation up to 24 hours old is returned for that cycle with
`stale=true`. The failed observation is not written over the last successful
cache. If no such cache exists, the profile reports no refreshed snapshot. An
expired cache is marked `expired=true`; a known refresh token marks it
`recoverable=true`. The agent does not expose tokens or raw provider errors in
its serialized quota report.

## Local persistence and locking

With no override, files are stored below `$HOME/.neomax`. `NEOMAX_HOME`
replaces that state root:

- `usage-watch.state.json` stores source byte offsets, Codex cumulative totals
  and model identities, OpenCode row fingerprints, the baseline marker, and
  maintenance attempt/result summaries, including the worktree tidy action.
  Unknown JSON fields survive a round trip.
- `usage-ledger/YYYY-MM-DD.jsonl` stores normalized records partitioned by the
  record timestamp. Appends use a per-date lock. Reads deduplicate by record
  ID and retain the largest cumulative value where applicable.
- `usage/<engine>-<profile identity>.json` stores quota snapshots. The profile
  identity is a hash of its normalized path, so same-named accounts do not
  collide. An older basename cache is read once and migrated to the hashed
  path.

The whole collection/state update uses an exclusive lock beside the watch
state. State and credential JSON writes are atomic. Missing state starts empty;
malformed state fails closed without overwriting the original file.

## Disable and offline behavior

`neomax-usage-agent uninstall` stops and removes the installed service artifact
but leaves usage state, ledger files, quota caches, provider profiles, and
credentials intact. On macOS uninstall also attempts the prior launchd label
as a migration cleanup; that identifier is intentionally absent from install
and status behavior. Reinstall creates only the `io.neomax.usagewatch` service.

There is no separate offline flag. Running `once` or `run` with no Claude
access/refresh token and no usable cache skips Claude HTTP and continues local
collection. Codex and the three reactive providers remain local-only. If a
Claude token or refresh token is present, a normal non-fresh cycle may attempt
the Claude endpoint even when `--no-backfill` is set. Network failure is
best-effort: local collection and maintenance still complete, and a recent
cache is retained as stale. The product-install opt-out does not disable an
already-running service; uninstalling the service is the supported way to
disable periodic cycles. An explicit `once` still performs its normal quota
behavior.

## Reference compatibility

The native behavior preserves the reference watcher contract for incremental
offsets, full backfill, no-backfill baselining, rate-limit-triggered refresh,
date-partitioned deduplicated ledger data, and service uninstall migration.
On macOS, the old launchd label is deliberately narrower here: only uninstall
boots it out and removes its plist; install and status use only the current
label.
It intentionally differs from the reference where the native implementation
has no provider-specific network endpoint: Codex quota is local rollout data,
and reactive providers do not receive invented numeric quota values. The
native Claude path also makes one HTTP attempt per refresh, has no rate-limit
backoff or refresh-attempt marker, does not skip recently active profiles, and
persists refreshed credentials only to the profile file. A stale fallback is
marked on the in-memory cycle report rather than replacing the last successful
cache on disk.
