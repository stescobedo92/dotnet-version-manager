use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run_doctor_checks() {
    use crate::utils::common::get_home_dir;

    println!("Checking environment configuration...");
    println!("-------------------------------------");

    let home = get_home_dir().expect("Could not determine home directory");
    let dver_dotnet_dir = if cfg!(windows) {
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

    // 1. Check where 'dotnet' resolves to
    let which_output = if cfg!(windows) {
        Command::new("where").arg("dotnet").output()
    } else {
        Command::new("which").arg("dotnet").output()
    };

    #[allow(unused_assignments)]
    let mut current_dotnet_path = PathBuf::new();
    #[allow(unused_assignments)]
    let mut is_shadowed = false;

    if let Ok(output) = which_output {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // 'where' on windows returns multiple lines, first is active. 'which' usually returns one.
            let first_path = path_str.lines().next().unwrap_or("").trim();
            current_dotnet_path = PathBuf::from(first_path);

            println!(
                "✅ 'dotnet' command found at: {}",
                current_dotnet_path.display()
            );

            // Check if it's our managed dotnet
            if !current_dotnet_path.starts_with(&dver_dotnet_dir) {
                println!("⚠️  ACTIVE DOTNET IS NOT MANAGED BY DVER");
                println!("   Current: {}", current_dotnet_path.display());
                println!("   Managed: {}", dver_dotnet_dir.display());
                is_shadowed = true;
            } else {
                println!("✅ Active 'dotnet' is correctly inside the managed directory.");
            }
        } else {
            println!("❌ 'dotnet' command NOT found in PATH.");
        }
    }

    // 2. Check PATH environment variable
    if let Ok(path_var) = std::env::var("PATH") {
        let separator = if cfg!(windows) { ';' } else { ':' };
        let paths: Vec<&str> = path_var.split(separator).collect();

        let _dver_dir_str = dver_dotnet_dir.to_string_lossy();
        let dver_in_path = paths
            .iter()
            .any(|p| Path::new(p) == dver_dotnet_dir || Path::new(p) == dver_dotnet_dir.join("")); // some paths might have trailing slash

        if is_shadowed {
            println!("\n🔍 DIAGNOSIS: PATH CONFIGURATION ISSUE");
            println!(
                "   The system 'dotnet' is taking precedence over the 'dver' managed versions."
            );
            println!("   This prevents you from using versions installed by dver.");

            if dver_in_path {
                println!("   ✅ The managed directory is in your PATH, but it comes AFTER the system dotnet.");
            } else {
                println!("   ❌ The managed directory is NOT in your PATH.");
            }

            println!("\n🛠  SUGGESTED FIX:");
            println!("   Run the following command to automatically fix your PATH:");
            println!("   dver setup");
            println!("\n   Alternatively, you can manually fix it by adding the managed directory");
            println!("   to the BEGINNING of your PATH variable.");
        } else if !dver_in_path {
            println!("\n⚠️  The managed directory is not in your PATH.");
            println!("   You might not be able to use installed SDKs.");
        } else {
            println!("\n✅ PATH configuration looks correct.");
        }
    }

    println!("\n-------------------------------------");
    println!("Run 'dver list' to see what versions dver has installed.");
    println!("Run 'dotnet --list-sdks' to see what the active dotnet sees.");
}
