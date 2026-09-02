# Neomax Rust architecture

Neomax is a Rust workspace with one reusable domain library and four executable packages.

- `neomax-core` owns state, providers, routing, runs, usage, sessions, projects, scheduling,
  rotation, issues, and git behavior.
- `neomax-cli` builds the multicall `neomax` executable. Installer-created `neomax-cli`,
  `cmax`, `cdx`, `cdxmax`, `ocx`, `ocmax`, `kmx`, `kmax`, `gmx`, and `gmax` links resolve to
  that binary.
- `neomax-portal` serves the localhost dashboard from the same core status model.
- `neomax-usage-agent` installs and runs the background collector without model-prompt execution.
- `neomax-worktrees` creates coordinated project worktrees.

`neomax_core::registry` is the authoritative domain inventory. Every domain has one responsibility
and typed public boundary. Provider adapters implement the same interface and return provider-neutral
events, usage, sessions, authentication state, and commands.

Account policy is deliberately independent of runtime persistence. `accounts::ports` defines the
quota and live-work observations it needs; the usage cache implements the quota port and runs expose
the live-work adapter. Account code owns snapshots, quota advice, eligibility, and selection without
importing run stores, process probes, supervisor directives, or concrete usage stores.

Compatibility is observable behavior: command lines, exit codes, JSON shapes, state paths and
schemas, atomicity, process survival, provider environment isolation, cooldown decisions, resumable
session identity, worktree safety, and installed workflow locations. Golden fixtures are generated
from sanitized reference scenarios and then owned by this repository; tests do not depend on another
checkout or authenticated account.
