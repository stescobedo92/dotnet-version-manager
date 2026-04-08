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

The release workflow skips itself when tag `v<crate-version>` already exists.
