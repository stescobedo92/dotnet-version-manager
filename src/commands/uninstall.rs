use std::path::PathBuf;
use std::fs::{self, remove_dir_all};
use std::io;

pub async fn handle_uninstall(version: Option<String>, all: bool) -> Result<(), Box<dyn std::error::Error>> {
    use crate::utils::sdk::{list_installed_sdks, find_matching_versions, prompt_user_selection};
    
    let sdks = list_installed_sdks()?;
    
    if sdks.is_empty() {
        println!("No .NET SDK versions found.");
        return Ok(());
    }
    
    // Determine sdk root(s) from listed entries to avoid deleting outside
    let mut roots: Vec<PathBuf> = sdks
        .iter()
        .filter_map(|(_, p)| p.parent().map(|pp| pp.to_path_buf()))
        .collect();
    roots.sort();
    roots.dedup();

    let targets: Vec<(String, PathBuf)> = if all {
        sdks
    } else if let Some(v) = version {
        // Use the new find_matching_versions function
        match find_matching_versions(&v) {
            Ok(matches) if !matches.is_empty() => {
                if matches.len() == 1 {
                    matches
                } else {
                    // Multiple matches - prompt user to select
                    match prompt_user_selection(&matches) {
                        Ok(idx) => vec![matches[idx].clone()],
                        Err(e) => {
                            eprintln!("Error selecting version: {}", e);
                            return Err(e);
                        }
                    }
                }
            }
            Ok(_) => {
                println!("No matching SDK versions found for '{}'.", v);
                Vec::new()
            }
            Err(e) => {
                eprintln!("Error searching for SDK versions: {}", e);
                return Err(e);
            }
        }
    } else {
        eprintln!("Please provide a version or --all to uninstall.");
        Vec::new()
    };

    if targets.is_empty() {
        return Ok(());
    }
    
    // Check if we need elevated privileges
    let needs_sudo = targets.iter().any(|(_, path)| {
        if let Ok(metadata) = fs::metadata(path) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                // Check if owned by root (uid 0)
                metadata.uid() == 0
            }
            #[cfg(not(unix))]
            {
                // On Windows, try to check if we can write
                metadata.permissions().readonly()
            }
        } else {
            false
        }
    });

    if needs_sudo {
        if cfg!(windows) {
            eprintln!("\nWarning: Some SDK installations may require elevated privileges to remove.");
            eprintln!("If removal fails, try running this command as Administrator.\n");
        } else {
            eprintln!("\nWarning: Some SDK installations require elevated privileges to remove.");
            eprintln!("You may need to run this command with 'sudo'.");
            eprintln!("Example: sudo dver uninstall {}\n", 
                targets.first().map(|(v, _)| v.as_str()).unwrap_or("VERSION"));
        }
    }

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
                Err(e) => {
                    eprintln!("Failed to remove {}: {}", ver, e);
                    
                    // Provide more helpful error message based on error kind
                    if e.kind() == io::ErrorKind::PermissionDenied {
                        if cfg!(windows) {
                            eprintln!("  → Try running this command as Administrator.");
                        } else {
                            eprintln!("  → Try running: sudo dver uninstall {}", ver);
                        }
                    }
                }
            }
        } else {
            println!("Directory for {} not found", ver);
        }
    }
    
    Ok(())
}
