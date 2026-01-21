use crate::utils::{common, downloader};
use std::path::PathBuf;

async fn install_dotnet(
    lts: bool,
    version: Option<String>,
    install_path: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let target_version = if let Some(v) = version {
        // Simple heuristic: if it looks like X.Y.Z, use it directly. Otherwise treat as channel (X.Y, LTS, STS) and resolve.
        if v.split('.').count() >= 3 {
            v
        } else {
            println!("Resolving latest version for channel '{}'...", v);
            downloader::resolve_sdk_version(&v).await?
        }
    } else {
        let channel = if lts { "LTS" } else { "LTS" }; // Default to LTS
        println!("Resolving latest version for channel '{}'...", channel);
        downloader::resolve_sdk_version(channel).await?
    };

    let target_path = if let Some(p) = install_path {
        PathBuf::from(p)
    } else {
        let home = common::get_home_dir().ok_or("Could not determine home directory")?;
        if cfg!(windows) {
            // Standard per-user install on Windows
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
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
        }
    };

    // Confirm to user
    println!(
        "Installing .NET SDK {} to {:?}",
        target_version, target_path
    );

    downloader::download_and_extract(&target_version, &target_path).await?;

    Ok(())
}

pub async fn handle_install(
    lts: bool,
    version: Option<String>,
    install_path: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::utils::downloader;
    use crate::utils::sdk::list_installed_sdks_grouped;

    // 1. Resolve the version we intend to install
    let target_version = if let Some(ref v) = version {
        if v.split('.').count() >= 3 {
            v.clone()
        } else {
            println!("Resolving version for '{}'...", v);
            downloader::resolve_sdk_version(v).await?
        }
    } else {
        let channel = if lts { "LTS" } else { "LTS" };
        println!("Resolving latest version for channel '{}'...", channel);
        downloader::resolve_sdk_version(channel).await?
    };

    // 2. Comprehensive check for existing installations
    println!(
        "Checking if .NET SDK {} is already installed...",
        target_version
    );

    // We don't check `is_dotnet_installed` here because we want to run our comprehensive check
    // regardless of whether `dotnet` is in PATH or not (since we scan standard dirs now).
    match list_installed_sdks_grouped() {
        Ok(sdks_by_location) => {
            let mut found_locations = Vec::new();

            for (location, sdks) in &sdks_by_location {
                for sdk in sdks {
                    if sdk.version == target_version {
                        found_locations.push(location.clone());
                    }
                }
            }

            if !found_locations.is_empty() {
                println!(
                    "✅ .NET SDK version {} is ALREADY installed in the following location(s):",
                    target_version
                );
                for loc in &found_locations {
                    println!("   - {}", loc);
                }
                println!("\nSkipping installation to prevent duplicates.");
                println!("Use 'dver use {}' to select this version.", target_version);
                return Ok(());
            }
        }
        Err(e) => {
            // If listing fails, warn but proceed, or fail safe?
            // Given the user constraint, we should probably warn loudly.
            eprintln!("⚠️  Warning: Failed to scan for existing SDKs: {}", e);
            eprintln!("Proceeding with installation, but duplicates might occur.");
        }
    }

    // 3. Proceed with installation (pass resolved version to avoid re-resolving)
    // We need to modify install_dotnet to take the explicit resolved version or refactor logic.
    // For now, let's keep calling install_dotnet but pass the resolved version as "version".

    // Since install_dotnet re-resolves if it sees a version string, we pass the exact X.Y.Z string
    // which the helper handles as "use directly".
    if let Err(e) = install_dotnet(false, Some(target_version), install_path).await {
        eprintln!("Installation failed: {}", e);
        return Err(e);
    }

    println!("dotnet installation completed successfully.");

    // Auto-configure PATH to ensure the managed location is in PATH
    use crate::commands::consolidate::ensure_path_configured;
    if let Err(e) = ensure_path_configured() {
        eprintln!("⚠️  Note: Could not auto-configure PATH: {}", e);
    }

    // Post-install hint
    println!(
        "\nNOTE: To use the installed version, ensure the installation directory is in your PATH."
    );
    println!("      Or use 'dver use <version>' to configure project-specific version.");
    println!("      Run 'dver doctor' to verify your configuration.");

    Ok(())
}
