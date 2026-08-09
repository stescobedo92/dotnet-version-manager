use crate::utils::common::{ensure_dir, get_shims_dir};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
use crate::utils::common::{get_dver_root, get_home_dir};
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(windows)]
const WINDOWS_BLOCK_START: &str = "# >>> dver >>>";
#[cfg(windows)]
const WINDOWS_BLOCK_END: &str = "# <<< dver <<<";
#[cfg(unix)]
const UNIX_BLOCK_START: &str = "# >>> dver >>>";
#[cfg(unix)]
const UNIX_BLOCK_END: &str = "# <<< dver <<<";

pub fn move_to_top_of_path() -> Result<(), Box<dyn std::error::Error>> {
    print_setup_header();
    let progress = ProgressBar::new(3);
    progress.set_style(
        ProgressStyle::with_template("{spinner:.cyan} [{bar:24.cyan/blue}] {pos}/{len} {msg}")
            .expect("valid setup progress template")
            .progress_chars("##-"),
    );
    progress.enable_steady_tick(Duration::from_millis(80));

    progress.set_position(1);
    progress.set_message("Preparing shims");
    ensure_shims_exist()?;

    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;
    progress.set_position(2);
    progress.set_message("Configuring shell integration");

    let setup_report = if cfg!(windows) {
        configure_windows_path(&shims_dir)?
    } else {
        configure_unix_path(&shims_dir)?
    };

    progress.set_position(3);
    progress.set_message("Finalizing setup");
    progress.finish_and_clear();

    print_setup_report(&shims_dir, &setup_report);

    Ok(())
}

fn print_setup_header() {
    println!("{}", setup_color("== dver setup ==", SetupStyle::Title));
    println!(
        "{}",
        setup_color(
            "Creating the dotnet shim and wiring your shell to the dver-managed SDKs.",
            SetupStyle::Dim
        )
    );
    println!();
}

fn print_setup_report(shims_dir: &Path, report: &SetupReport) {
    println!(
        "{} {}",
        setup_color("[OK]", SetupStyle::Ok),
        setup_color("Setup completed", SetupStyle::Title)
    );
    println!(
        "  {}",
        setup_color(
            format!("shim directory: {}", shims_dir.display()),
            SetupStyle::Info
        )
    );
    println!(
        "  {}",
        setup_color(
            format!(
                "user PATH updated: {}",
                if report.path_updated { "yes" } else { "no" }
            ),
            SetupStyle::Info
        )
    );

    if !report.updated_profiles.is_empty() {
        println!();
        println!("{}", setup_color("Updated profiles", SetupStyle::Section));
        for profile in &report.updated_profiles {
            println!(
                "  {} {}",
                setup_color("•", SetupStyle::Ok),
                setup_color(profile.display().to_string(), SetupStyle::Info)
            );
        }
    }

    if !report.warnings.is_empty() {
        println!();
        println!("{}", setup_color("Warnings", SetupStyle::Warn));
        for warning in &report.warnings {
            println!(
                "  {} {}",
                setup_color("•", SetupStyle::Warn),
                setup_color(warning, SetupStyle::Dim)
            );
        }
    }

    println!();
    println!("{}", setup_color("Next steps", SetupStyle::Section));
    if cfg!(windows) {
        println!(
            "  {}",
            setup_color(
                "Open a brand-new PowerShell tab and run `Get-Command dotnet`.",
                SetupStyle::Info
            )
        );
        println!(
            "  {}",
            setup_color(
                "Then verify with `dotnet --version` inside the folder where you ran `dver use ...`.",
                SetupStyle::Info
            )
        );
    } else {
        println!(
            "  {}",
            setup_color(
                "Open a new terminal session so your updated shell profile is loaded.",
                SetupStyle::Info
            )
        );
        println!(
            "  {}",
            setup_color("Then verify with `dotnet --version`.", SetupStyle::Info)
        );
    }
}

pub fn remove_setup() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", setup_color("== dver setup --remove ==", SetupStyle::Title));
    println!(
        "{}",
        setup_color(
            "Undoing PATH entry, shell profile hook, and shim directory created by `dver setup`.",
            SetupStyle::Dim
        )
    );
    println!();

    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;

    remove_configuration()?;

    let shims_removed = if shims_dir.exists() {
        match fs::remove_dir_all(&shims_dir) {
            Ok(()) => true,
            Err(error) if is_locked_file_error(&error) => {
                return Err(format!(
                    "Could not remove shim directory {} because a file inside is locked \
                     (often a 'dotnet' shim still in use by another shell). \
                     Close any open terminals or processes using dver, then re-run \
                     'dver setup --remove'. Underlying error: {error}",
                    shims_dir.display()
                )
                .into());
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        false
    };

    println!(
        "{} {}",
        setup_color("[OK]", SetupStyle::Ok),
        setup_color("Setup configuration removed", SetupStyle::Title)
    );
    println!(
        "  {}",
        setup_color(
            format!(
                "shim directory: {} ({})",
                shims_dir.display(),
                if shims_removed { "removed" } else { "not present" }
            ),
            SetupStyle::Info
        )
    );
    println!(
        "  {}",
        setup_color(
            "Managed SDKs and default-version selection were left untouched.",
            SetupStyle::Dim
        )
    );
    println!();
    println!("{}", setup_color("Next steps", SetupStyle::Section));
    if cfg!(windows) {
        println!(
            "  {}",
            setup_color(
                "Open a fresh PowerShell tab so the updated PATH and profile take effect.",
                SetupStyle::Info
            )
        );
    } else {
        println!(
            "  {}",
            setup_color(
                "Open a new terminal session so the updated shell profile is loaded.",
                SetupStyle::Info
            )
        );
    }

    Ok(())
}

pub fn ensure_shims_exist() -> Result<(), Box<dyn std::error::Error>> {
    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;
    ensure_dir(&shims_dir)?;

    let current_exe = env::current_exe()?;
    if cfg!(windows) {
        write_windows_shim(&current_exe, &shims_dir.join("dotnet.cmd"))?;
        write_windows_exe_shim(&current_exe, &shims_dir.join("dotnet.exe"))?;
    } else {
        write_unix_shim(&current_exe, &shims_dir.join("dotnet"))?;
    }

    Ok(())
}

#[cfg(windows)]
fn configure_windows_path(shims_dir: &Path) -> Result<SetupReport, Box<dyn std::error::Error>> {
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
    let mut report = SetupReport {
        path_updated: true,
        updated_profiles: Vec::new(),
        warnings: Vec::new(),
    };
    configure_windows_powershell_profile(shims_dir, &mut report)?;
    Ok(report)
}

#[cfg(not(windows))]
fn configure_windows_path(_shims_dir: &Path) -> Result<SetupReport, Box<dyn std::error::Error>> {
    Ok(SetupReport::default())
}

#[cfg(unix)]
fn configure_unix_path(shims_dir: &Path) -> Result<SetupReport, Box<dyn std::error::Error>> {
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
    Ok(SetupReport {
        path_updated: true,
        updated_profiles: vec![rc_file],
        warnings: Vec::new(),
    })
}

#[cfg(not(unix))]
fn configure_unix_path(_shims_dir: &Path) -> Result<SetupReport, Box<dyn std::error::Error>> {
    Ok(SetupReport::default())
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
    remove_windows_powershell_profile()?;
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
fn write_windows_exe_shim(
    dver_executable: &Path,
    shim_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::copy(dver_executable, shim_path)?;
    Ok(())
}

#[cfg(not(windows))]
fn write_windows_exe_shim(
    _dver_executable: &Path,
    _shim_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(windows)]
fn paths_equal_windows(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

#[cfg(windows)]
pub fn windows_powershell_profile_paths() -> Vec<PathBuf> {
    let candidates = ["pwsh", "powershell"];
    let mut profiles = Vec::new();

    for shell in candidates {
        let output = std::process::Command::new(shell)
            .arg("-NoProfile")
            .arg("-Command")
            .arg(
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
                 Write-Output $PROFILE.CurrentUserCurrentHost",
            )
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let profile = stdout.trim().trim_start_matches('\u{feff}');
                if !profile.is_empty() {
                    let profile_path = PathBuf::from(profile);
                    if !profiles.iter().any(|existing| existing == &profile_path) {
                        profiles.push(profile_path);
                    }
                }
            }
        }
    }

    if let Some(user_profile) = env::var_os("USERPROFILE") {
        let user_profile = PathBuf::from(user_profile);
        let fallbacks = [
            user_profile
                .join("Documents")
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
            user_profile
                .join("Documents")
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ];

        for fallback in fallbacks {
            if !profiles.iter().any(|existing| existing == &fallback) {
                profiles.push(fallback);
            }
        }
    }

    profiles
}

#[cfg(windows)]
pub fn has_windows_profile_hook(shims_dir: &Path) -> bool {
    windows_powershell_profile_paths()
        .into_iter()
        .any(|profile_path| {
            let Ok(contents) = fs::read_to_string(profile_path) else {
                return false;
            };

            contents.contains(WINDOWS_BLOCK_START)
                && contents.contains(WINDOWS_BLOCK_END)
                && contents.contains(&shims_dir.display().to_string())
        })
}

#[cfg(windows)]
fn configure_windows_powershell_profile(
    shims_dir: &Path,
    report: &mut SetupReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let shim_path = shims_dir.join("dotnet.exe");
    let block = format!(
        "{WINDOWS_BLOCK_START}\nif ($env:PATH -notlike \"*{shim_dir}*\") {{\n    $env:PATH = \"{shim_dir};$env:PATH\"\n}}\nfunction global:dotnet {{\n    & '{shim_path}' @Args\n}}\n{WINDOWS_BLOCK_END}\n",
        shim_dir = shims_dir.display().to_string().replace('\"', "`\""),
        shim_path = shim_path.display().to_string().replace('\'', "''")
    );

    let mut wrote_any_profile = false;
    for profile_path in windows_powershell_profile_paths() {
        let write_result: Result<(), Box<dyn std::error::Error>> = (|| {
            if let Some(parent) = profile_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let existing = if profile_path.exists() {
                fs::read_to_string(&profile_path)?
            } else {
                String::new()
            };

            let updated = upsert_windows_config_block(&existing, &block);
            fs::write(&profile_path, updated)?;
            Ok(())
        })();

        match write_result {
            Ok(_) => {
                wrote_any_profile = true;
                report.updated_profiles.push(profile_path);
            }
            Err(error) => {
                report.warnings.push(format!(
                    "could not update PowerShell profile {}: {}",
                    profile_path.display(),
                    error
                ));
            }
        }
    }

    if !wrote_any_profile {
        return Err("Could not update any PowerShell profile with the dver hook.".into());
    }

    Ok(())
}

#[derive(Default)]
struct SetupReport {
    path_updated: bool,
    updated_profiles: Vec<PathBuf>,
    warnings: Vec<String>,
}

#[derive(Clone, Copy)]
enum SetupStyle {
    Title,
    Section,
    Ok,
    Warn,
    Info,
    Dim,
}

fn setup_color(value: impl AsRef<str>, style: SetupStyle) -> String {
    let code = match style {
        SetupStyle::Title => "1;96",
        SetupStyle::Section => "1;97",
        SetupStyle::Ok => "32",
        SetupStyle::Warn => "33",
        SetupStyle::Info => "37",
        SetupStyle::Dim => "90",
    };

    format!("\x1b[{code}m{}\x1b[0m", value.as_ref())
}

#[cfg(windows)]
fn remove_windows_powershell_profile() -> Result<(), Box<dyn std::error::Error>> {
    for profile_path in windows_powershell_profile_paths() {
        if !profile_path.exists() {
            continue;
        }

        let remove_result: Result<(), Box<dyn std::error::Error>> = (|| {
            let existing = fs::read_to_string(&profile_path)?;
            let updated = remove_windows_config_block(&existing);
            fs::write(&profile_path, updated)?;
            Ok(())
        })();

        if let Err(error) = remove_result {
            eprintln!(
                "Warning: could not clean PowerShell profile {}: {}",
                profile_path.display(),
                error
            );
        }
    }

    Ok(())
}

#[cfg(windows)]
fn upsert_windows_config_block(existing: &str, block: &str) -> String {
    let without_old_block = remove_windows_config_block(existing);
    if without_old_block.trim().is_empty() {
        return block.to_string();
    }

    format!("{}\n{}", without_old_block.trim_end(), block)
}

#[cfg(windows)]
fn remove_windows_config_block(existing: &str) -> String {
    let mut lines = Vec::new();
    let mut skipping = false;

    for line in existing.lines() {
        if line.trim() == WINDOWS_BLOCK_START {
            skipping = true;
            continue;
        }

        if line.trim() == WINDOWS_BLOCK_END {
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

fn is_locked_file_error(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }

    #[cfg(windows)]
    {
        // ERROR_ACCESS_DENIED = 5, ERROR_SHARING_VIOLATION = 32, ERROR_LOCK_VIOLATION = 33
        if let Some(code) = error.raw_os_error() {
            return matches!(code, 5 | 32 | 33);
        }
    }

    false
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
