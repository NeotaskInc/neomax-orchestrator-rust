# Work log

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
