use clap::{Parser, Subcommand};
use std::process::{Command, Stdio};
use serde_json::json;
use std::fs::{File, remove_file};
use std::io::{Write, copy};
use std::path::PathBuf;
use reqwest;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Get current dotnet version
    Current,
    /// List available SDK versions
    List,
    /// Set SDK version
    Use { version: String },
    /// Check if dotnet is installed and install if not
    Install {
        /// Install LTS version
        #[arg(long)]
        lts: bool,
        /// Specific version to install
        #[arg(long)]
        version: Option<String>,
    },
}

fn get_home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn is_dotnet_installed() -> bool {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

async fn download_install_script() -> Result<String, Box<dyn std::error::Error>> {
    let script_url = if cfg!(windows) {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.ps1"
    } else {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.sh"
    };

    let response = reqwest::get(script_url).await?;
    let script_content = response.text().await?;

    let script_name = if cfg!(windows) { "dotnet-install.ps1" } else { "dotnet-install.sh" };
    let mut file = File::create(script_name)?;
    file.write_all(script_content.as_bytes())?;

    if !cfg!(windows) {
        Command::new("chmod")
            .args(&["+x", script_name])
            .output()?;
    }

    Ok(script_name.to_string())
}

async fn install_dotnet(lts: bool, version: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let script_name = download_install_script().await?;

    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("powershell");
        cmd.arg("-ExecutionPolicy").arg("Bypass");
        cmd.arg("-File").arg(&script_name);
        cmd
    } else {
        let mut cmd = Command::new("bash");
        cmd.arg(&script_name);
        cmd
    };

    if lts {
        command.arg("-Channel").arg("LTS");
    } else if let Some(v) = version {
        command.arg("-Version").arg(v);
    }

    let output = command.output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));

    remove_file(script_name)?;

    Ok(())
}

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
                eprintln!("Failed to get current dotnet version");
            }
        }
        Commands::List => {
            let output = Command::new("dotnet")
                .args(["--list-sdks"])
                .output()?;
            if output.status.success() {
                let sdks = String::from_utf8_lossy(&output.stdout);
                for line in sdks.lines() {
                    if let Some(version) = line.split_whitespace().next() {
                        println!("{}", version);
                    }
                }
            } else {
                eprintln!("Failed to list SDK versions");
            }
        }
        Commands::Use { version } => {
            let json_data = json!({
                "sdk": {
                    "version": version
                }
            });

            let home_dir = get_home_dir().ok_or("Unable to determine home directory")?;
            let file_path = home_dir.join("global.json");

            let file = File::create(&file_path)?;
            serde_json::to_writer_pretty(file, &json_data)?;
            println!("SDK version set to {} in {:?}", version, file_path);
        }
        Commands::Install { lts, version } => {
            if is_dotnet_installed() {
                println!("dotnet is already installed on your system.");
                let output = Command::new("dotnet")
                    .arg("--version")
                    .output()?;
                let version = String::from_utf8_lossy(&output.stdout);
                println!("Current version: {}", version.trim());
            } else {
                println!("dotnet is not installed. Installing now...");
                install_dotnet(*lts, version.clone()).await?;
                println!("dotnet installation completed.");
            }
        }
    }

    Ok(())
}