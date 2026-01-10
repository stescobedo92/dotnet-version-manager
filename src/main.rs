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
                    let mut versions: Vec<String> = sdks
                        .into_iter()
                        .map(|(version, _)| version)
                        .collect();
                    // Already sorted by list_installed_sdks, but dedup to be safe
                    versions.dedup();
                    for v in versions {
                        println!("{}", v);
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

            // If a file exists, keep a simple backup alongside
            if file_path.exists() {
                let backup = file_path.with_extension("json.bak");
                let _ = std::fs::copy(&file_path, &backup);
            }

            let file = File::create(&file_path)?;
            serde_json::to_writer_pretty(file, &json_data)?;
            println!("SDK version set to {} in {:?}", version, file_path);
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
