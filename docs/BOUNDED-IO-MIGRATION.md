# Bounded local I/O migration

`neomax_core::io` is the shared boundary for local process and file access
that can encounter untrusted size, latency, or partial state. It is usable
with injected `Clock`, `FileSource`, and `ProcessRunner` seams, so domain
tests do not need a real provider process or a real account directory.

## API contract

- `ProcessRequest` requires a finite timeout and positive stdout and stderr
  limits. `LocalProcessRunner::capture` always terminates and reaps its child
  after a timeout or output limit. `ProcessRunner::execute` fails closed on
  timeout, truncation, or nonzero exit.
- `read_file` and `hash_file` reject non-regular files, missing files,
  oversized metadata, short reads, and changed-file length mismatches.
- `read_file_range` validates the complete range before seeking and requires
  the requested bytes to be present.
- `read_reader` and `hash_reader` handle partial reads and classify timeout,
  truncation, I/O failure, and corruption separately.
- `BoundedIoError` is the only error classification domain code should
  translate into a provider, state, or installation result.

## Migration inventory

| Area | Current risk | Migration |
| --- | --- | --- |
| `git::command` | Unbounded `git` stdout and stderr | Build `ProcessRequest` with git-specific limits and use `execute`. |
| `providers::catalog` | Discovery is bounded locally but owns a duplicate runner | Adapt `CommandRunner` to `ProcessRunner`; retain catalog output flags only at the adapter. |
| `providers::auth` | Credential and keychain reads can be unbounded | Use bounded file reads for JSON and config; classify malformed JSON as corruption. |
| `runs::liveness` and execution logs | Polling and log reads can outlive a run or grow without a cap | Use a bounded process/log reader and preserve timeout state in the run record. |
| `scheduler::locks::owner` | Owner metadata is local state but can be malformed or unexpectedly large | Use bounded JSON reads and map corruption to a stale-owner decision only where the scheduler contract permits. |
| `installation::files`, package, and manifest | Installation inputs must remain bounded even when an archive is damaged or unexpectedly large | Uses streaming `hash_file` and bounded manifest reads before deserialization. |
| `neomax-portal` file sources | State and Git reads are request-facing | Use bounded reads and bounded Git process requests; return an isolated warning on optional corruption. |
| `neomax-usage-agent` collectors | Quota logs, credential files, and incremental tails vary in size | Use bounded range reads for tails and bounded credential/config reads; preserve incremental offsets. |
| `neomax-usage-agent` install and maintenance runners | External helper commands can hang or emit unbounded output | Use `ProcessRequest` with explicit per-command limits and finite maintenance timeouts. |
| `neomax-worktrees` Git helper | Git commands use direct `Command::output` | Use the shared process runner and fail closed before mutating worktrees. |

Small fixed-format values such as one-line branch names may retain direct
reads only when a caller enforces a byte limit before decoding. Anything that
comes from a provider cache, repository, worktree, process, portal request,
or install package should use this boundary.
