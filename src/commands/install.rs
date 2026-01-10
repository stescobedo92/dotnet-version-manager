use std::path::PathBuf;
use std::fs::{self, File, remove_file};
use std::process::Command;
use reqwest::header;

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
    
    // Fix: Put the process ID before the extension to maintain proper file extension
    let script_name = if cfg!(windows) {
        format!("dotnet-install_{}.ps1", std::process::id())
    } else {
        format!("dotnet-install_{}.sh", std::process::id())
    };
    file_path.push(script_name);

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

pub async fn handle_install(lts: bool, version: Option<String>, install_path: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    use crate::utils::sdk::{is_dotnet_installed, find_matching_versions};
    
    // Check if a specific version is requested and if it's already installed
    if let Some(ref v) = version {
        // Only check if dotnet is available before trying to list SDKs
        if is_dotnet_installed() {
            match find_matching_versions(v) {
                Ok(matches) if !matches.is_empty() => {
                    if matches.len() == 1 {
                        println!(".NET SDK version {} is already installed.", matches[0].0);
                        return Ok(());
                    } else {
                        // Multiple matches found
                        println!("Multiple versions match '{}' and are already installed:", v);
                        for (ver, _) in &matches {
                            println!("  - {}", ver);
                        }
                        println!("\nPlease specify the exact version to install, or all listed versions are already installed.");
                        return Ok(());
                    }
                }
                Ok(_) => {
                    // No matches, proceed with installation
                    println!("Installing .NET SDK version {}...", v);
                }
                Err(e) => {
                    // If we can't check, proceed with installation attempt
                    eprintln!("Warning: Could not verify if SDK version is already installed: {}", e);
                    println!("Installing .NET SDK version {}...", v);
                }
            }
        } else {
            println!("Installing .NET SDK version {}...", v);
        }
    } else if lts {
        println!("Installing latest LTS .NET SDK...");
    } else {
        println!("Installing .NET SDK...");
    }

    // Proceed with installation
    if let Err(e) = install_dotnet(lts, version, install_path).await {
        eprintln!("Installation failed: {}", e);
        return Err(e);
    }
    println!("dotnet installation completed successfully.");
    Ok(())
}
