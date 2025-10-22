# .NET Version Manager (dver)

`dver` is a command-line tool to simplify managing multiple .NET SDK versions on your system. Inspired by `nvm` and `sdkman`, it provides an easy way to install, uninstall, and switch between .NET SDK versions.

## Features

- **`current`**: Check the currently active .NET SDK version.
- **`list`**: View all installed .NET SDK versions.
- **`use`**: Switch to a different .NET SDK version for your project by creating a `global.json` file.
- **`install`**: Install new .NET SDK versions, including LTS, specific versions, or versions from a specific channel, runtime, or architecture.
- **`uninstall`**: Remove specific .NET SDK versions.
- **`doctor`**: Check your system for common configuration issues.
- **`which`**: Display the installation path for the SDK versions managed by `dver`.
- **`remote`**: Inspect Microsoft's published release channels and their latest SDK/runtime versions.

## Why It Matters

In .NET development, different projects often require different SDK versions. `dver` helps you:

1.  **Ensure Consistency**: Keep your team on the same .NET SDK version.
2.  **Switch with Ease**: Quickly switch between .NET versions for different projects.
3.  **Simplify Setup**: Easily set up new development environments.
4.  **Control Versions**: Specify and control the exact .NET SDK version for each project.

## Installation

You can download the latest release for your operating system from the [Releases](https://github.com/stescobedo92/dotnet-version-manager/releases) page.

## Getting Started

After installing `dver`, it's recommended to run the `doctor` command to ensure your environment is set up correctly.

```bash
dver doctor
```

The `doctor` command will check if the .NET SDK installation directory is in your `PATH` and provide instructions on how to add it if it's missing. This is crucial for the `dotnet` command to find the SDKs installed by `dver`.

## Usage

### `install`

Install a specific .NET SDK version.

```bash
dver install --version 8.0.406
```

Install the latest Long-Term Support (LTS) version.

```bash
dver install --lts
```

Install from a specific channel (for example, the latest 9.0 preview builds).

```bash
dver install --channel 9.0 --architecture arm64
```

Install only the runtime components (no SDK) to a custom directory.

```bash
dver install --runtime aspnetcore --install-path /opt/dotnet
```

By default, `dver` installs SDKs to the standard user-level location (`~/.dotnet` on Linux/macOS, `%LOCALAPPDATA%\Microsoft\dotnet` on Windows).

### `list`

List all installed .NET SDK versions.

```bash
dver list
```

### `use`

Set the .NET SDK version for the current directory by creating a `global.json` file.

```bash
dver use 8.0.406
```

You can also tweak the `global.json` content via optional flags:

```bash
dver use 8.0.406 --roll-forward latestMinor --allow-prerelease
```

To write the file to a different directory, pass the `--path` option pointing to a folder or file path.

### `uninstall`

Uninstall a specific .NET SDK version.

```bash
dver uninstall --version 8.0.406
```

Uninstall all SDKs of a major version (e.g., all .NET 8 versions).

```bash
dver uninstall --version 8
```

Uninstall all installed .NET SDKs.

```bash
dver uninstall --all
```

### `which`

Display where the SDK versions are installed.

```bash
dver which 8
```

Omit the version to show every installed SDK path.

### `current`

Display the currently active .NET SDK version.

```bash
dver current
```

### `doctor`

Run checks to diagnose common issues with your environment.

```bash
dver doctor
```

### `remote`

Review Microsoft's published release channels and optionally inspect recent releases.

```bash
dver remote --include-preview --show-releases 3
```

Use `--channel` to filter a specific channel (e.g., `dver remote --channel 8.0`).
