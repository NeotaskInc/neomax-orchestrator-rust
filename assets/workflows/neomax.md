---
description: Run the Neomax orchestrator across the enabled provider pool
---

You are the {{PROVIDER}} entry point for Neomax. The same Neomax command surface coordinates Claude, Codex, OpenCode, Kimi, and Grok sessions. Keep the current project and its instructions in scope. Do not invent provider-specific authentication or routing commands.

Task: $ARGUMENTS

1. Recover state before starting work:

   ```sh
   neomax ls
   neomax reconcile
   ```

   Resume, retry, acknowledge, or clean existing runs as appropriate. Do not abandon a completed run in the inbox.

2. Inspect the project and issue queue. Use `neomax projects`, `neomax issue list --json`, and `neomax queue status` when they apply.

   If the requested work belongs to another registered project, use `/project`
   first. It resolves the registry, changes focus to the selected project root,
   and refreshes the project context through the same canonical Neomax commands
   on every supported provider.

3. For independent work, dispatch complete briefs with `neomax auto`. Include the objective, context, exact scope, constraints, acceptance checks, and files that must not be touched. Use `--engine claude`, `--engine codex`, `--engine opencode`, `--engine kimi`, or `--engine grok` only when the routing decision is explicit. Otherwise let Neomax select an eligible account and model from the configured pool.

4. Keep concurrent work isolated. Review every result, run the relevant checks, and reconcile runs before reporting completion. Preserve the project's work log and use the repository's normal pull request workflow.

The provider wrapper selected this session. The orchestrator remains universal and may route any task to any eligible provider. If this session needs another account, use `/rotate` or `neomax rotate --engine {{ENGINE}}`.
