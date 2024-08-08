# .NET Version Manager (dver)

## Overview

`dver` is a command-line tool designed to simplify the process of managing multiple .NET SDK versions on your system. Inspired by popular version managers like nvm (Node Version Manager) and sdkman, this tool provides an easy and efficient way to switch between different .NET SDK versions, install new versions, and maintain consistent development environments across projects.

By providing an easy-to-use interface for managing .NET SDK versions, this tool aims to streamline the development process and reduce version-related headaches. Whether you're working on multiple projects with different .NET version

## Features

- `current`: Quickly check the currently active .NET SDK version.
- `list`: View all installed .NET SDK versions on your system.
- `use` : Easily switch to a different .NET SDK version for your project.
- `install` : Automatically download and install the lts version or install a new .NET SDK versions

## Why It Matters
In the fast-paced world of .NET development, different projects often require different SDK versions. This tool addresses several key challenges:

1. Consistency: Ensure all team members are using the same .NET SDK version, reducing "works on my machine" issues.
2. Flexibility: Quickly switch between .NET versions for different projects without manual intervention.
3. Ease of Setup: Simplify the process of setting up new development environments or onboarding new team members.
4. Version Control: Easily specify and control the exact .NET SDK version for each project, improving reproducibility and reliability.

## Usage

```bash
./dver current  # Display current .NET SDK version

Current dotnet version: 6.0.132
```
```bash
./dver list  # List all installed .NET SDK versions

6.0.132
8.0.105
```

```bash
./dver use 6.0.132  # Switch to .NET SDK version

SDK version set to 6.0.132
```

```bash
./dver install --lts  # Install LTS version if not present

dotnet is already installed on your system.
Current version: 6.0.132
```

```bash
./dver install --version  7.0.100  # (Although you can use this command, it is still a work in progress so it is in the experimental phase.)
```