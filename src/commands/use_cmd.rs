use crate::utils::common::get_home_dir;
use crate::utils::sdk::list_installed_sdks_grouped;
use serde_json::json;
use std::env;
use std::fs::{self, File};
use std::path::Path;

pub async fn handle_use(
    version_arg: Option<String>,
    global: bool,
    clear: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Handle Clear Action
    if clear {
        return handle_clear(global);
    }

    // 2. Validate Version Argument
    let version = if let Some(v) = version_arg {
        v
    } else {
        eprintln!("Error: Please specify a version to use, or use --clear to reset.");
        return Ok(());
    };

    // 3. Validate Version is Installed and get location info
    let version_info = is_version_installed(&version)?;

    match version_info {
        None => {
            eprintln!("Error: .NET SDK version {} is not installed.", version);
            eprintln!("Run 'dver list' to see installed versions.");
            eprintln!("Run 'dver install --version {}' to install it.", version);
            return Err("Requested SDK version is not installed".into());
        }
        Some(ref info) if !info.is_managed => {
            // Version found but in a non-managed location (e.g., Homebrew, system install)
            println!(
                "⚠️  WARNING: SDK {} found at a non-managed location:",
                version
            );
            println!("   Location: {}", info.location_name);
            println!();
            println!(
                "   This SDK was likely installed via a package manager (Homebrew, apt, etc.)."
            );
            println!(
                "   It may not work correctly if your PATH is not configured for that location."
            );
            println!();
            println!("   Recommended actions:");
            println!("   1. Run 'dver setup' to ensure PATH is correctly configured.");
            println!(
                "   2. Or install via dver: 'dver install --version {}'",
                version
            );
            println!("   3. Or uninstall via package manager and reinstall with dver.");
            println!();
        }
        Some(_) => {
            // Version found in managed location - all good
        }
    }

    // 4. Determine Target Path (Scope)
    let target_path = if global {
        get_home_dir()
            .ok_or("Could not determine HOME directory")?
            .join("global.json")
    } else {
        env::current_dir()?.join("global.json")
    };

    // 5. Write Configuration
    write_global_json(&target_path, &version)?;

    if global {
        println!("✅ Global SDK version set to {} (User Home).", version);
        println!("   Location: {:?}", target_path);
        println!(
            "   Scope: System-wide default for this user (unless overridden by local config)."
        );
    } else {
        println!(
            "✅ Local SDK version set to {} (Current Directory).",
            version
        );
        println!("   Location: {:?}", target_path);
        println!("   Scope: Only this directory and its subdirectories.");
    }

    Ok(())
}

fn handle_clear(global: bool) -> Result<(), Box<dyn std::error::Error>> {
    let target_path = if global {
        get_home_dir()
            .ok_or("Could not determine HOME directory")?
            .join("global.json")
    } else {
        env::current_dir()?.join("global.json")
    };

    if target_path.exists() {
        fs::remove_file(&target_path)?;
        println!("✅ Removed configuration file: {:?}", target_path);
        println!(
            "   .NET will now use the default resolution logic (usually latest installed version)."
        );
    } else {
        println!("ℹ️  No configuration file found at: {:?}", target_path);
    }

    Ok(())
}

fn is_version_installed(
    version: &str,
) -> Result<Option<VersionLocation>, Box<dyn std::error::Error>> {
    let sdks_by_location = list_installed_sdks_grouped()?;

    let home = get_home_dir();
    let managed_path_prefix = home.as_ref().map(|h| {
        if cfg!(windows) {
            h.join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("dotnet")
        } else {
            h.join(".dotnet")
        }
    });

    for (location, sdks) in sdks_by_location.iter() {
        for sdk in sdks {
            if sdk.version == version {
                // Determine if this is a managed location
                let is_managed = managed_path_prefix
                    .as_ref()
                    .map(|mp| sdk.path.starts_with(mp))
                    .unwrap_or(false);

                return Ok(Some(VersionLocation {
                    version: sdk.version.clone(),
                    path: sdk.path.clone(),
                    is_managed,
                    location_name: location.clone(),
                }));
            }
        }
    }
    Ok(None)
}

#[allow(dead_code)]
struct VersionLocation {
    version: String,
    path: std::path::PathBuf,
    is_managed: bool,
    location_name: String,
}

fn write_global_json(path: &Path, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let json_data = json!({
        "sdk": {
            "version": version
        }
    });

    // Backup existing if meant to be overwriting?
    // Usually dver use is explicit intent to overwrite, backing up might be noisy.
    // Let's standard write.

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &json_data)?;
    Ok(())
}
