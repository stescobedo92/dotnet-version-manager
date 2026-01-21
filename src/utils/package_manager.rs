use std::process::Command;

pub fn try_uninstall_all() {
    println!("🔍 Checking for Package Manager installations...");

    if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
        if check_brew_installed() {
            uninstall_brew_dotnet();
        }
    }

    if cfg!(target_os = "linux") {
        if check_snap_installed() {
            uninstall_snap_dotnet();
        }
    }
}

fn check_brew_installed() -> bool {
    Command::new("brew")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn uninstall_brew_dotnet() {
    // Check if dotnet is installed via brew
    // We check for 'dotnet' (cask or formula) and 'dotnet-sdk'

    let packages = ["dotnet", "dotnet-sdk", "dotnet-runtime"];

    for pkg in packages {
        let check = Command::new("brew").args(["list", pkg]).output();

        if let Ok(output) = check {
            if output.status.success() {
                println!("📦 Found Homebrew package: {}", pkg);
                println!("   Attempting to uninstall via brew...");

                let status = Command::new("brew").args(["uninstall", pkg]).status();

                match status {
                    Ok(s) => {
                        if s.success() {
                            println!("✅ Successfully uninstalled {} via Homebrew.", pkg);
                        } else {
                            eprintln!("❌ Failed to uninstall {} via Homebrew.", pkg);
                        }
                    }
                    Err(e) => eprintln!("❌ Failed to execute brew uninstall: {}", e),
                }
            }
        }
    }
}

fn check_snap_installed() -> bool {
    Command::new("snap")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn uninstall_snap_dotnet() {
    // Snap packages are typically 'dotnet-sdk' or 'dotnet-runtime'
    // 'dotnet-sdk' usually includes everything.
    let packages = ["dotnet-sdk", "dotnet-runtime"];

    for pkg in packages {
        let check = Command::new("snap").args(["list", pkg]).output();

        if let Ok(output) = check {
            if output.status.success() {
                println!("📦 Found Snap package: {}", pkg);
                println!("   Attempting to uninstall via snap (asking for sudo)...");

                // Snap remove usually requires sudo
                let status = Command::new("sudo").args(["snap", "remove", pkg]).status();

                match status {
                    Ok(s) => {
                        if s.success() {
                            println!("✅ Successfully uninstalled {} via Snap.", pkg);
                        } else {
                            eprintln!("❌ Failed to uninstall {} via Snap.", pkg);
                        }
                    }
                    Err(e) => eprintln!("❌ Failed to execute snap remove: {}", e),
                }
            }
        }
    }
}
