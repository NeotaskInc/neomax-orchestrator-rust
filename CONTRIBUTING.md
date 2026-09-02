# Contributing to Neomax for Rust

Issues, documentation updates, hermetic tests, provider compatibility work,
and focused implementation changes are welcome. Read `README.md` and
`AGENTS.md` first. The repository is designed for contributions from people
and development agents using any supported coding-agent harness.

## Issues

Search existing issues before opening a new one. Include the smallest useful
reproduction and, when relevant, the Neomax version, operating system, Rust
toolchain, upstream CLI version, provider, command, expected result, and actual
result. Remove API keys, OAuth data, cookies, account identities, profile
contents, private repository names, local paths, proprietary source, and
unredacted logs. Use the security contact or private process for a
vulnerability instead of publishing exploit details in an issue.

## Pull requests

1. Fork the repository and create a focused branch from `main`.
2. Read `AGENTS.md` and `ARCHITECTURE.md`. Preserve the provider-neutral
  boundaries, model defaults, 92 percent proactive threshold, 99 percent quota
  wall, durable-run behavior, worktree safety, tool manifest, and privacy rules.
3. Keep each domain and test suite responsibility-focused. Add the smallest
   hermetic regression fixture that proves the changed behavior.
4. Use injected command runners, temporary state directories, fake provider
   executables, and sanitized data. Do not use an authenticated account or
   make a live model request during repository verification.
5. Update `README.md` and the provider-neutral development docs when a public
   command, state path, model rule, provider capability, or workflow changes.
6. Replace the blank `WORKLOG.md` template on your branch with a concise,
   product-safe record. This is required for every pull request, including
   documentation-only, test-only, and agent-authored changes. Include
   user-visible behavior, affected files or domains, exact verification, and
   remaining risk. Do not include secrets, account details, private project
   names, or machine-specific paths.
7. Run the complete verification gate from `AGENTS.md`, then explain the
   result in the pull request description.

Maintainers clear accepted worklog entries when preparing the next `main`
template. This keeps every incoming change reviewable without turning the
repository work log into a history of private operations.

## Provider changes

A provider integration is a cross-surface contract. Review all of these when
changing one provider:

- provider specification, binary discovery, profile isolation, and
  authentication-state detection
- OAuth, API-key, device, or local credential capability metadata where the
  provider supports it
- orchestrator and headless worker command construction, environment
  scrubbing, model resolution, and local model discovery
- transcript, event, session, native-subagent, token, cost, and rate-limit
  parsing
- account eligibility, quota pressure, cooldowns, reservations, 92 percent
  proactive selection, 99 percent admission, same-provider rotation, and
  cross-provider failover scope
- durable run state, handoff identity, worktree preservation, and cleanup
- `neomax-portal` status, history, usage, sessions, subagents, logs, and diff
  payloads
- usage-agent collectors, incremental offsets, deduplication, and service
  installation behavior
- provider-native command assets, distribution aliases, README examples, and
  hermetic tests

Do not claim live provider support from a dry run alone. If authenticated proof
is needed, describe the exact boundary and obtain operator authorization
before making the request. Keep that proof separate from ordinary repository
tests.

## Review expectations

Reviewers may ask for a smaller scope, a clearer module owner, privacy cleanup,
additional fixtures, or proof against a fake upstream CLI. Preserve unrelated
work in a dirty checkout, keep commits reviewable, and do not weaken a safety
gate to make a build pass. An issue is useful for larger changes but is not
required for a clear, self-contained fix.
