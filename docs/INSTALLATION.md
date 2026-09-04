# Installing Neomax

Neomax is available as precompiled release packages and as source. Release installation does not
require Rust or Cargo. Source builds require Rust 1.85 or newer, Cargo, Git, and a native linker.

## Bootstrap installers

The repository includes `install.sh` for supported Unix hosts and `install.ps1` for supported
Windows hosts. These installers select the host target, download the release archive and the
matching `SHA256SUMS` file, verify the archive before extraction, reject unsafe archive entries,
and run the packaged `neomax install` command. They do not edit shell profiles, install Rust, read
provider credentials, or invoke a provider. Use the printed PATH command after installation.

Linux and macOS:

```bash
curl -fsSL https://neotask.ai/neomax-orchestrator/install.sh | bash
```

Windows PowerShell:

```powershell
Invoke-RestMethod https://neotask.ai/neomax-orchestrator/install.ps1 | Invoke-Expression
```

Set `NEOMAX_VERSION=0.1.0` or `$env:NEOMAX_VERSION = '0.1.0'` to install an exact release. Without
it, the installer resolves the latest GitHub release. `NEOMAX_TARGET` can select another supported
release target when the host and package are intentionally matched.

## Build and run from source

Rustup installs Rust and Cargo together. Follow the official
[Rust installation guide](https://www.rust-lang.org/tools/install). On Linux or macOS, use the
official rustup command:

```bash
curl --proto '=https' --tlsv1.2 --silent --show-error --fail https://sh.rustup.rs | sh
```

Ubuntu and Debian users can install the source-build prerequisites with:

```bash
sudo apt update
sudo apt install -y build-essential git curl
```

Install the Xcode command-line tools on macOS with `xcode-select --install`. Other Linux
distributions require Git, curl, and a C toolchain such as GCC or Clang. On Windows, the official
Rust installer guides users through Rustup and the required Visual Studio C++ build tools.

Build and run from a checkout:

```bash
git clone https://github.com/NeotaskInc/neomax-orchestrator-rust.git
cd neomax-orchestrator-rust
cargo build --workspace --locked --release
cargo test --workspace --locked
./target/release/neomax --help
```

On Windows, run `.\target\release\neomax.exe --help`. The workspace build also creates
`neomax-portal`, `neomax-usage-agent`, and `neomax-worktrees` under `target/release`. It does not
modify the user account or install provider workflows. Use a verified release package for the
complete installed command surface, or follow `dist/README.md` to assemble and verify a package
from the source build.

For an offline mirror, set `NEOMAX_VERSION` and `NEOMAX_BASE_URL`. The base URL must contain a
`vVERSION` directory with the exact archive name and `SHA256SUMS`, for example:

```text
MIRROR/v0.1.0/SHA256SUMS
MIRROR/v0.1.0/neomax-v0.1.0-aarch64-apple-darwin.tar.gz
```

Use `file:///...` for a local mirror. A trusted local HTTP mirror requires
`NEOMAX_ALLOW_HTTP=1`. To test latest resolution from a mirror, set `NEOMAX_LATEST_URL` to a
JSON file containing `{"tag_name":"v0.1.0"}`. The repository and metadata endpoints can be
overridden with `NEOMAX_REPOSITORY` and `NEOMAX_LATEST_URL`.

After extracting a package, run the native installer from the package directory:

```text
./bin/neomax install
```

On Windows, run `bin\neomax.exe install` from the extracted package directory.

The installer creates the user command surface in one transaction. It installs the `neomax`
multicall executable, the `neomax-cli` compatibility name, the `cmax`, `cdx`, `cdxmax`, `ocx`,
`ocmax`, `kmx`, `kmax`, `gmx`, and `gmax` launchers, and the `neomax-portal`,
`neomax-usage-agent`, and `neomax-worktrees` auxiliary commands. It also installs the OpenCode
model policy and product documentation assets.

The three auxiliary commands are universal. `neomax-portal` reports the combined provider view,
`neomax-usage-agent` collects usage and runs maintenance for all supported providers, and
`neomax-worktrees` manages provider-neutral coordinated Git worktrees. No provider-prefixed
worktree or usage-agent executable is installed.

It also installs the provider workflow surface. Claude receives commands under
`commands`, Codex receives prompts under `prompts`, OpenCode receives commands under its XDG
configuration directory, Kimi receives `skills/<workflow>/SKILL.md`, and Grok receives commands
under `commands`. The same canonical workflow is rendered for each provider, so `/neomax`,
`/project`, `/rotate`, `/find-issues`, and `/fix-issues` use the universal Neomax command
surface. `/project` lists registered projects, focuses the session on a selected project root,
and refreshes context through `neomax projects` and `neomax orient`. `/rotate` always invokes
`neomax rotate --engine ENGINE` and does not rely on a provider-specific login command.

For Claude profiles, installation merges the required `SessionStart`, `Stop`, and
`UserPromptSubmit` hooks into an existing JSON settings object. It adds only Neomax-owned commands,
keeps existing matcher groups and hooks intact, and records ownership for a scoped uninstall. A
profile created later through a Neomax account helper receives the same workflow files and hooks.

The default user locations are:

- Unix: `~/.local/bin` and `~/.local/share/neomax`
- Windows: the user's local application directory

On macOS, Linux, and Windows, a successful install also invokes the newly installed
`neomax-usage-agent install` command. That creates and starts the per-user background service
that collects local usage, runs the rotation and keepalive maintenance ticks, and periodically
tidies safe Neomax worktrees. The tidy interval defaults to 600 seconds and is controlled by
`NEOMAX_WORKTREE_TIDY_EVERY`; set it to `0` to disable only that periodic sweep. Each tidy
invocation has a separate 300-second default timeout controlled by
`NEOMAX_WORKTREE_TIDY_TIMEOUT_SECS`. Set these values before installation and reinstall the
usage-agent service after changing them. The service activation is bounded and is best effort: if
the service manager is unavailable, the installer prints a warning with the manual recovery
command while keeping the completed file installation.
The installer never edits shell profiles as part of this step.

## Optional zsh account shortcuts

The normal install does not edit a shell startup file. On a Unix-like system using zsh, the
installed shell asset can be enabled explicitly when the `providerN` convenience names are useful:

```text
NEOMAX_SHARE="${NEOMAX_INSTALL_SHARE:-$HOME/.local/share/neomax}"
sh "$NEOMAX_SHARE/shell/neomax-shell-shortcuts.sh" install \
  --profile "$HOME/.zshrc" \
  --asset "$NEOMAX_SHARE/shell/neomax-aliases.zsh"
```

The generated `claudeN`, `codexN`, `opencodeN`, `kimiN`, and `grokN` functions discover existing
account profiles each time a new shell starts. They call the installed `cmax`, `cdx`, `ocx`,
`kmx`, and `gmx` helpers, respectively, and do not contain credentials or account-specific paths.
The explicit manager owns only its bounded profile block. Remove that block with:

```text
sh "$NEOMAX_SHARE/shell/neomax-shell-shortcuts.sh" uninstall --profile "$HOME/.zshrc"
```

Run this explicit uninstall before removing Neomax if the profile still contains the managed
block. The normal `neomax uninstall` command does not edit user shell startup files.

Managed, CI, and hermetic installs can opt out explicitly:

```text
./bin/neomax install --no-usage-agent
NEOMAX_NO_USAGE_AGENT=1 ./bin/neomax install
```

The opt-out applies only to automatic service activation. It does not remove an existing service.
`neomax uninstall` first invokes the installed `neomax-usage-agent uninstall` command, when the
owned agent exists, before removing the installed files. This prevents a service from continuing
to reference a removed executable. Provider credentials and provider commands are not used by
the native installer or its service activation step.

The installer does not install Rust or modify provider credentials. Add the installed `bin`
directory to `PATH` using the normal shell or operating-system settings.

## Persistent concurrency settings

Neomax keeps the one-place concurrency settings in one user configuration file:

- Unix-like systems: `~/.config/neomax/config.toml`, or `$XDG_CONFIG_HOME/neomax/config.toml` when `XDG_CONFIG_HOME` is set.
- Windows: `%APPDATA%\neomax\config.toml`.

The `[concurrency]` table controls persistent limits such as `max_subagents` and
`max_sessions_per_account`. Set them without editing TOML when the installed CLI is available:

```text
neomax config set max-subagents 80
neomax config set max-sessions-per-account 12
```

For a one-process override, use `NEOMAX_MAX_SUBAGENTS` or
`NEOMAX_MAX_SESSIONS_PER_ACCOUNT`. The `neomax config set` commands write the persistent
configuration file; environment variables apply only to the process and its children.

## Upgrades and rollback

Each installation writes `share/neomax/install-manifest.json`. An upgrade checks that existing
files still match the recorded installation and stages the replacement before activating it. A
modified or unrelated file is preserved and causes an error unless `--force` is supplied. If an
activation step fails, the previous files are restored.

To upgrade with the bootstrap installer, rerun it with the desired `NEOMAX_VERSION`. To upgrade
from an already extracted package, run that package's `neomax install` command. Both paths keep
the installation transactional and preserve modified or unrelated files unless `--force` is
explicit.

Override the user locations for a managed or test installation with:

```text
NEOMAX_INSTALL_ROOT=/path/to/user-scope ./bin/neomax install
```

`NEOMAX_INSTALL_BIN` and `NEOMAX_INSTALL_SHARE` can override the two directories independently.

## Uninstall

Run:

```text
neomax uninstall
```

Only files recorded by the ownership manifest are removed. Provider profiles, credentials,
`NEOMAX_HOME`, worktrees, logs, history, and unrelated files are not touched. Modified installed
files are preserved unless the explicit `--force` option is used. Workflow files and Claude hook
ownership are recorded separately so removing Neomax cannot remove an unrelated provider command
or settings entry.

All installation behavior is local and hermetic. Help, version, install, and uninstall checks use
temporary directories and never execute Claude, Codex, OpenCode, Kimi, or Grok.
