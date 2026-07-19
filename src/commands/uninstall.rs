use crate::commands::setup;
use crate::utils::common::{current_working_dir, get_dver_root, get_versions_dir};
use crate::utils::sdk::{
    clear_default_version, find_nearest_global_json, get_default_version, get_managed_sdk,
    list_managed_sdks, list_system_sdks, read_global_json_version, resolve_managed_sdk,
    resolve_system_sdk, set_default_version,
};
use std::fs;
use std::path::Path;

pub async fn handle_uninstall(
    version: Option<String>,
    all: bool,
    system: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if all {
        return uninstall_all();
    }

    let version = version
        .ok_or("Please provide a version to uninstall, for example: dver uninstall 8.0.406")?;

    if system {
        return uninstall_system_sdk(&version);
    }

    uninstall_managed_sdk(&version)
}

fn uninstall_managed_sdk(version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let sdk = match resolve_managed_sdk(version) {
        Ok(sdk) => sdk,
        Err(managed_error) => {
            // Not managed by dver: guide the user towards --system when the
            // version exists system-wide instead of failing with a dead end.
            if let Ok(system_sdk) = resolve_system_sdk(version) {
                return Err(format!(
                    "SDK {} is not managed by dver, but it is installed system-wide at {}.\nRun 'dver uninstall {} --system' to remove the system copy (may require an elevated/Administrator terminal).",
                    system_sdk.version,
                    system_sdk.location.display(),
                    system_sdk.version
                )
                .into());
            }
            return Err(managed_error);
        }
    };
    let default_version = get_default_version()?;

    fs::remove_dir_all(&sdk.root)?;
    println!("Removed managed .NET SDK {}.", sdk.version);

    if default_version.as_deref() == Some(&sdk.version) {
        // Repoint the default to the newest remaining managed SDK (SDKMAN-style)
        // instead of leaving a dangling selection.
        let remaining = list_managed_sdks()?;
        if let Some(newest) = remaining.first() {
            set_default_version(&newest.version)?;
            println!(
                "Default managed SDK was {}; repointed to {}.",
                sdk.version, newest.version
            );
        } else {
            clear_default_version()?;
            println!("Cleared the default managed SDK because it matched the removed version.");
        }
    }

    if let Ok(system_sdks) = list_system_sdks() {
        if let Some(system) = system_sdks
            .iter()
            .find(|entry| entry.version == sdk.version)
        {
            println!(
                "Note: SDK {} is still installed system-wide at {}. Remove it with 'dver uninstall {} --system' if you no longer want it.",
                system.version,
                system.location.display(),
                system.version
            );
        }
    }

    warn_if_global_json_references(&sdk.version)?;

    Ok(())
}

fn uninstall_system_sdk(version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let sdk = resolve_system_sdk(version)?;
    let sdk_dir = sdk.location.join(&sdk.version);

    if !sdk_dir.exists() {
        return Err(format!(
            "Expected system SDK directory does not exist: {}",
            sdk_dir.display()
        )
        .into());
    }

    ensure_dir_not_locked(&sdk_dir)?;

    let system_sdks = list_system_sdks()?;
    let remaining_in_location = system_sdks
        .iter()
        .filter(|entry| entry.location == sdk.location && entry.version != sdk.version)
        .count();

    if remaining_in_location == 0 {
        println!(
            "Warning: {} is the last SDK under {}. After removal, 'dotnet' there will have no SDK (runtimes are kept).",
            sdk.version,
            sdk.location.display()
        );
    }

    match fs::remove_dir_all(&sdk_dir) {
        Ok(()) => {
            println!(
                "Removed system .NET SDK {} from {}.",
                sdk.version,
                sdk.location.display()
            );

            if get_managed_sdk(&sdk.version)?.is_some() {
                println!(
                    "The dver-managed copy of {} is still installed and remains usable.",
                    sdk.version
                );
            }

            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            #[cfg(windows)]
            {
                // Avoid recursive elevation if we are already the elevated child.
                if std::env::var_os("DVER_ELEVATED").is_none() {
                    return elevate_and_retry(&sdk.version, &sdk_dir);
                }
            }

            Err(format!(
                "Access denied removing {}.\nSystem SDK directories are protected: re-run this command from an elevated (Administrator/sudo) terminal.",
                sdk_dir.display()
            )
            .into())
        }
        Err(error) => Err(format!("Failed to remove {}: {error}", sdk_dir.display()).into()),
    }
}

/// Detects processes that keep the SDK directory locked: anything running
/// from inside it (e.g. Roslyn's VBCSCompiler) or with DLLs loaded from it
/// (e.g. persistent `dotnet` MSBuild server nodes). Tries a graceful
/// `dotnet build-server shutdown`, then stops well-known restartable build
/// daemons, and only fails if something else still holds the directory.
fn ensure_dir_not_locked(sdk_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let lockers = processes_locking_dir(sdk_dir);
    if lockers.is_empty() {
        return Ok(());
    }

    println!("The SDK directory is in use by running processes:");
    for locker in &lockers {
        println!("  {} {} ({})", locker.pid, locker.name, locker.path);
    }
    println!("Attempting a graceful 'dotnet build-server shutdown'...");
    shutdown_dotnet_build_servers();

    let mut remaining = processes_locking_dir(sdk_dir);
    let killable: Vec<_> = remaining
        .iter()
        .filter(|locker| is_restartable_build_process(&locker.name))
        .cloned()
        .collect();

    if !killable.is_empty() {
        println!("Stopping build-server processes still holding the SDK:");
        for locker in &killable {
            println!("  stopping {} {}", locker.pid, locker.name);
            stop_process(locker.pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        remaining = processes_locking_dir(sdk_dir);
    }

    if remaining.is_empty() {
        println!("SDK directory is no longer locked; continuing with the removal.");
        return Ok(());
    }

    let listing = remaining
        .iter()
        .map(|locker| format!("{} {} ({})", locker.pid, locker.name, locker.path))
        .collect::<Vec<_>>()
        .join("\n  ");
    Err(format!(
        "Cannot remove {} because these processes are still using it:\n  {listing}\nClose them (or run 'Stop-Process -Id <PID>' in PowerShell) and retry.",
        sdk_dir.display()
    )
    .into())
}

#[derive(Debug, Clone)]
struct LockingProcess {
    pid: u32,
    name: String,
    path: String,
}

/// Build daemons that are safe to stop: they restart on demand.
fn is_restartable_build_process(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "dotnet" | "msbuild" | "vbcscompiler" | "msbuildtaskhost"
    )
}

#[cfg(windows)]
fn stop_process(pid: u32) {
    use std::process::Command;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

#[cfg(not(windows))]
fn stop_process(pid: u32) {
    use std::process::Command;
    let _ = Command::new("kill").arg(pid.to_string()).output();
}

/// Finds processes whose executable OR loaded modules live under `sdk_dir`.
/// Module inspection matters: MSBuild server nodes run as
/// `C:\Program Files\dotnet\dotnet.exe` but keep SDK DLLs loaded.
#[cfg(windows)]
fn processes_locking_dir(sdk_dir: &Path) -> Vec<LockingProcess> {
    use std::process::Command;

    let pattern = format!("{}\\*", sdk_dir.display());
    let script = format!(
        r#"Get-Process | ForEach-Object {{
            $p = $_
            try {{
                $inside = $p.Path -like '{pattern}'
                if (-not $inside) {{
                    $inside = [bool]($p.Modules | Where-Object {{ $_.FileName -like '{pattern}' }} | Select-Object -First 1)
                }}
                if ($inside) {{ "$($p.Id)|$($p.ProcessName)|$($p.Path)" }}
            }} catch {{}}
        }}"#
    );

    let Ok(output) = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&script)
        .output()
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let pid = parts.next()?.trim().parse::<u32>().ok()?;
            let name = parts.next()?.trim().to_string();
            let path = parts.next().unwrap_or("").trim().to_string();
            Some(LockingProcess { pid, name, path })
        })
        .collect()
}

#[cfg(not(windows))]
fn processes_locking_dir(_sdk_dir: &Path) -> Vec<LockingProcess> {
    Vec::new()
}

/// Best-effort: stops MSBuild/Roslyn/Razor build servers so their binaries
/// stop locking SDK directories. Failures are ignored on purpose.
fn shutdown_dotnet_build_servers() {
    use std::process::Command;

    if let Some(dotnet) = crate::utils::sdk::find_system_dotnet_on_path() {
        let _ = Command::new(dotnet)
            .args(["build-server", "shutdown"])
            .output();
    }
}

/// Relaunches `dver uninstall <version> --system` elevated through UAC and
/// verifies the result by checking whether the SDK directory is gone.
#[cfg(windows)]
fn elevate_and_retry(version: &str, sdk_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    println!(
        "Access to {} requires Administrator rights.",
        sdk_dir.display()
    );
    println!("Requesting elevation: accept the Windows UAC prompt to continue...");

    let exe = std::env::current_exe()?;
    let script = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList 'uninstall','{}','--system' -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
        exe.display(),
        version
    );

    let status = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&script)
        .env("DVER_ELEVATED", "1")
        .status();

    if !sdk_dir.exists() {
        println!("Removed system .NET SDK {version} (elevated).");
        return Ok(());
    }

    let lockers = processes_locking_dir(sdk_dir);
    if !lockers.is_empty() {
        let listing = lockers
            .iter()
            .map(|locker| format!("{} {} ({})", locker.pid, locker.name, locker.path))
            .collect::<Vec<_>>()
            .join("\n  ");
        return Err(format!(
            "Removal failed because these processes are running from the SDK directory:\n  {listing}\nClose them (or 'Stop-Process -Id <PID>') and retry: dver uninstall {version} --system",
        )
        .into());
    }

    match status {
        Ok(status) if !status.success() => Err(format!(
            "Elevation was cancelled or the elevated removal failed. {} still exists.\nYou can also run this manually from an Administrator terminal: dver uninstall {version} --system",
            sdk_dir.display()
        )
        .into()),
        _ => Err(format!(
            "The elevated process finished but {} still exists. Check whether another process is locking the directory.",
            sdk_dir.display()
        )
        .into()),
    }
}

fn warn_if_global_json_references(removed: &str) -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = current_working_dir()?;
    if let Some(global_json) = find_nearest_global_json(&current_dir) {
        if let Ok(Some(version)) = read_global_json_version(&global_json) {
            if version == removed {
                println!(
                    "Warning: {} still requests SDK {}. Update it with 'dver use <version>' or remove it with 'dver use --clear'.",
                    global_json.display(),
                    removed
                );
            }
        }
    }
    Ok(())
}

fn uninstall_all() -> Result<(), Box<dyn std::error::Error>> {
    let versions_dir =
        get_versions_dir().ok_or("Could not determine managed versions directory")?;
    let managed_sdks = list_managed_sdks()?;

    if managed_sdks.is_empty() {
        println!("No managed .NET SDKs are currently installed.");
    } else if versions_dir.exists() {
        fs::remove_dir_all(&versions_dir)?;
        println!("Removed all managed .NET SDK versions.");
    }

    clear_default_version()?;
    setup::remove_configuration()?;

    if let Some(root) = get_dver_root() {
        let shims_dir: std::path::PathBuf = root.join("bin");
        remove_dir_if_exists(&shims_dir)?;
    }

    println!("Removed dver-managed SDK state. System or package-manager .NET installations were left untouched.");
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}
