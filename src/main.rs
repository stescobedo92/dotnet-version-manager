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
                eprintln!("Failed to get current dotnet version{}{}",
                          if stderr.trim().is_empty() { "" } else { ": " }, stderr.trim());
            }
        }
        Commands::List => {
            use crate::utils::sdk::list_installed_sdks;
            
            match list_installed_sdks() {
                Ok(sdks) => {
                    // list_installed_sdks already returns deduplicated and sorted versions
                    for (version, _) in sdks {
                        println!("{}", version);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list SDK versions: {}", e);
                }
            }
        }
        Commands::Use { version } => {
            let json_data = json!({
                "sdk": {
                    "version": version
                }
            });

            // Write to current working directory to follow common dotnet practice
            let file_path = std::env::current_dir()?.join("global.json");

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
