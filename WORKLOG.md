# Work log

## 2026-09-25 - CI Gate, pinned actions and main-push concurrency

Rust CI gains a `CI Gate` job that depends on every CI job, fails on any
failed or cancelled job and fails on draft pull requests with a clear message.
Draft pull requests still run `quality` and `msrv`; the macOS, Windows and
packaging jobs skip until the pull request is ready for review, and
`ready_for_review` starts the full run on the same head. Packaging jobs no
longer wait for `quality` and `msrv`, so all jobs start together.

Concurrency now cancels superseded runs only for pull requests. Pushes to
`main` are never cancelled, so every landed commit keeps a complete CI run for
the exact-SHA release gate. Every action in the three workflows is pinned to a
full commit SHA, every job has a timeout, and CI checkouts no longer persist
the token. The release CI verifier and its fixture test now expect the
`CI Gate` job in the exact-SHA run. Release triggers, job graph and packaging
steps are unchanged.

The `cross-package` Rust cache is now keyed per target in CI and release. All
three legs shared one cache key, so the first warm run after 2026-09-20
restored build scripts compiled in the Ubuntu 20.04 `x86_64-unknown-linux-musl`
cross image into the Ubuntu 16.04 and 18.04 aarch64 images, which failed with
`GLIBC_2.28` not found. Cold caches hid this on the only earlier Blacksmith
runs.

Affected files: `.github/workflows/ci.yml`,
`.github/workflows/release-assemble.yml`,
`.github/workflows/windows-rotation.yml`, `dist/verify-ci-run.sh`,
`dist/test-release-workflow.sh`.

Verification:

- `bash dist/test-release-workflow.sh`: passed, including the fake-CLI
  verifier cases for incomplete matrices, wrong SHA and failed runs.
- `bash scripts/check-doc-style.sh`, `bash scripts/check-product-surface.sh`
  and `bash scripts/check-shell-syntax.sh`: passed.
- `actionlint` on all three workflows: no findings apart from the two existing
  ShellCheck notes, which are unchanged.
- The test and package commands are byte-identical, so the executed test set
  is unchanged; pull request CI provides the run evidence.

Remaining risk: a release of a commit whose `main` CI run predates this change
would fail the exact-SHA gate because that run has no `CI Gate` job. Release
from a commit that landed after this change.
