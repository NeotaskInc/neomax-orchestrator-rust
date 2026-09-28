# Work log

## 2026-09-27 - GPT-6 and Opus 5.5 model catalog

Codex aliases `sol` and `luna` now select GPT-6 Sol and GPT-6 Luna.
GPT-6 Astra remains the Codex default. Claude now defaults to
`claude-opus-5-5[1m]` across root launches, workers and scheduler parts.
Explicit model overrides, full GPT-5.6 IDs and the existing `--opus`
selection remain available.

The usage catalog includes the new models at official standard input,
output, cache-read and cache-write rates. Opus 5.5 uses its reduced $0.20
cache-read price and $8 one-hour write price per million tokens.
Source links and the rate table are in `docs/USAGE-AGENT.md`.

Affected areas: core model resolution, worker model catalog, usage pricing,
launcher registration and settings precedence tests, and model documentation.

Verification: formatting, Clippy, documentation style, product surface,
privacy surface, shell syntax and diff whitespace checks passed. The pricing
regression passed. All workspace test targets passed, including 897 core tests
and 36 compatibility fixtures; the corrected core target passed on rerun after
updating the old Luna alias expectation. Independent review passed model, pricing,
precedence and CLI dry-run checks with no blocking findings.
UBS findings were reviewed: flagged panics/assertions are test failure paths;
other warnings concern test unwraps and existing conversions/allocations.

The Claude default follow-up updates catalog, configuration, scheduler and
launcher expectations, plus the shipped skill and development guidance.
The failover regression checks that the new default can use an account whose
Fable quota is exhausted, while an explicit Fable selection remains blocked.

Remaining limits: rates are standard API-equivalent estimates. Session totals
cannot establish request-level context or service-tier premiums. No authenticated
provider request or release was performed.
