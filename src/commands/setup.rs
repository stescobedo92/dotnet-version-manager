use crate::utils::common::get_home_dir;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn move_to_top_of_path() -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("Could not determine home directory")?;

    // Determine the managed directory location based on OS
    let dver_dir = if cfg!(windows) {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("dotnet")
        } else {
            home.join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("dotnet")
        }
    } else {
        home.join(".dotnet")
    };

    println!("Configuring PATH for: {:?}", dver_dir);
    println!("This will ensure managed .NET SDKs take precedence.");

    if cfg!(windows) {
        configure_windows_path(&dver_dir)?;
    } else {
        configure_unix_path(&dver_dir)?;
    }

    println!("✅ Configuration applied successfully.");
    println!("⚠️  You may need to restart your terminal or shell for changes to take effect.");

    Ok(())
}

#[cfg(windows)]
fn configure_windows_path(dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = hkcu.create_subkey("Environment")?;

    let current_path: String = env.get_value("Path").unwrap_or_default();
    let dver_str = dver_dir.to_str().ok_or("Invalid path string")?;

    // Check if distinct parts already contain it
    let mut parts: Vec<&str> = current_path.split(';').filter(|s| !s.is_empty()).collect();

    // Remove existing dver entries to avoid duplicates and ensure we are top
    parts.retain(|&p| p != dver_str && Path::new(p) != dver_dir);

    // Prepend
    parts.insert(0, dver_str);

    let new_path = parts.join(";");

    env.set_value("Path", &new_path)?;

    // Broadcast change (simplified, better to just tell user to restart)
    // In a real CLI usually we just modify registry and ask for restart.

    println!("Updated User Environment Variable 'Path' in Registry.");

    Ok(())
}

#[cfg(not(windows))]
fn configure_windows_path(_dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(unix)]
fn configure_unix_path(dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("No home dir")?;
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    let rc_file = if shell.contains("zsh") {
        home.join(".zshrc")
    } else if shell.contains("bash") {
        if cfg!(target_os = "macos") {
            home.join(".bash_profile")
        } else {
            home.join(".bashrc")
        }
    } else {
        home.join(".profile")
    };

    println!("Detected shell: {}", shell);
    println!("Updating configuration file: {:?}", rc_file);

    let dver_dir_str = dver_dir.to_str().ok_or("Invalid path")?;

    // We want to prepend to PATH.
    // export DOTNET_ROOT="..."
    // export PATH="$DOTNET_ROOT:$PATH"

    let lines_to_add = format!(
        "\n# dver configuration\nexport DOTNET_ROOT=\"{}\"\nexport PATH=\"$DOTNET_ROOT:$PATH\"\n",
        dver_dir_str
    );

    // Read file to check if already present
    let content = if rc_file.exists() {
        fs::read_to_string(&rc_file)?
    } else {
        String::new()
    };

    if content.contains("dver configuration") || content.contains(dver_dir_str) {
        println!("Configuration seems to already exist in {:?}.", rc_file);
        println!("We will append the precedence fix to the end of the file to ensure it overrides other settings.");
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_file)?;

    write!(file, "{}", lines_to_add)?;

    println!("Added configuration to {:?}.", rc_file);

    Ok(())
}

#[cfg(not(unix))]
fn configure_unix_path(_dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

pub fn remove_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("Could not determine home directory")?;

    // Determine the managed directory location based on OS for matching purposes
    let dver_dir = if cfg!(windows) {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("dotnet")
        } else {
            home.join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("dotnet")
        }
    } else {
        home.join(".dotnet")
    };

    if cfg!(windows) {
        remove_windows_path(&dver_dir)?;
    } else {
        remove_unix_path(&dver_dir)?;
    }

    Ok(())
}

#[cfg(windows)]
fn remove_windows_path(dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = hkcu.create_subkey("Environment")?;

    let current_path: String = env.get_value("Path").unwrap_or_default();
    let dver_str = dver_dir.to_str().ok_or("Invalid path string")?;

    let mut parts: Vec<&str> = current_path.split(';').filter(|s| !s.is_empty()).collect();

    // Remove dver entries
    let original_len = parts.len();
    parts.retain(|&p| p != dver_str && Path::new(p) != dver_dir);

    if parts.len() < original_len {
        let new_path = parts.join(";");
        env.set_value("Path", &new_path)?;
        println!("✅ Removed dver from User PATH in Registry.");
    } else {
        println!("dver not found in User PATH.");
    }

    Ok(())
}

#[cfg(not(windows))]
fn remove_windows_path(_dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(unix)]
fn remove_unix_path(dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("No home dir")?;
    let _shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    // We try to clean up known config files.
    // Since 'setup' only targets one file based on SHELL, we should try to remove from that one.
    // Or ideally, check all common ones? Let's stick to the detection logic for now to avoid side effects.

    let rc_files = vec![
        home.join(".zshrc"),
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".profile"),
    ];

    let dver_dir_str = dver_dir.to_str().ok_or("Invalid path")?;
    let marker = "# dver configuration";

    for rc_file in rc_files {
        if rc_file.exists() {
            let content = fs::read_to_string(&rc_file)?;
            if content.contains(marker) {
                // We basically want to remove the block we added.
                // Naive approach: remove lines containing our distinct signatures?
                // Our block is:
                // \n# dver configuration\nexport DOTNET_ROOT="..."\nexport PATH="$DOTNET_ROOT:$PATH"\n

                let explicit_export_root = format!("export DOTNET_ROOT=\"{}\"", dver_dir_str);

                // Let's filter out lines that look like our config
                let new_lines: Vec<&str> = content
                    .lines()
                    .filter(|line| !line.contains(marker))
                    .filter(|line| !line.contains(&explicit_export_root))
                    .filter(|line| !line.contains("export PATH=\"$DOTNET_ROOT:$PATH\"")) // This is generic, might be risky?
                    // But in our setup we wrote it exactly like that.
                    // If user wrote it manually differently, we won't touch it.
                    .collect();

                let new_content = new_lines.join("\n");

                // Only write if changed (length difference - assuming we removed something)
                // Note: new_lines.join adds newlines back but might change trailing newline behavior. This is usually fine for rc files.
                if new_content.len() < content.len() {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .open(&rc_file)?;
                    write!(file, "{}", new_content)?;
                    #[cfg(unix)]
                    {
                        // Ensure trailing newline if file non-empty
                        if !new_content.is_empty() {
                            write!(file, "\n")?;
                        }
                    }
                    println!("✅ Removed dver configuration from {:?}", rc_file);
                }
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn remove_unix_path(_dver_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

/// Install shell wrapper functions to intercept package manager dotnet installs.
pub fn install_intercept_wrappers() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📦 Installing package manager intercept wrappers...");

    if cfg!(windows) {
        install_windows_intercept()?;
    } else {
        install_unix_intercept()?;
    }

    println!("✅ Intercept wrappers installed.");
    println!("   Now 'brew install dotnet' (and similar) will be redirected to 'dver install'.");

    Ok(())
}

#[cfg(unix)]
fn install_unix_intercept() -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("No home dir")?;
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    let rc_file = if shell.contains("zsh") {
        home.join(".zshrc")
    } else if shell.contains("bash") {
        if cfg!(target_os = "macos") {
            home.join(".bash_profile")
        } else {
            home.join(".bashrc")
        }
    } else {
        home.join(".profile")
    };

    let wrapper_code = r#"
# dver intercept wrappers - Redirects package manager dotnet installs to dver
# Auto-generated by dver setup --intercept

_dver_get_path() {
    # Find dver in common locations
    if command -v dver &>/dev/null; then
        echo "dver"
    elif [[ -x "/usr/local/bin/dver" ]]; then
        echo "/usr/local/bin/dver"
    elif [[ -x "$HOME/.local/bin/dver" ]]; then
        echo "$HOME/.local/bin/dver"
    elif [[ -x "$HOME/.dver/bin/dver" ]]; then
        echo "$HOME/.dver/bin/dver"
    else
        echo ""
    fi
}

_dver_check_version_installed() {
    local version="$1"
    local dver_cmd="$2"
    
    if [[ -z "$dver_cmd" ]]; then
        return 1
    fi
    
    # Check if version is installed by listing and grepping
    "$dver_cmd" list 2>/dev/null | grep -q "$version"
}

_dver_extract_dotnet_version() {
    local arg="$1"
    local version=""
    
    # Handle: dotnet-sdk@8, dotnet@8.0, dotnet-sdk, dotnet
    if [[ "$arg" =~ @([0-9.]+)$ ]]; then
        version="${BASH_REMATCH[1]}"
    fi
    
    echo "$version"
}

brew() {
    local is_dotnet_install=false
    local version=""
    local pkg_name=""
    
    # Parse args to detect dotnet installation
    # Formats: brew install dotnet, brew install dotnet@8, brew install --cask dotnet-sdk@8
    for arg in "$@"; do
        if [[ "$arg" == dotnet* || "$arg" == *dotnet-sdk* || "$arg" == *dotnet-runtime* ]]; then
            is_dotnet_install=true
            pkg_name="$arg"
            version=$(_dver_extract_dotnet_version "$arg")
            break
        fi
    done
    
    if [[ "$1" == "install" && "$is_dotnet_install" == true ]]; then
        local dver_cmd=$(_dver_get_path)
        
        if [[ -z "$dver_cmd" ]]; then
            echo "⚠️  dver is not installed or not in PATH. Running original brew command..."
            command brew "$@"
            return
        fi
        
        if [[ -z "$version" ]]; then
            # No version specified, install LTS
            echo "🔄 dver: Detected 'brew install $pkg_name'. Redirecting to dver..."
            echo "   Installing latest LTS version via dver..."
            "$dver_cmd" install --lts
        else
            # Check if already installed
            if _dver_check_version_installed "$version" "$dver_cmd"; then
                echo "✅ dver: Detected you want to install .NET SDK version $version."
                echo "   Version $version is ALREADY installed on your system."
                echo "   Use 'dver use $version' to select it, or 'dver list' to see all versions."
            else
                echo "🔄 dver: Detected you want to install .NET SDK version $version."
                echo "   Proceeding with installation of version $version via dver..."
                "$dver_cmd" install --version "$version"
            fi
        fi
    else
        command brew "$@"
    fi
}

apt() {
    local is_dotnet=false
    for arg in "$@"; do
        if [[ "$arg" == dotnet* ]]; then
            is_dotnet=true
            break
        fi
    done
    
    if [[ "$1" == "install" && "$is_dotnet" == true ]]; then
        local dver_cmd=$(_dver_get_path)
        
        if [[ -z "$dver_cmd" ]]; then
            echo "⚠️  dver is not installed or not in PATH. Running original apt command..."
            command apt "$@"
            return
        fi
        
        echo "🔄 dver: Detected 'apt install $2'. Redirecting to dver..."
        echo "   Installing latest LTS version via dver..."
        "$dver_cmd" install --lts
    else
        command apt "$@"
    fi
}

snap() {
    local is_dotnet=false
    for arg in "$@"; do
        if [[ "$arg" == dotnet* ]]; then
            is_dotnet=true
            break
        fi
    done
    
    if [[ "$1" == "install" && "$is_dotnet" == true ]]; then
        local dver_cmd=$(_dver_get_path)
        
        if [[ -z "$dver_cmd" ]]; then
            echo "⚠️  dver is not installed or not in PATH. Running original snap command..."
            command snap "$@"
            return
        fi
        
        echo "🔄 dver: Detected 'snap install $2'. Redirecting to dver..."
        echo "   Installing latest LTS version via dver..."
        "$dver_cmd" install --lts
    else
        command snap "$@"
    fi
}
"#;

    // Check if already installed
    let content = if rc_file.exists() {
        fs::read_to_string(&rc_file)?
    } else {
        String::new()
    };

    if content.contains("dver intercept wrappers") {
        // Remove old wrappers first, then add new ones
        println!("   Updating existing intercept wrappers in {:?}", rc_file);
        let new_content = remove_old_intercept_wrappers(&content);
        fs::write(&rc_file, &new_content)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_file)?;

    write!(file, "{}", wrapper_code)?;

    println!("   Added intercept wrappers to {:?}", rc_file);

    Ok(())
}

#[cfg(unix)]
fn remove_old_intercept_wrappers(content: &str) -> String {
    // Find and remove everything from "# dver intercept wrappers" to the end of the snap function
    let mut result = String::new();
    let mut in_wrapper_block = false;
    let mut brace_count = 0;
    let mut skip_until_matching_brace = false;

    for line in content.lines() {
        if line.contains("# dver intercept wrappers") {
            in_wrapper_block = true;
            continue;
        }

        if in_wrapper_block {
            // Count braces to find end of functions
            for c in line.chars() {
                if c == '{' {
                    brace_count += 1;
                    skip_until_matching_brace = true;
                } else if c == '}' {
                    brace_count -= 1;
                }
            }

            // Check if we're at the end of snap function (last function)
            if line.trim() == "}" && brace_count == 0 && skip_until_matching_brace {
                // Check if next lines don't have more wrapper functions
                in_wrapper_block = false;
                skip_until_matching_brace = false;
            }
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}

#[cfg(not(unix))]
fn remove_old_intercept_wrappers(_content: &str) -> String {
    String::new()
}

#[cfg(not(unix))]
fn install_unix_intercept() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(windows)]
fn install_windows_intercept() -> Result<(), Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("No home dir")?;

    // PowerShell profile path
    let profile_dir = home.join("Documents").join("WindowsPowerShell");
    let profile_file = profile_dir.join("Microsoft.PowerShell_profile.ps1");

    // Create directory if needed
    if !profile_dir.exists() {
        fs::create_dir_all(&profile_dir)?;
    }

    let wrapper_code = r#"
# dver intercept wrappers - Redirects package manager dotnet installs to dver
# Auto-generated by dver setup --intercept

function Get-DverPath {
    # Find dver in common locations
    if (Get-Command dver -ErrorAction SilentlyContinue) {
        return "dver"
    } elseif (Test-Path "$env:LOCALAPPDATA\Programs\dver\dver.exe") {
        return "$env:LOCALAPPDATA\Programs\dver\dver.exe"
    } elseif (Test-Path "$env:ProgramFiles\dver\dver.exe") {
        return "$env:ProgramFiles\dver\dver.exe"
    } elseif (Test-Path "$env:USERPROFILE\.dver\bin\dver.exe") {
        return "$env:USERPROFILE\.dver\bin\dver.exe"
    }
    return $null
}

function Test-DverVersionInstalled {
    param([string]$Version, [string]$DverCmd)
    
    if (-not $DverCmd) { return $false }
    
    $output = & $DverCmd list 2>$null
    return $output -match $Version
}

function choco {
    $isDotnet = $false
    $version = ""
    $pkgName = ""
    
    foreach ($arg in $args) {
        if ($arg -like "dotnet*") {
            $isDotnet = $true
            $pkgName = $arg
            if ($arg -match '@(\d+[\d.]*)') {
                $version = $Matches[1]
            }
            break
        }
    }
    
    if ($args[0] -eq "install" -and $isDotnet) {
        $dverCmd = Get-DverPath
        
        if (-not $dverCmd) {
            Write-Host "⚠️  dver is not installed or not in PATH. Running original choco command..." -ForegroundColor Yellow
            & "${env:ChocolateyInstall}\bin\choco.exe" @args
            return
        }
        
        if (-not $version) {
            Write-Host "🔄 dver: Detected 'choco install $pkgName'. Redirecting to dver..." -ForegroundColor Cyan
            Write-Host "   Installing latest LTS version via dver..." -ForegroundColor Cyan
            & $dverCmd install --lts
        } else {
            if (Test-DverVersionInstalled -Version $version -DverCmd $dverCmd) {
                Write-Host "✅ dver: Detected you want to install .NET SDK version $version." -ForegroundColor Green
                Write-Host "   Version $version is ALREADY installed on your system." -ForegroundColor Green
                Write-Host "   Use 'dver use $version' to select it, or 'dver list' to see all versions." -ForegroundColor Green
            } else {
                Write-Host "🔄 dver: Detected you want to install .NET SDK version $version." -ForegroundColor Cyan
                Write-Host "   Proceeding with installation of version $version via dver..." -ForegroundColor Cyan
                & $dverCmd install --version $version
            }
        }
    } else {
        & "${env:ChocolateyInstall}\bin\choco.exe" @args
    }
}

function winget {
    $isDotnet = $false
    $pkgName = ""
    
    foreach ($arg in $args) {
        if ($arg -like "*dotnet*" -or $arg -like "*Microsoft.DotNet*") {
            $isDotnet = $true
            $pkgName = $arg
            break
        }
    }
    
    if ($args[0] -eq "install" -and $isDotnet) {
        $dverCmd = Get-DverPath
        
        if (-not $dverCmd) {
            Write-Host "⚠️  dver is not installed or not in PATH. Running original winget command..." -ForegroundColor Yellow
            & "${env:LOCALAPPDATA}\Microsoft\WindowsApps\winget.exe" @args
            return
        }
        
        Write-Host "🔄 dver: Detected 'winget install $pkgName'. Redirecting to dver..." -ForegroundColor Cyan
        Write-Host "   Installing latest LTS version via dver..." -ForegroundColor Cyan
        & $dverCmd install --lts
    } else {
        & "${env:LOCALAPPDATA}\Microsoft\WindowsApps\winget.exe" @args
    }
}
"#;

    // Check if already installed
    let content = if profile_file.exists() {
        fs::read_to_string(&profile_file)?
    } else {
        String::new()
    };

    if content.contains("dver intercept wrappers") {
        println!(
            "   Updating existing intercept wrappers in {:?}",
            profile_file
        );
        // For Windows, we'll just append new wrappers (PowerShell functions override)
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile_file)?;

    write!(file, "{}", wrapper_code)?;

    println!("   Added intercept wrappers to {:?}", profile_file);

    Ok(())
}

#[cfg(not(windows))]
fn install_windows_intercept() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
