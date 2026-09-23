# Work log

## 2026-09-23 - Claude Opus 5.5 is the default Claude model

The built-in Claude default is now `claude-opus-5-5[1m]` (Claude Opus 5.5)
instead of `claude-fable-5-1[1m]`. It is the only default Claude model; Neomax
has no advisor model. `--opus` and a scheduler part's `opus` flag now select
Opus 5.5. A scheduler part with `opus: true` and no explicit model now
dispatches Opus 5.5 instead of silently using the configured default. Fable 5,
Fable 5.1, Opus 5, and every other locally supported Claude ID still run when
passed explicitly. Combining `--opus` or `opus: true` with a different explicit
Claude model is still rejected. Because `--opus` now means Opus 5.5, pairing
it with `claude-opus-5` is now a conflict; pass `claude-opus-5` on its own.

Opus 5.5 maps to the `opus` model family, so account selection, live rotation,
and failover use the Opus weekly window (`seven_day_opus`), not the Fable
window. Usage pricing adds Opus 5.5 at $4 input, $20 output, $0.20 cache
reads, $5 five-minute cache writes, and $8 one-hour cache writes per million
tokens; before this change Opus 5.5 rows were priced as Opus 5. Newly ingested
Claude transcript rows with no model id are recorded under the default, now
Opus 5.5. The portal now
always shows an Opus weekly row for Claude accounts, ahead of the Fable row.

Affected areas: `neomax-core` provider catalog, scheduler dispatch, usage
pricing; `neomax-cli` launch model selection and help; the portal account
view; README, AGENTS.md, `docs/REFERENCE.md`, `docs/USAGE-AGENT.md`, and the
Neomax skill.

Verification:

- `bash scripts/check-doc-style.sh`, `bash scripts/check-product-surface.sh`,
  `cargo fmt --check`, `node --check` on the portal script, and
  `git diff --check`: passed.
- `cargo test -p neomax-core`: 901 unit, 2 account-boundary, and 36
  compatibility tests passed, zero failed.
- `cargo test -p neomax-cli --test e2e_launch --test
  e2e_reference_model_controls`: 13 and 8 passed, zero failed. These use fake
  provider executables; no authenticated provider was called.
- `cargo test -p neomax-tui provider_change`: 1 passed.
  `cargo test -p neomax-portal`: 115 passed.
- Not run: `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace`. Local Rust checks used four build jobs, disabled
  incremental compilation, and omitted debug symbols to limit disk use.

Remaining risk: hosts with a `models.toml` Claude override keep that override.
The unknown-model price fallback is unchanged at Fable 5 rates. Cache-write
prices are derived with the existing 1.25x and 2x multipliers.

## 2026-09-19 - Blacksmith CI and paused Intel Mac releases

CI, packaging and Windows rotation use Blacksmith. Intel Mac matrix entries
remain defined as comments for explicit reactivation. The release manifest now
requires six active target archives and all twelve final assets. Existing
published releases are untouched; exact-SHA CI, checksums and publication
verification remain required.

Affected areas: `.github/workflows`, distribution assembly/publication and
verification scripts under `dist`, README and distribution documentation.

Verification:

- `bash scripts/check-doc-style.sh` and `bash scripts/check-product-surface.sh`:
  passed.
- `cargo fmt --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: 1,648 passed, zero failed, one existing ignored
  manual performance measurement. No authenticated provider was called.
- `bash dist/test-release-assets.sh` and
  `bash dist/test-release-workflow.sh`: passed. These use fixture binaries and a
  fake GitHub CLI, including missing, extra and tampered artifact rejection.
- `cargo build --workspace --bins`, followed by `dist/package.sh` with
  `--target aarch64-apple-darwin --version 0.1.4 --binaries-dir target/debug`:
  passed. `dist/check-package.sh` and `dist/verify-install.sh` passed on that
  archive using a temporary home and fake providers. This was a local debug
  package, not a published release artifact.
- Actionlint and ShellCheck baseline comparisons: no new finding. Bash syntax,
  the shared runner audit and `git diff --check` passed. UBS has no shell scanner.

Local Rust checks used two build jobs, disabled incremental compilation and
omitted debug symbols to limit disk use. Linking reported a local rust-objcopy
LLVM-library warning; builds and tests completed successfully. Final Blacksmith
platform CI and future tagged release execution remain to be verified. Runner
installation access is an operating requirement, not established by YAML alone.
