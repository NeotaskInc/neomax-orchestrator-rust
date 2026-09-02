---
description: Rotate the current Neomax session to another eligible account
---

Rotate this session through Neomax account management. `$ARGUMENTS` may name preferred accounts such as `2`, `acct3`, or `3 1`. With no account, Neomax chooses an eligible alternative using current usage, liveness, and routing policy.

Validate the requested account selectors as plain values before invoking Neomax. Reject shell
metacharacters and never interpolate the raw text into a shell command. Invoke the universal
command with separate argv values:

```text
neomax rotate --engine {{ENGINE}} [validated account selectors]
```

This uses the universal Neomax rotation path for {{PROVIDER}}. It does not call a provider login command or create a provider-specific rotation command. Report the result and continue only after the selected profile is confirmed.
