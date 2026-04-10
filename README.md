# dver

`dver` is a `.NET SDK` version manager inspired by `nvm`.

Instead of installing SDKs into the shared system dotnet location, `dver` keeps each SDK isolated under its own managed directory and exposes a stable `dotnet` shim. That makes version switching predictable on Windows, Linux, and macOS.

## What changed

- Managed SDKs now live in a dedicated `dver` root:
  - Windows: `%LOCALAPPDATA%\dver`
  - Linux and macOS: `~/.dver`
- Each SDK is installed into its own isolated directory under `versions/<sdk-version>`.
- `dver setup` creates a `dotnet` shim and adds only the shim directory to `PATH`.
- `dver use --global` now works like `nvm alias default`: it sets the default managed SDK instead of writing `~/global.json`.
- Local `dver use <version>` still writes a project `global.json`.
- `dver install` now uses the official `dotnet-install` script internally instead of manually downloading SDK archives.

## Why this is more reliable

The previous implementation mixed managed SDKs with the shared user/system dotnet directories, depended on archive URLs that are easy to break, and treated `~/global.json` as a global switch even though that is not how `.NET` version resolution works.

The new flow is closer to `nvm`:

1. `dver install 8.0.406`
2. `dver setup`
3. `dver use 8.0.406 --global`
4. `dotnet --version`

Or for a project:

1. `dver install 8.0.406`
2. `dver use 8.0.406`
3. `dotnet --version`

## Commands

### Install

Install a specific SDK version:

```bash
dver install 8.0.406
```

Install the latest SDK from a channel:

```bash
dver install 8.0
```

Install the latest LTS SDK:

```bash
dver install --lts
```

### Setup

Create the `dotnet` shim and add it to your PATH:

```bash
dver setup
```

Run this once after installing `dver`, then restart your shell.

### Use

Set the default managed SDK:

```bash
dver use 8.0.406 --global
```

Create a local `global.json` in the current project:

```bash
dver use 8.0.406
```

Clear the default managed SDK:

```bash
dver use --global --clear
```

Remove the local `global.json`:

```bash
dver use --clear
```

### List

```bash
dver list
```

### Current

```bash
dver current
```

### Doctor

```bash
dver doctor
```

### Uninstall

Remove a managed SDK:

```bash
dver uninstall 8.0.406
```

Remove every managed SDK and the `dver` PATH configuration:

```bash
dver uninstall --all
```

System-installed `.NET` SDKs are not removed.

## Release automation

The repository now includes:

- `CI` on Windows, Linux, and macOS
- automatic `crates.io` publishing when a new `Cargo.toml` version is pushed to `main` or `master`
- automatic GitHub Release creation using the same crate version
- automatic binary packaging for Windows, Linux, and macOS
- automatic Snap package builds, with Snap Store publication when `SNAPCRAFT_STORE_CREDENTIALS` is configured
- automatic Scoop manifest publication to the `scoop-bucket` branch

`Cargo.toml` is the single source of truth for versioning:

- bump the crate version in `Cargo.toml`
- push to `main` or `master`
- the workflow creates tag `v<version>` automatically
- the workflow publishes to `crates.io`
- the workflow creates the GitHub Release with the packaged binaries

The release workflow skips itself when tag `v<crate-version>` already exists.

## Additional distribution channels

### Snapcraft

The repository now contains `snap/snapcraft.yaml` and the release workflow builds a snap automatically.

To publish that snap to the Snap Store, configure the repository secret:

- `SNAPCRAFT_STORE_CREDENTIALS`

Generate it with `snapcraft export-login` for the registered snap name, then store the exported login file content as the secret.

This snap uses `classic` confinement because `dver` is a host-facing version manager and needs access to shell profiles and managed SDK directories.

### Scoop

The release workflow automatically updates a Scoop manifest on the `scoop-bucket` branch.

Users can install from that bucket with:

```powershell
scoop bucket add dver https://github.com/stescobedo92/dotnet-version-manager --branch scoop-bucket
scoop install dver
```

### Flatpak

Flatpak is intentionally not enabled for automatic distribution.

`dver` is designed to manage host SDKs, PATH shims, and shell startup files. That behavior conflicts with Flatpak sandboxing, so shipping a Flatpak package would be misleading unless the tool is redesigned around host-bridging behavior.

### Required secrets

Store-backed publishing still requires these repository secrets:

- `CARGO_REGISTRY_TOKEN`
- `SNAPCRAFT_STORE_CREDENTIALS`
