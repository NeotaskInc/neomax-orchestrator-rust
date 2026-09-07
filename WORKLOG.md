# Work log

## Terminal workspace and account completion

Restores the six-page terminal workspace and provider/account/email entry.
Codex uses standard service unless fast mode is requested. Native transcript
activity and complete session discovery feed the shared portal projections.
The canonical agent-tool manifest uses a content-addressed filename.

Adds read-only doctor diagnostics, local credential health, quota freshness,
opt-in fractional reset ranking, and account-targeted OAuth rotation. Quota
and cooldown records follow swapped credentials through the shared
continuation service. Model-free ticks preserve unsupported handoffs and
cannot claim that an unstarted replacement is running. Credential swaps are
locked, preserve private backups, and roll back on journal or metadata errors.
New Codex configuration preserves explicit user settings. The installer can
reconcile byte-identical workflows and preserve primary Claude settings links.

Affected domains: CLI composition, account selection, provider catalog,
orchestration, session projections, installation, portal, and the new TUI crate.
README, command reference, help, and the Neomax skill describe the commands and
their limits.

Verification passed:

- `cargo test --workspace -j2 --quiet`, including 882 core unit tests,
  36 compatibility fixtures, 113 portal tests, and 17 TUI unit tests.
- The CLI real-PTY fixture exercised launch cancellation, confirmation,
  native input/output, page changes, resizing, and confirmed exit.
- `cargo clippy --workspace --all-targets -j2 -- -D warnings`.
- `cargo fmt --all --check`, documentation, product-surface, privacy,
  shell-shortcut, and skill-validation checks.
- `rustup run 1.85.0 cargo check --workspace --locked -j2 --target-dir target/msrv`.
- Native release build and package/install verification in an isolated home
  with fake provider executables; no provider invocation occurred.

UBS inspected changed and new Rust source. Its critical groups were test
panics, terminal-key comparisons, timestamps misclassified as secret
generation, local JWT metadata decoding, and the intentionally detached PTY
reader. These were reviewed against the code; this is not a zero-finding scan.
The reader exits when the owned child closes or its receiver is dropped.

No authenticated model or live credential-rotation test was performed. An
in-place credential swap does not prove that a native provider hot-reloaded
authentication or resumed work. The optional before/after startup benchmark
remains ignored; no new performance improvement is claimed.
