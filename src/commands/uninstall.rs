use std::fs::{self, remove_dir_all};
use std::io;
use std::path::PathBuf;

pub async fn handle_uninstall(
    version: Option<String>,
    all: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::utils::sdk::list_installed_sdks_grouped;
    use std::path::{Path, PathBuf};

    // We fetch grouped SDKs to find ALL instances of the version
    // We fetch grouped SDKs to find ALL instances of the version
    let sdks_by_location = list_installed_sdks_grouped().unwrap_or_default();

    if sdks_by_location.is_empty() && !all {
        println!("No .NET SDK versions found.");
        return Ok(());
    }

    let mut targets: Vec<(String, PathBuf)> = Vec::new();

    if all {
        // 0. Use Package Managers first (Homebrew, Snap) to avoid breaking their state
        use crate::utils::package_manager::try_uninstall_all;
        try_uninstall_all();

        // Collect ALL versions from ALL locations (SDKs)
        for sdks in sdks_by_location.values() {
            for sdk in sdks {
                targets.push((sdk.version.clone(), sdk.path.clone()));
            }
        }

        // Also collect ALL Runtimes
        use crate::utils::sdk::list_installed_runtimes_grouped;
        let runtimes_by_location = list_installed_runtimes_grouped().unwrap_or_default();
        for runtimes in runtimes_by_location.values() {
            for runtime in runtimes {
                targets.push((format!("Runtime {}", runtime.version), runtime.path.clone()));
            }
        }
        // Also check for binaries via 'whereis' or 'where'
        let bin_cmd = if cfg!(windows) { "where" } else { "whereis" };
        if let Ok(output) = std::process::Command::new(bin_cmd).arg("dotnet").output() {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if cfg!(windows) {
                    // Windows 'where': one path per line
                    for line in out_str.lines() {
                        let p = PathBuf::from(line.trim());
                        if p.exists() {
                            targets.push(("dotnet.exe".to_string(), p));
                        }
                    }
                } else {
                    // Unix 'whereis': "dotnet: /usr/bin/dotnet /path/to/man ..."
                    if let Some((_, paths)) = out_str.split_once(':') {
                        for path_str in paths.split_whitespace() {
                            let p = PathBuf::from(path_str);
                            if p.exists() {
                                targets.push(("dotnet binary/resource".to_string(), p));
                            }
                        }
                    }
                }
            }
        }
    } else if let Some(v) = version {
        let pattern = v;
        // Search in all locations
        for sdks in sdks_by_location.values() {
            for sdk in sdks {
                // Exact match
                if sdk.version == pattern {
                    targets.push((sdk.version.clone(), sdk.path.clone()));
                }
                // Prefix match (if valid partial version provided)
                else if sdk.version.starts_with(&pattern) {
                    // Only add if we haven't added this exact path yet (though we reset logic below)
                    targets.push((sdk.version.clone(), sdk.path.clone()));
                }
            }
        }

        if targets.is_empty() {
            println!("No matching SDK versions found for '{}'.", pattern);
            return Ok(());
        }

        // If generic prefix (like "8") matched multiple DIFFERENT versions (8.0.100 vs 8.0.200),
        // we should probably ask unless it's exact match?
        // User asked to "eliminar la version pasada por parametro".
        // If they pass "8.0.401", we delete all instances of 8.0.401.
        // If they pass "8", we might delete all 8.x?

        // Let's filter to exact match if there is at least one exact match.
        let exact_matches: Vec<_> = targets
            .iter()
            .filter(|(ver, _)| ver == &pattern)
            .cloned()
            .collect();
        if !exact_matches.is_empty() {
            targets = exact_matches;
        } else {
            // Ambiguous or prefix match
            if targets.len() > 1 {
                // Check if they are actually different VERSIONS or just same version in different places
                let distinct_versions: Vec<_> = targets
                    .iter()
                    .map(|(v, _)| v)
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                if distinct_versions.len() > 1 {
                    println!("Found multiple versions matching '{}':", pattern);
                    for v in &distinct_versions {
                        println!(" - {}", v);
                    }
                    println!("Please specify the full version to uninstall.");
                    return Ok(());
                }
            }
        }
    } else {
        eprintln!("Please provide a version or --all to uninstall.");
        return Ok(());
    }

    if targets.is_empty() {
        if !all {
            return Ok(());
        }
    }

    // Deduplicate targets by path
    targets.sort_by(|(_, a), (_, b)| a.cmp(b));
    targets.dedup_by(|(_, a), (_, b)| a == b);

    println!(
        "Found {} installation(s)/artifact(s) to remove.",
        targets.len()
    );

    let mut successfully_removed_any = false;

    // Check existence robustly (handling broken symlinks)
    fn check_exists(p: &PathBuf) -> bool {
        p.exists() || std::fs::symlink_metadata(p).is_ok()
    }

    for (ver, path) in targets {
        if check_exists(&path) {
            println!("Removing {} from {:?}", ver, path);

            // Check if it is a directory or file (use symlink_metadata to be safe for broken links)
            let is_dir = if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                metadata.is_dir()
            } else {
                path.is_dir() // Fallback
            };

            let removal_result = if is_dir {
                remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };

            match removal_result {
                Ok(_) => {
                    println!("✅ Succesfully removed {}", ver);
                    successfully_removed_any = true;
                }
                Err(e) => {
                    eprintln!("❌ Failed to remove from {:?}: {}", path, e);

                    if e.kind() == io::ErrorKind::PermissionDenied {
                        eprintln!("\n⚠️  PERMISSION DENIED");
                        if cfg!(unix) {
                            println!("   Attempting to escalate privileges via sudo...");
                            let status = std::process::Command::new("sudo")
                                .arg("rm")
                                .arg("-rf")
                                .arg(&path)
                                .status();

                            match status {
                                Ok(s) if s.success() => {
                                    println!("✅ Succesfully removed {} via sudo", ver);
                                    successfully_removed_any = true;
                                }
                                _ => {
                                    println!("   Failed to remove automatically.");
                                    println!("   To force removal, manually run:");
                                    println!("   sudo rm -rf {:?}", path);
                                }
                            }
                        } else {
                            println!("   Try running 'dver' as Administrator.");
                        }
                        println!("");
                    }
                }
            }
        } else {
            println!("Artifact for {} not found at {:?}", ver, path);
        }
    }

    // Comprehensive Cleanup if --all is requested
    if all {
        use crate::commands::setup::remove_configuration;
        use crate::utils::common::get_home_dir;

        println!("\n🧹 Performing deep cleanup of managed artifacts...");

        // 0. Remove PATH configuration
        if let Err(e) = remove_configuration() {
            eprintln!("⚠️  Failed to cleanup PATH configuration: {}", e);
        }

        // 1. Remove User Global JSON
        if let Some(home) = get_home_dir() {
            let global_json = home.join("global.json");
            if global_json.exists() {
                if let Err(e) = std::fs::remove_file(&global_json) {
                    eprintln!("⚠️  Failed to remove global configuration: {}", e);
                } else {
                    println!("✅ Removed global configuration (~/global.json)");
                }
            }
        }

        // 2. Wipe the entire managed directory (to remove runtimes, host, packs, etc.)
        if let Some(home) = get_home_dir() {
            let managed_dir = if cfg!(windows) {
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
            };

            if managed_dir.exists() {
                println!("⚠️  Removing entire managed directory: {:?}", managed_dir);
                match remove_dir_all(&managed_dir) {
                    Ok(_) => println!("✅ Managed .dotnet directory completely removed."),
                    Err(e) => eprintln!("❌ Failed to remove managed directory: {}", e),
                }
            }
        }
    } else if successfully_removed_any {
        // Optional: Could verify if active version in global.json was removed
    }

    Ok(())
}
