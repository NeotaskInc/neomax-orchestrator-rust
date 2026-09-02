# Scheduler capacity

`run-all` derives its default live-worker capacity from the effective settings
and the eligible worker-account count. The account lane count is bounded by
`max_sessions_per_account`; the resulting account capacity is then clamped by
`max_subagents`, a non-zero `max_tasks`, and an optional fleet-wide live cap.

`--max-live` is an explicit per-plan ceiling. It is rejected when it is
greater than the effective shared capacity instead of being silently reduced.
The detached supervisor receives the same resolved value as the foreground
scheduler. It does not replace the fleet-wide admission cap; both limits apply.

The default fleet-wide live-worker cap is `50`. `NEOMAX_MAX_LIVE` and its
compatibility alias `NEOMAX_FLEET_CAP` lower or override that cap; the effective
fleet cap is still bounded by `max_subagents`, and neither variable is an alias
for `max_subagents`. A value of `0` is valid and denies worker dispatch.
Existing live workers can inform the displayed default, but they are not the
admission authority. Every new dispatch is checked against the shared lease
state while holding the admission lock, so a plan cannot exceed the cap even
when another process changes the live set between those observations.

## Shared dispatch admission

Run-all and direct worker dispatch use the same file-backed admission authority
at `dispatch-admission.json` in the state directory. Each dispatch reserves its
fleet slot, task slot, and known provider slot under one file lock before
account selection or provider launch. Account lanes and sessions are bound
before the run record is created. The lease remains held through the pre-PID
window and is released when the worker reaches a terminal outcome. A lease
whose owner is dead or whose TTL has expired is reclaimed on the next
admission transaction. A fleet cap of `0` denies every new dispatch.

The existing task queue still allocates reservations in FIFO order. The shared
authority is the final atomic gate, so a stale capacity snapshot cannot admit
more work than the configured limits.
