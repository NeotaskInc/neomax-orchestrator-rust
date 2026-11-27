# Work log

## 2026-11-27 - Antigravity sign-in probe and Neon worker delegation

Antigravity profiles are now confirmed signed in before Neomax uses them.
Discovery reuses the `agy models` run it already makes for model listing: when
that run completes with a non-zero exit, as agy does when signed out ("Please
sign in"), native sign-in profiles become unauthenticated. A timed-out or
truncated listing keeps the file evidence, and Gemini API-key profiles are not
changed. A signed-out agy is therefore refused at once instead of a launch
waiting at agy's sign-in prompt. The probe does not depend on where agy keeps
its token, so a keyring or file check can be added behind the same function.

An Antigravity orchestrator's delegation order gains its second step: its own
subagents and threads for bounded side work, then Antigravity (Neon) workers
for units that own a task, branch or pull request, then Claude Code workers on
Opus at effort high. Reviewers stay Claude, and Fable stays refused. The neomax
skill and reference describe the order and the probe.

Affected areas: provider discovery, Antigravity auth detection, the
orchestrator delegation text, the neomax skill and `docs/REFERENCE.md`.

Verification: formatting, Clippy with `-D warnings`, documentation style and
all workspace test targets passed. With the real agy 1.5.18 binary signed out
in a temporary home, `amax --foreground "hi"` exited in under a second with
"no eligible authenticated antigravity orchestrator account"; with a fake agy
whose `models` succeeds, the same launch planned the Antigravity orchestrator.

Remaining limits: a failed `agy models` run for a reason other than sign-in,
such as a network error, also marks the native profile signed out until the
next discovery.
