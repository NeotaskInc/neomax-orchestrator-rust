# Distribution tooling

The scripts in this directory package the native workspace without contacting a provider or
reading an authenticated profile. They are build and verification tools, not runtime dependencies.
The public repository hosts the source and release workflow. A release is published only from a
verified tag after all target packages pass assembly and verification.

End users can use the repository bootstrap installers, `install.sh` on supported Unix hosts or
`install.ps1` on supported Windows hosts. Each selects the target, downloads the matching archive
and canonical `SHA256SUMS`, verifies the checksum before extraction, rejects unsafe archive layouts,
and invokes only the packaged `neomax install` command. They do not require Rust and do not edit
shell profiles. See `docs/INSTALLATION.md` for exact release, mirror, upgrade, and uninstall
guidance.

The archive also contains an opt-in zsh account-shortcut asset and its profile manager. Normal
installation leaves startup files unchanged. Run the installed manager explicitly with
`install --profile FILE --asset FILE` to add its bounded block, or `uninstall --profile FILE` to
remove only that block. The generated providerN functions call the canonical installed helpers
and rediscover account profiles in each new shell. Run the explicit uninstall before removing
Neomax if the profile still contains the managed block; `neomax uninstall` does not edit user
shell startup files.

## Target package

Build the workspace for a target, then package its release binaries:

```text
cargo build --workspace --locked --release --target x86_64-unknown-linux-gnu
bash dist/package.sh --target x86_64-unknown-linux-gnu
```

The archive contains the `neomax` multicall executable, provider aliases, auxiliary executables,
the OpenCode policy asset, provider-neutral workflows including `/neomax`, `/project`, `/rotate`,
`/find-issues`, and `/fix-issues`, the license, the README, the installation guide, and
the opt-in zsh account-shortcut assets, and `RELEASE-MANIFEST.json`. Unix aliases are
relative symlinks to `neomax`. Windows aliases are executable copies so installation does not
require symlink privileges.

## Install a package

Extract the archive, enter its package directory, and run the included multicall binary:

```text
./bin/neomax install
```

The native installer copies the multicall executable, `neomax-cli`, every provider launcher, the
portal, the usage agent, the worktree command, shared assets, and provider workflow files into the
current user's installation directories. Provider workflows are materialized for every discovered
default, account, and orchestrator profile. New profiles created through Neomax are seeded on
demand. The Claude settings merge adds only the owned orient, usage, and turn hooks and preserves
unrelated settings and hooks. It records ownership so upgrades and uninstall remain scoped.

On macOS, Linux, and Windows, a successful install also invokes the installed
`neomax-usage-agent install` command to keep local usage collection, rotation, and keepalive
maintenance running. This bounded service step is best effort and never edits shell profiles. Use
`./bin/neomax install --no-usage-agent` or set `NEOMAX_NO_USAGE_AGENT=1` for managed and hermetic
installs. Uninstall stops the owned service before removing its executable when available.
Existing files that were modified or do not belong to a previous Neomax installation are
preserved and cause a clear error. Use `--force` only when intentionally replacing those named
files.

The default Unix locations are `~/.local/bin` and `~/.local/share/neomax`. Windows uses the user's
local application directory. Set `NEOMAX_INSTALL_ROOT`, `NEOMAX_INSTALL_BIN`, or
`NEOMAX_INSTALL_SHARE` for a different user-scoped location. The installer never changes provider
credentials or invokes a provider CLI.

To remove an installation, run:

```text
neomax uninstall
```

Uninstall reads both ownership manifests, refuses modified workflow files by default, removes only
the recorded commands and owned Claude hooks, and leaves all provider profiles, credentials,
state, unrelated commands, and unrelated settings in place. `neomax uninstall --force` removes
modified files listed by those manifests only.

## Verification

`check-package.sh` rejects absolute and parent-directory archive entries, checks the expected
layout, verifies every manifest hash, and confirms alias targets or Windows copies.

```text
bash dist/check-package.sh \
  --archive target/dist/neomax-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --version 0.1.0 \
  --target x86_64-unknown-linux-gnu
```

`verify-install.sh` runs version, help, temporary configuration, and the native install/uninstall
path. It opts out of background service activation, puts fake provider executables first on
`PATH`, and fails if any provider executable is called. It uses temporary homes and installation
directories and never reads an existing login.

`checksums.sh` writes standard SHA256SUMS records for one or more archives. The release workflow
first uploads one temporary artifact for each of the seven targets, then
`assemble-release.sh` downloads and validates all seven archives, rejects missing or duplicate
target assets, verifies each per-target checksum, and writes one canonical SHA256SUMS file.
It also creates a release asset manifest, generated release notes, and a copy of LICENSE. The
assembly job runs for manual dispatches, but only a verified `v<workspace-version>` tag can run
the publish job. A verified tagged run creates or updates the non-draft release in
`NeotaskInc/neomax-orchestrator-rust` with permanent package archives and supporting assets.
Bootstrap installers are included automatically when a supported installer file exists at the
repository root.

Run the hermetic completeness test locally with:

```text
bash dist/test-release-assets.sh
```

The test builds fixture packages from fake binaries, checks the seven-target manifest, and proves
that missing workflow assets and duplicate archives are rejected. It never contacts GitHub or a
provider.
