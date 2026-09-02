---
description: Focus the session on a registered Neomax project
---

Focus this session on a registered Neomax project. `$ARGUMENTS` may be empty,
`list`, a project name, or an unambiguous project-name prefix.

1. Run `neomax projects --json`. If the argument is empty or `list`, show each
   project's name, root, repositories, and branch prefix, then stop.
2. Resolve a specific argument against that registry without guessing. If it
   is missing or ambiguous, show the matching projects and stop. Do not create,
   remove, or rewrite a registration just to satisfy this workflow.
3. Use the selected project for the rest of the session. Change into its
   registered root before every `neomax` dispatch. When work belongs to one
   repository, change into that repository beneath the same root. Use the
   project's branch prefix and never mix another project's namespace.
4. Read the selected project's root `AGENTS.md` and `CLAUDE.md` when present,
   then read the configured orchestrator and planning files from the registry.
   Treat those files as project instructions, not as Neomax configuration.
5. Refresh context from the canonical tool surface after changing directory:
   run `neomax orient --json` inside an orchestrator session and use
   `neomax projects --json` elsewhere. Use the exact project name with
   `--project NAME` on commands that support an explicit project selector.
6. Confirm the selected name, root, repositories, branch prefix, and planning
   location before dispatching work. Keep every run, task, issue, and worktree
   scoped to that project.

The project workflow is provider-neutral. It uses the canonical `projects`
and `orient` Neomax commands, so the same list, use, and context behavior is
available from Claude, Codex, OpenCode, Kimi, and Grok. It does not invoke a
provider login command or start another orchestrator.
