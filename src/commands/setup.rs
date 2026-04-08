use crate::utils::common::{ensure_dir, get_shims_dir};
use std::env;
use std::fs;
use std::path::Path;

#[cfg(unix)]
use crate::utils::common::{get_dver_root, get_home_dir};
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
const UNIX_BLOCK_START: &str = "# >>> dver >>>";
#[cfg(unix)]
const UNIX_BLOCK_END: &str = "# <<< dver <<<";

pub fn move_to_top_of_path() -> Result<(), Box<dyn std::error::Error>> {
    ensure_shims_exist()?;

    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;
    if cfg!(windows) {
        configure_windows_path(&shims_dir)?;
    } else {
        configure_unix_path(&shims_dir)?;
    }

    println!("dver setup completed.");
    println!("Restart your shell so the updated PATH takes effect.");
    Ok(())
}

pub fn ensure_shims_exist() -> Result<(), Box<dyn std::error::Error>> {
    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;
    ensure_dir(&shims_dir)?;

    let current_exe = env::current_exe()?;
    if cfg!(windows) {
        write_windows_shim(&current_exe, &shims_dir.join("dotnet.cmd"))?;
    } else {
        write_unix_shim(&current_exe, &shims_dir.join("dotnet"))?;
    }

    Ok(())
}

#[cfg(windows)]
fn configure_windows_path(shims_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu.create_subkey("Environment")?;

    let current_path: String = env_key.get_value("Path").unwrap_or_default();
    let shims_str = shims_dir.to_str().ok_or("Invalid shims path")?;
    let mut entries: Vec<&str> = current_path
        .split(';')
        .filter(|entry| !entry.is_empty())
        .collect();
    entries.retain(|entry| !paths_equal_windows(entry, shims_str));
    entries.insert(0, shims_str);

    env_key.set_value("Path", &entries.join(";"))?;
    println!("Updated the user PATH to include {}.", shims_dir.display());
    Ok(())
}

#[cfg(not(windows))]
fn configure_windows_path(_shims_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(unix)]
fn configure_unix_path(shims_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let rc_file = detect_unix_rc_file()?;
    let dver_root = get_dver_root().ok_or("Could not determine dver root")?;
    let new_block = format!(
        "{UNIX_BLOCK_START}\nexport DVER_ROOT=\"{}\"\nexport PATH=\"{}:$PATH\"\n{UNIX_BLOCK_END}\n",
        dver_root.display(),
        shims_dir.display()
    );

    let existing = if rc_file.exists() {
        fs::read_to_string(&rc_file)?
    } else {
        String::new()
    };

    let updated = upsert_config_block(&existing, &new_block);
    fs::write(&rc_file, updated)?;
    println!("Updated {}.", rc_file.display());
    Ok(())
}

#[cfg(not(unix))]
fn configure_unix_path(_shims_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

pub fn remove_configuration() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(windows) {
        let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;
        remove_windows_path(&shims_dir)?;
    } else {
        remove_unix_path()?;
    }
    Ok(())
}

#[cfg(windows)]
fn remove_windows_path(shims_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu.create_subkey("Environment")?;
    let current_path: String = env_key.get_value("Path").unwrap_or_default();
    let shims_str = shims_dir.to_str().ok_or("Invalid shims path")?;

    let mut entries: Vec<&str> = current_path
        .split(';')
        .filter(|entry| !entry.is_empty())
        .collect();
    entries.retain(|entry| !paths_equal_windows(entry, shims_str));
    env_key.set_value("Path", &entries.join(";"))?;
    Ok(())
}

#[cfg(not(windows))]
fn remove_windows_path(_shims_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(unix)]
fn remove_unix_path() -> Result<(), Box<dyn std::error::Error>> {
    let rc_file = detect_unix_rc_file()?;
    if !rc_file.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(&rc_file)?;
    let updated = remove_config_block(&existing);
    fs::write(&rc_file, updated)?;
    Ok(())
}

#[cfg(not(unix))]
fn remove_unix_path() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(unix)]
fn detect_unix_rc_file() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = get_home_dir().ok_or("Could not determine the home directory")?;
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

    Ok(rc_file)
}

#[cfg(unix)]
fn write_unix_shim(
    dver_executable: &Path,
    shim_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let contents = format!(
        "#!/usr/bin/env sh\nexec {} __internal_shim \"$@\"\n",
        shell_quote(dver_executable)
    );
    fs::write(shim_path, contents)?;
    fs::set_permissions(shim_path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_unix_shim(
    _dver_executable: &Path,
    _shim_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(windows)]
fn write_windows_shim(
    dver_executable: &Path,
    shim_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let contents = format!(
        "@echo off\r\n\"{}\" __internal_shim %*\r\n",
        dver_executable.display()
    );
    fs::write(shim_path, contents)?;
    Ok(())
}

#[cfg(not(windows))]
fn write_windows_shim(
    _dver_executable: &Path,
    _shim_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(windows)]
fn paths_equal_windows(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(unix)]
fn upsert_config_block(existing: &str, block: &str) -> String {
    let without_old_block = remove_config_block(existing);
    if without_old_block.trim().is_empty() {
        return block.to_string();
    }

    format!("{}\n{}", without_old_block.trim_end(), block)
}

#[cfg(unix)]
fn remove_config_block(existing: &str) -> String {
    let mut lines = Vec::new();
    let mut skipping = false;

    for line in existing.lines() {
        if line.trim() == UNIX_BLOCK_START {
            skipping = true;
            continue;
        }

        if line.trim() == UNIX_BLOCK_END {
            skipping = false;
            continue;
        }

        if !skipping {
            lines.push(line);
        }
    }

    let mut result = lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }

    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_dver_block() {
        let old = format!("{UNIX_BLOCK_START}\nold\n{UNIX_BLOCK_END}\n");
        let new = format!("{UNIX_BLOCK_START}\nnew\n{UNIX_BLOCK_END}\n");
        let updated = upsert_config_block(&old, &new);
        assert_eq!(updated, new);
    }

    #[test]
    fn appends_new_dver_block() {
        let existing = "export PATH=\"/usr/bin:$PATH\"\n";
        let new = format!("{UNIX_BLOCK_START}\nnew\n{UNIX_BLOCK_END}\n");
        let updated = upsert_config_block(existing, &new);
        assert!(updated.contains("export PATH=\"/usr/bin:$PATH\""));
        assert!(updated.contains("new"));
    }
}
