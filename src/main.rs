mod cli;
mod commands;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use serde_json::json;
use std::fs::File;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Current => {
            let output = Command::new("dotnet")
                .arg("--version")
                .output()?;
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("Current dotnet version: {}", version.trim());
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                
                // Check if the error is about SDK not found due to global.json
                if stderr.contains("SDK was not found") || stderr.contains("Requested SDK version") {
                    eprintln!("Error: The .NET SDK version specified in global.json is not installed.");
                    eprintln!();
                    
                    // Try to find which global.json is being used
                    if let Ok(current) = std::env::current_dir() {
                        let mut check_dir = Some(current.as_path());
                        while let Some(dir) = check_dir {
                            let global_json_path = dir.join("global.json");
                            if global_json_path.exists() {
                                eprintln!("Found global.json at: {:?}", global_json_path);
                                if let Ok(content) = std::fs::read_to_string(&global_json_path) {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                                        if let Some(version) = json.get("sdk").and_then(|s| s.get("version")).and_then(|v| v.as_str()) {
                                            eprintln!("Requested SDK version: {}", version);
                                            eprintln!();
                                            eprintln!("You can either:");
                                            eprintln!("  1. Install the requested version:");
                                            eprintln!("     dver install --version {}", version);
                                            eprintln!("  2. Change to an installed version:");
                                            eprintln!("     dver use <installed-version>");
                                            eprintln!("     Run 'dver list' to see installed versions");
                                        }
                                    }
                                }
                                break;
                            }
                            check_dir = dir.parent();
                        }
                    }
                } else {
                    eprintln!("Failed to get current dotnet version{}{}",
                              if stderr.trim().is_empty() { "" } else { ": " }, stderr.trim());
                }
            }
        }
        Commands::List => {
            use crate::utils::sdk::list_installed_sdks_grouped;
            
            match list_installed_sdks_grouped() {
                Ok(sdks_by_location) => {
                    if sdks_by_location.is_empty() {
                        println!("No .NET SDK versions found.");
                        return Ok(());
                    }
                    
                    // Sort locations: system paths first, then user paths
                    let mut locations: Vec<_> = sdks_by_location.keys().collect();
                    locations.sort_by(|a, b| {
                        // System paths (Program Files, /usr/share) should come first
                        let a_is_system = a.contains("Program Files") || a.contains("/usr/share");
                        let b_is_system = b.contains("Program Files") || b.contains("/usr/share");
                        match (a_is_system, b_is_system) {
                            (true, false) => std::cmp::Ordering::Less,
                            (false, true) => std::cmp::Ordering::Greater,
                            _ => a.cmp(b),
                        }
                    });
                    
                    for location in locations {
                        if let Some(sdks) = sdks_by_location.get(location) {
                            println!("\nVersions found in [{}]", location);
                            for sdk in sdks {
                                println!("  {}", sdk.version);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list SDK versions: {}", e);
                }
            }
        }
        Commands::Use { version } => {
            // First, validate that the requested version is installed
            use crate::utils::sdk::list_installed_sdks_grouped;
            
            let sdks_by_location = list_installed_sdks_grouped()?;
            let mut found_version = false;
            let mut installed_versions = Vec::new();
            
            for sdks in sdks_by_location.values() {
                for sdk in sdks {
                    installed_versions.push(sdk.version.clone());
                    if sdk.version == *version {
                        found_version = true;
                    }
                }
            }
            
            if !found_version {
                eprintln!("Error: .NET SDK version {} is not installed.", version);
                eprintln!("\nInstalled SDK versions:");
                for sdks in sdks_by_location.values() {
                    for sdk in sdks {
                        eprintln!("  {}", sdk.version);
                    }
                }
                eprintln!("\nPlease install the SDK version first using:");
                eprintln!("  dver install --version {}", version);
                return Err("Requested SDK version is not installed".into());
            }
            
            let json_data = json!({
                "sdk": {
                    "version": version
                }
            });

            // Priority 1: Check if global.json exists in HOME directory and update it there
            let home_dir = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .ok()
                .and_then(|h| std::path::PathBuf::from(h).canonicalize().ok());

            let mut file_path = std::env::current_dir()?.join("global.json");
            let mut updated_home = false;

            if let Some(home) = home_dir {
                let home_global = home.join("global.json");
                if home_global.exists() {
                    // Update existing global.json in HOME
                    let backup = home_global.with_extension("json.bak");
                    let _ = std::fs::copy(&home_global, &backup);
                    
                    let file = File::create(&home_global)?;
                    serde_json::to_writer_pretty(file, &json_data)?;
                    println!("Updated existing global.json in HOME directory: {:?}", home_global);
                    println!("SDK version set to {}", version);
                    updated_home = true;
                    file_path = home_global;
                }
            }

            // Priority 2: If no global.json in HOME, check current directory and parents
            if !updated_home {
                // Check if there's already a global.json in parent directories
                let mut found_parent_config = false;
                if let Ok(current) = std::env::current_dir() {
                    let mut check_dir = current.parent();
                    while let Some(dir) = check_dir {
                        let parent_global = dir.join("global.json");
                        if parent_global.exists() {
                            println!("Note: Found global.json in parent directory: {:?}", parent_global);
                            println!("      The new global.json in the current directory will take precedence.");
                            found_parent_config = true;
                            break;
                        }
                        check_dir = dir.parent();
                    }
                }

                // If a file exists in current directory, keep a backup
                if file_path.exists() {
                    let backup = file_path.with_extension("json.bak");
                    let _ = std::fs::copy(&file_path, &backup);
                    println!("Previous global.json backed up to: {:?}", backup);
                }

                let file = File::create(&file_path)?;
                serde_json::to_writer_pretty(file, &json_data)?;
                println!("SDK version set to {} in {:?}", version, file_path);
                
                if !found_parent_config {
                    println!("\nThis global.json will be used by .NET SDK for this directory and all subdirectories.");
                    println!("The SDK searches upward from the current directory until it finds a global.json file.");
                }
            }
        }
        Commands::Install { lts, version, install_path } => {
            commands::install::handle_install(*lts, version.clone(), install_path.clone()).await?;
        }
        Commands::Uninstall { version, all } => {
            commands::uninstall::handle_uninstall(version.clone(), *all).await?;
        }
        Commands::Doctor => {
            commands::doctor::run_doctor_checks();
        }
    }

    Ok(())
}
