use std::path::Path;

pub fn run_doctor_checks() {
    use crate::utils::common::get_home_dir;
    use crate::utils::sdk::is_dotnet_installed;
    
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
