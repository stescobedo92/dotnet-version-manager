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
            let output = Command::new("dotnet").arg("--version").output()?;
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("Current dotnet version: {}", version.trim());
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Check if the error is about SDK not found due to global.json
                if stderr.contains("SDK was not found") || stderr.contains("Requested SDK version")
                {
                    eprintln!(
                        "Error: The .NET SDK version specified in global.json is not installed."
                    );
                    eprintln!();

                    // Try to find which global.json is being used
                    if let Ok(current) = std::env::current_dir() {
                        let mut check_dir = Some(current.as_path());
                        while let Some(dir) = check_dir {
                            let global_json_path = dir.join("global.json");
                            if global_json_path.exists() {
                                eprintln!("Found global.json at: {:?}", global_json_path);
                                if let Ok(content) = std::fs::read_to_string(&global_json_path) {
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(&content)
                                    {
                                        if let Some(version) = json
                                            .get("sdk")
                                            .and_then(|s| s.get("version"))
                                            .and_then(|v| v.as_str())
                                        {
                                            eprintln!("Requested SDK version: {}", version);
                                            eprintln!();
                                            eprintln!("You can either:");
                                            eprintln!("  1. Install the requested version:");
                                            eprintln!("     dver install --version {}", version);
                                            eprintln!("  2. Change to an installed version:");
                                            eprintln!("     dver use <installed-version>");
                                            eprintln!(
                                                "     Run 'dver list' to see installed versions"
                                            );
                                        }
                                    }
                                }
                                break;
                            }
                            check_dir = dir.parent();
                        }
                    }
                } else {
                    eprintln!(
                        "Failed to get current dotnet version{}{}",
                        if stderr.trim().is_empty() { "" } else { ": " },
                        stderr.trim()
                    );
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
        Commands::Use {
            version,
            global,
            clear,
        } => {
            commands::use_cmd::handle_use(version.clone(), *global, *clear).await?;
        }
        Commands::Install {
            lts,
            version,
            install_path,
        } => {
            commands::install::handle_install(*lts, version.clone(), install_path.clone()).await?;
        }
        Commands::Uninstall { version, all } => {
            commands::uninstall::handle_uninstall(version.clone(), *all).await?;
        }
        Commands::Doctor => {
            commands::doctor::run_doctor_checks();
        }
        Commands::Setup => {
            if let Err(e) = commands::setup::move_to_top_of_path() {
                eprintln!("Failed to configure PATH: {}", e);
            }
        }
    }

    Ok(())
}
