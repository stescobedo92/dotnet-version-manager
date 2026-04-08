mod cli;
mod commands;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os();
    let _program_name = args.next();

    if args
        .next()
        .as_deref()
        .is_some_and(|arg| arg == "__internal_shim")
    {
        let exit_code = commands::shim::handle_dotnet_shim(args)?;
        std::process::exit(exit_code);
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Current => {
            handle_current_command()?;
        }
        Commands::List => {
            handle_list_command()?;
        }
        Commands::Use {
            version,
            global,
            clear,
        } => {
            commands::use_cmd::handle_use(version.clone(), *global, *clear).await?;
        }
        Commands::Install {
            version,
            version_flag,
            channel,
            lts,
        } => {
            let request = version
                .clone()
                .or_else(|| version_flag.clone())
                .or_else(|| channel.clone());
            commands::install::handle_install(*lts, request).await?;
        }
        Commands::Uninstall {
            version,
            version_flag,
            all,
        } => {
            let request = version.clone().or_else(|| version_flag.clone());
            commands::uninstall::handle_uninstall(request, *all).await?;
        }
        Commands::Doctor => {
            commands::doctor::run_doctor_checks()?;
        }
        Commands::Setup => {
            commands::setup::move_to_top_of_path()?;
        }
    }

    Ok(())
}

fn handle_current_command() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = crate::utils::common::current_working_dir()?;

    if let Some(selection) = crate::utils::sdk::resolve_version_selection(&current_dir)? {
        let sdk = crate::utils::sdk::resolve_managed_sdk(&selection.requested_version)?;
        let output = Command::new(crate::utils::common::managed_dotnet_path(&sdk.root))
            .arg("--version")
            .env("DOTNET_ROOT", &sdk.root)
            .env("DOTNET_MULTILEVEL_LOOKUP", "0")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("Current managed dotnet version: {}", version.trim());
            return Ok(());
        }
    }

    let output = Command::new("dotnet").arg("--version").output()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        println!("Current system dotnet version: {}", version.trim());
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "Failed to determine the active dotnet version: {}",
        stderr.trim()
    )
    .into())
}

fn handle_list_command() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = crate::utils::common::current_working_dir()?;
    let managed_sdks = crate::utils::sdk::list_managed_sdks()?;
    let system_sdks = crate::utils::sdk::list_system_sdks()?;
    let selected_version = crate::utils::sdk::resolve_version_selection(&current_dir)?
        .map(|selection| selection.requested_version);
    let default_version = crate::utils::sdk::get_default_version()?;

    if managed_sdks.is_empty() {
        println!("Managed .NET SDK versions:");
        println!("  none");
        println!("  Tip: run 'dver install 8.0.406' to install one.");
    } else {
        println!("Managed .NET SDK versions:");
        for sdk in managed_sdks {
            let mut markers = Vec::new();
            if selected_version.as_deref() == Some(&sdk.version) {
                markers.push("selected");
            }
            if default_version.as_deref() == Some(&sdk.version) {
                markers.push("default");
            }

            if markers.is_empty() {
                println!("  {}", sdk.version);
            } else {
                println!("  {} [{}]", sdk.version, markers.join(", "));
            }
        }
    }

    println!();
    println!("System/global .NET SDK versions:");
    if system_sdks.is_empty() {
        println!("  none detected");
    } else {
        for sdk in system_sdks {
            println!("  {} ({})", sdk.version, sdk.location.display());
        }
    }

    Ok(())
}
