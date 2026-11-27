# Work log

## 2026-11-27 - Antigravity (agy) engine and amax launcher

Neomax gains a sixth engine, Antigravity, driven through the Antigravity CLI
`agy`. It works as a root orchestrator (`amax`, `neomax antigravity`) and as a
guarded headless worker. The default model is `gemini-4-neon` at effort
`high`; `-e` accepts agy's `low|medium|high|xhigh|max` and `-u` selects
`xhigh`. Workers run `agy --print ... --output-format stream-json` with
`--dangerously-skip-permissions`, or `--mode plan` for read-only scouts, and
resume with `--conversation`. Root sessions pass the orchestrator instruction
and task through `--prompt-interactive`, because agy has no system-prompt flag.
Every agy process gets `AGY_CLI_DISABLE_AUTO_UPDATE=true`.

agy's `init`, `step_update` and `result` events are parsed for the
conversation id, response, token usage (including thinking and cached tokens),
native subagents and error or quota status. Sessions and native subagents are
read from agy's own `antigravity-cli/conversation_summaries.db`, opened
read-only. agy keeps its Google sign-in in the operating-system keyring, which
Neomax never reads: a profile counts as connected once agy has initialised its
state there, or when its settings select Gemini API-key mode, in which case
`GEMINI_API_KEY` is passed only to the agy process. `amx` is the account helper
(`login` starts agy's own sign-in; `run`, `models`). Antigravity is appended
to every provider priority and failover order, so existing routing is
unchanged.

An Antigravity orchestrator delegates in a fixed order: its own subagents and
threads first, then Claude Code workers on Opus at effort high. `amax`
defaults its worker pool to `antigravity,claude` (`--workers` still overrides
it), its initial prompt states that order, and a Claude worker dispatched from
inside it gets effort `high` unless the dispatch names one. A Fable Claude
model is refused both in the `amax` plan and in any dispatch from inside an
Antigravity orchestrator. Provider discovery (`agy --version`, `agy models`)
also runs with auto-update disabled. The neomax skill, README, reference,
installation and architecture docs list Antigravity, `amax` and `amx`.

The Codex default becomes `gpt-6.1-sol`, and the Codex sub-agent model list no
longer offers GPT-5.6 models. Explicit GPT-5.6 IDs and aliases still resolve.

Affected areas: core engine enum, provider spec, worker and orchestrator
command builders, event parser, session discovery, auth detection, process
secrets, failover, scheduler effort validation, model settings, CLI launchers,
help, account helpers, portal, usage agent, installer alias lists, shell
shortcuts and the e2e fake-provider harness.

Verification: formatting, Clippy with `-D warnings`, documentation style,
product surface, privacy surface, shell syntax, shell shortcut and diff
whitespace checks passed. All workspace test targets passed, including the
provider-secret isolation e2e tests for `amax` (only `GEMINI_API_KEY` reaches
agy in API-key mode; OAuth profiles receive no ambient keys) and the hermetic
launch tests through a fake `agy`. `amax --dry-run --json` plans the Antigravity
orchestrator with `gemini-4-neon` and workers `claude,antigravity`, and refuses
`--claude-model claude-fable-5`. A real `amax` launch against a recording fake
`agy` received `--model gemini-4-neon --effort high
--dangerously-skip-permissions --prompt-interactive` with the delegation order,
`NEOMAX_FLEET=claude,antigravity` and `NEOMAX_CLAUDE_MODEL=claude-opus-5-5[1m]`.
From inside that session a Fable dispatch was refused and a plain Claude
dispatch ran `claude ... --model claude-opus-5-5[1m] --effort high`.

A new stable Clippy lint (`needless_bool`) in the installer's cross-device
check is fixed by expressing the per-platform error code as a constant.

Remaining limits: the usage price table has no rates for `gpt-6.1-sol` or
Gemini 4 Neon, so their cost estimates read zero. Antigravity has a single
native profile per host; account pooling is left to the provider or a proxy.
No authenticated provider request was made by these tests.
