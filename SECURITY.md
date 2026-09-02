# Security policy

Please do not report suspected vulnerabilities in a public issue.

Use GitHub's private vulnerability reporting channel:

https://github.com/NeotaskInc/neomax-orchestrator-rust/security/advisories/new

If that channel is unavailable, request a private reporting path without
including vulnerability details.

Neomax for Rust includes the Cargo workspace, provider command construction,
credential isolation, process environment scrubbing, local portal access,
usage collection, worktree cleanup, state files, and distribution scripts.
Security reports should include:

- the affected crate, command, version, or commit
- the Rust toolchain, target, and operating system
- the impact and a minimal sanitized reproduction
- any suggested mitigation

Remove credentials, tokens, OAuth data, account or profile details, personal
paths, private repository names, proprietary source, and unredacted logs.
Maintainers will coordinate validation, remediation, and disclosure through
the private channel.
