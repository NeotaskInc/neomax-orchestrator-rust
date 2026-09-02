## Summary

Describe the user-visible problem and the focused solution.

## Affected areas

List the Rust crates, commands, provider surfaces, documentation, or
distribution files affected.

## Verification

List the exact checks you ran and their results. Keep provider validation
hermetic and do not make authenticated model requests as part of repository
verification.

## Work log

On a contribution branch, replace the blank `WORKLOG.md` template with one
concise, product-safe record of the user-visible behavior, affected files or
domains, exact verification, and remaining risk. Keep the blank template on
`main`. Do not use the work log for credentials, account details, provider
transcripts, private project names, machine-specific paths, proprietary
source, or unredacted logs.

## Checklist

- [ ] The change is focused and preserves unrelated behavior.
- [ ] I read and followed `AGENTS.md` and `CONTRIBUTING.md`.
- [ ] I added or updated hermetic tests for behavior changes.
- [ ] I ran the relevant focused checks and the full repository gate.
- [ ] I replaced the blank `WORKLOG.md` template on this branch with the required product-safe record.
- [ ] I updated documentation, provider workflows, and installers where needed.
- [ ] Model overrides remain explicit and there is no silent fallback.
- [ ] I did not include credentials, profile state, personal paths, private logs, or proprietary project data.
- [ ] Any authenticated provider testing was explicitly authorized and is described without exposing account data.

## Risks and compatibility

Note affected providers, platforms, state migrations, rollback considerations,
and anything not verified.
