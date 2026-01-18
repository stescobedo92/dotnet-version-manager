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
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

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
