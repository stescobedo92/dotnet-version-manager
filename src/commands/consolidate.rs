use std::collections::HashSet;
use std::io::{self, Write};

use crate::commands::install::handle_install;
use crate::commands::setup::move_to_top_of_path;
use crate::commands::uninstall::handle_uninstall;
use crate::utils::sdk::list_installed_sdks_grouped;

pub async fn handle_consolidate(skip_confirm: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Scanning for all installed .NET SDK versions...");
    println!();

    // 1. Scan all locations and collect unique versions
    let sdks_by_location = list_installed_sdks_grouped()?;

    if sdks_by_location.is_empty() {
        println!("No .NET SDK installations found. Nothing to consolidate.");
        return Ok(());
    }

    // Extract unique versions
    let mut unique_versions: HashSet<String> = HashSet::new();
    let mut non_managed_count = 0;
    let mut managed_count = 0;

    let home = crate::utils::common::get_home_dir();
    let managed_path_prefix = home.as_ref().map(|h| {
        if cfg!(windows) {
            h.join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("dotnet")
        } else {
            h.join(".dotnet")
        }
    });

    for (location, sdks) in &sdks_by_location {
        for sdk in sdks {
            unique_versions.insert(sdk.version.clone());

            let is_managed = managed_path_prefix
                .as_ref()
                .map(|mp| sdk.path.starts_with(mp))
                .unwrap_or(false);

            if is_managed {
                managed_count += 1;
            } else {
                non_managed_count += 1;
            }
        }
    }

    let versions: Vec<String> = {
        let mut v: Vec<_> = unique_versions.into_iter().collect();
        v.sort();
        v
    };

    // 2. Display summary
    println!("📦 Found {} unique SDK version(s):", versions.len());
    for v in &versions {
        println!("   - {}", v);
    }
    println!();
    println!("📍 Locations:");
    for location in sdks_by_location.keys() {
        println!("   - {}", location);
    }
    println!();

    if non_managed_count == 0 {
        println!("✅ All SDKs are already in the dver-managed location.");
        println!("   No consolidation needed.");
        return Ok(());
    }

    println!(
        "⚠️  {} installation(s) are outside the managed folder.",
        non_managed_count
    );
    println!();
    println!("📋 Consolidation Plan:");
    println!("   1. Uninstall ALL SDKs (including via package managers like Homebrew/Snap)");
    println!(
        "   2. Reinstall {} version(s) into ~/.dotnet via dver",
        versions.len()
    );
    println!("   3. Configure PATH to use the managed location");
    println!();

    // 3. Confirmation
    if !skip_confirm {
        print!("Do you want to proceed? [y/N]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    println!();
    println!("🚀 Starting consolidation...");
    println!();

    // 4. Uninstall everything
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Step 1/3: Uninstalling all .NET SDKs...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    handle_uninstall(None, true).await?;

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "Step 2/3: Reinstalling {} version(s) into ~/.dotnet...",
        versions.len()
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 5. Reinstall each version
    let mut install_errors = Vec::new();
    for version in &versions {
        println!();
        println!("📥 Installing {}...", version);
        match handle_install(false, Some(version.clone()), None).await {
            Ok(_) => println!("✅ {} installed successfully.", version),
            Err(e) => {
                eprintln!("❌ Failed to install {}: {}", version, e);
                install_errors.push(version.clone());
            }
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Step 3/3: Configuring PATH...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 6. Configure PATH
    if let Err(e) = move_to_top_of_path() {
        eprintln!("⚠️  Failed to configure PATH: {}", e);
    }

    // 7. Summary
    println!();
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("                           CONSOLIDATION COMPLETE");
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!();

    if install_errors.is_empty() {
        println!(
            "✅ All {} SDK version(s) have been consolidated into ~/.dotnet",
            versions.len()
        );
    } else {
        println!(
            "⚠️  {} version(s) installed successfully.",
            versions.len() - install_errors.len()
        );
        println!("❌ {} version(s) failed to install:", install_errors.len());
        for v in &install_errors {
            println!("   - {}", v);
        }
    }

    println!();
    println!("📌 Next step: Restart your terminal or run 'source ~/.zshrc' to apply PATH changes.");

    Ok(())
}
