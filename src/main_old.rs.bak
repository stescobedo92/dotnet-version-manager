use clap::{Parser, Subcommand};
use std::process::Command;
use serde_json::json;
use std::fs::{self, File, remove_file, remove_dir_all};
use std::path::{Path, PathBuf};
use reqwest::{self, header};

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
        /// The path to install the SDK to
        #[arg(long)]
        install_path: Option<String>,
    },
    Uninstall {
        /// Version to uninstall. Can be a full version like 8.0.406 or a major version like 8
        version: Option<String>,
        /// Remove all installed SDK versions managed by dotnet
        #[arg(long)]
        all: bool,
    },
    /// Check for common issues
    Doctor,
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

fn list_installed_sdks() -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    let output = Command::new("dotnet")
        .args(["--list-sdks"])
        .output()?;
    if !output.status.success() {
        return Err("Failed to list SDKs".into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sdks = Vec::new();
    for line in stdout.lines() {
        // Expected format: "8.0.406 [C:\\Program Files\\dotnet\\sdk]"
        if let Some((ver_part, path_part)) = line.split_once('[') {
            let version = ver_part.trim().split_whitespace().next().unwrap_or("").to_string();
            let base = path_part.trim().trim_end_matches(']').trim();
            if version.is_empty() || base.is_empty() { continue; }
            let mut pb = PathBuf::from(base);
            pb.push(&version);
            sdks.push((version, pb));
        }
    }
    Ok(sdks)
}

async fn download_install_script() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let script_url = if cfg!(windows) {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.ps1"
    } else {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.sh"
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client
        .get(script_url)
        .header(header::USER_AGENT, "dver/0.1 (https://github.com/stescobedo92/dotnet-version-manager)")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Failed to download installer script: HTTP {}", response.status()).into());
    }

    let script_content = response.bytes().await?;

    let mut file_path = std::env::temp_dir();
    let script_name = if cfg!(windows) { "dotnet-install.ps1" } else { "dotnet-install.sh" };
    // Make filename unique per process
    let unique = format!("{}_{}", script_name, std::process::id());
    file_path.push(unique);

    let mut file = File::create(&file_path)?;
    std::io::Write::write_all(&mut file, &script_content)?;

    if !cfg!(windows) {
        // Set executable bit without spawning a process
        let mut perms = fs::metadata(&file_path)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&file_path, perms)?;
        }
    }

    Ok(file_path)
}

async fn install_dotnet(lts: bool, version: Option<String>, install_path: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let script_path = download_install_script().await?;

    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("powershell");
        cmd.arg("-NoLogo").arg("-NoProfile").arg("-NonInteractive");
        cmd.arg("-ExecutionPolicy").arg("Bypass");
        cmd.arg("-File").arg(&script_path);
        cmd
    } else {
        let mut cmd = Command::new("bash");
        cmd.arg(&script_path);
        cmd
    };

    if lts {
        command.arg("-Channel").arg("LTS");
    } else if let Some(v) = version {
        command.arg("-Version").arg(v);
    }

    if let Some(path) = install_path {
        command.arg("-InstallDir").arg(path);
    }

    let output = command.output()?;
    // Ensure cleanup of temp file
    let _ = remove_file(&script_path);

    if !output.status.success() {
        eprintln!("dotnet-install script failed with status: {:?}", output.status.code());
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() { eprintln!("{}", stderr.trim()); }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() { eprintln!("{}", stdout.trim()); }
        return Err("dotnet installation failed".into());
    }

    println!("{}", String::from_utf8_lossy(&output.stdout));

    Ok(())
}

fn run_doctor_checks() {
    println!("Checking for common issues...");

    // 1. Check if dotnet is installed
    if is_dotnet_installed() {
        println!("✅ dotnet command is available in your PATH.");
    } else {
        println!("❌ dotnet command not found. Please ensure .NET is installed and the installation directory is in your PATH.");
        // Don't proceed with other checks if dotnet isn't even installed.
        return;
    }

    // 2. Check if the default dotnet install directory is in PATH
    if let Some(home_dir) = get_home_dir() {
        let dotnet_dir = home_dir.join(".dotnet");
        if let Ok(path_var) = std::env::var("PATH") {
            if path_var.split(':').any(|p| Path::new(p) == dotnet_dir) {
                println!("✅ .NET SDK installation directory is in your PATH.");
            } else {
                println!("⚠️ .NET SDK installation directory (~/.dotnet) might not be in your PATH.");
                println!("   Consider adding it to ensure the 'dotnet' command is available everywhere.");
            }
        }
    }
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
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Failed to get current dotnet version{}{}",
                          if stderr.trim().is_empty() { "" } else { ": " }, stderr.trim());
            }
        }
        Commands::List => {
            let output = Command::new("dotnet")
                .args(["--list-sdks"])
                .output()?;
            if output.status.success() {
                let sdks = String::from_utf8_lossy(&output.stdout);
                let mut versions: Vec<String> = sdks
                    .lines()
                    .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
                    .collect();
                versions.sort();
                versions.dedup();
                for v in versions {
                    println!("{}", v);
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

            // Write to current working directory to follow common dotnet practice
            let file_path = std::env::current_dir()?.join("global.json");

            // If a file exists, keep a simple backup alongside
            if file_path.exists() {
                let backup = file_path.with_extension("json.bak");
                let _ = fs::copy(&file_path, &backup);
            }

            let file = File::create(&file_path)?;
            serde_json::to_writer_pretty(file, &json_data)?;
            println!("SDK version set to {} in {:?}", version, file_path);
        }
        Commands::Install { lts, version, install_path } => {
            if is_dotnet_installed() {
                println!("dotnet is already installed on your system.");
                let output = Command::new("dotnet")
                    .arg("--version")
                    .output()?;
                let version = String::from_utf8_lossy(&output.stdout);
                println!("Current version: {}", version.trim());
            } else {
                println!("dotnet is not installed. Installing now...");
                if let Err(e) = install_dotnet(*lts, version.clone(), install_path.clone()).await {
                    eprintln!("Installation failed: {}", e);
                    return Err(e);
                }
                println!("dotnet installation completed.");
            }
        }
        Commands::Uninstall { version, all } => {
            let sdks = list_installed_sdks()?;
            // Determine sdk root(s) from listed entries to avoid deleting outside
            let mut roots: Vec<PathBuf> = sdks
                .iter()
                .filter_map(|(_, p)| p.parent().map(|pp| pp.to_path_buf()))
                .collect();
            roots.sort();
            roots.dedup();

            let targets: Vec<(String, PathBuf)> = if *all {
                sdks
            } else if let Some(v) = version {
                if v.contains('.') {
                    sdks.into_iter().filter(|(ver, _)| ver == v).collect()
                } else {
                    let prefix = format!("{}.", v);
                    sdks.into_iter().filter(|(ver, _)| ver.starts_with(&prefix)).collect()
                }
            } else {
                eprintln!("Please provide a version or --all to uninstall.");
                Vec::new()
            };

            if targets.is_empty() {
                println!("No matching SDK versions found.");
            } else {
                for (ver, path) in targets {
                    // Safety: ensure the path is under one of the roots
                    let is_under_root = roots.iter().any(|r| path.starts_with(r));
                    if !is_under_root {
                        eprintln!("Skipping {}: path {:?} is outside known SDK roots", ver, path);
                        continue;
                    }
                    if path.exists() {
                        match remove_dir_all(&path) {
                            Ok(_) => println!("Removed {}", ver),
                            Err(e) => eprintln!("Failed to remove {}: {}", ver, e),
                        }
                    } else {
                        println!("Directory for {} not found", ver);
                    }
                }
            }
        }
        Commands::Doctor => {
            run_doctor_checks();
        }
    }

    Ok(())
}
