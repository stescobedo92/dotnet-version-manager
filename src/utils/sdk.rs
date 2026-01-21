use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[allow(dead_code)]
pub fn is_dotnet_installed() -> bool {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct SdkLocation {
    pub version: String,
    pub path: PathBuf,
}

pub fn list_installed_sdks_grouped(
) -> Result<HashMap<String, Vec<SdkLocation>>, Box<dyn std::error::Error>> {
    use crate::utils::common::get_home_dir;

    let mut sdks_by_location: HashMap<String, Vec<SdkLocation>> = HashMap::new();

    // 1. Check via 'dotnet --list-sdks' (System source of truth)
    if let Ok(output) = Command::new("dotnet").args(["--list-sdks"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some((ver_part, path_part)) = line.split_once('[') {
                    let version = ver_part.split_whitespace().next().unwrap_or("").to_string();
                    let base = path_part.trim().trim_end_matches(']').trim();
                    if version.is_empty() || base.is_empty() {
                        continue;
                    }

                    let mut pb = PathBuf::from(base);
                    pb.push(&version);

                    let location_key = base.to_string();
                    sdks_by_location
                        .entry(location_key.clone())
                        .or_default()
                        .push(SdkLocation { version, path: pb });
                }
            }
        }
    }

    // 2. Check User-Installed Dotnet (Managed by dver)
    if let Some(home_dir) = get_home_dir() {
        let user_sdk_dir = if cfg!(windows) {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                PathBuf::from(local_app_data)
                    .join("Microsoft")
                    .join("dotnet")
                    .join("sdk")
            } else {
                home_dir.join(".dotnet").join("sdk")
            }
        } else {
            home_dir.join(".dotnet").join("sdk")
        };

        check_directory_for_sdks(&user_sdk_dir, &mut sdks_by_location);
    }

    // 3. Check Standard System Locations (in case 'dotnet' command is missing or broken)
    let system_locations = if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\dotnet\sdk"),
            PathBuf::from(r"C:\Program Files (x86)\dotnet\sdk"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/usr/local/share/dotnet/sdk"),
            PathBuf::from("/opt/homebrew/share/dotnet/sdk"), // Homebrew
            PathBuf::from("/opt/homebrew/opt/dotnet/libexec/sdk"), // Homebrew alternative
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/dotnet/sdk"),
            PathBuf::from("/usr/lib/dotnet/sdk"),
        ]
    };

    for loc in system_locations {
        check_directory_for_sdks(&loc, &mut sdks_by_location);
    }

    // Sort versions within each location
    for sdks in sdks_by_location.values_mut() {
        sdks.sort_by(|a, b| {
            let a_parts: Vec<u32> = a
                .version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
            let b_parts: Vec<u32> = b
                .version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();

            for i in 0..a_parts.len().max(b_parts.len()) {
                let a_val = a_parts.get(i).copied().unwrap_or(0);
                let b_val = b_parts.get(i).copied().unwrap_or(0);
                match a_val.cmp(&b_val) {
                    std::cmp::Ordering::Equal => continue,
                    other => return other,
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    Ok(sdks_by_location)
}

pub fn list_installed_runtimes_grouped(
) -> Result<HashMap<String, Vec<SdkLocation>>, Box<dyn std::error::Error>> {
    let mut runtimes_by_location: HashMap<String, Vec<SdkLocation>> = HashMap::new();

    // 1. Check via 'dotnet --list-runtimes' (System source of truth)
    if let Ok(output) = Command::new("dotnet").args(["--list-runtimes"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Expected format: "Microsoft.NETCore.App 6.0.0 [/usr/share/dotnet/shared/Microsoft.NETCore.App]"
                if let Some((info_part, path_part)) = line.split_once('[') {
                    let parts: Vec<&str> = info_part.split_whitespace().collect();
                    if parts.len() < 2 {
                        continue;
                    }

                    let version = parts[1].to_string(); // 6.0.0
                    let _name = parts[0]; // Microsoft.NETCore.App
                    let base = path_part.trim().trim_end_matches(']').trim();

                    if version.is_empty() || base.is_empty() {
                        continue;
                    }

                    let mut pb = PathBuf::from(base);
                    pb.push(&version);

                    let location_key = base.to_string();
                    runtimes_by_location
                        .entry(location_key.clone())
                        .or_default()
                        .push(SdkLocation { version, path: pb });
                }
            }
        }
    }

    // 2. Check Standard System Locations for Runtimes (shared folder)
    // Common structure: .../dotnet/shared/{RuntimeName}/{Version}
    // We want to list all of them.

    let shared_dirs = if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\dotnet\shared"),
            PathBuf::from(r"C:\Program Files (x86)\dotnet\shared"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/usr/local/share/dotnet/shared"),
            PathBuf::from("/opt/homebrew/share/dotnet/shared"),
            PathBuf::from("/opt/homebrew/opt/dotnet/libexec/shared"),
            // User reported path: /opt/homebrew/Cellar/dotnet/VERSION/libexec/shared
            // We can't easily guess the version in the path without scanning Cellar, but if dotnet --list-runtimes found it, we have it.
            // If dotnet command is missing, we might miss the Cellar ones unless we specifically look for them.
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/dotnet/shared"),
            PathBuf::from("/usr/lib/dotnet/shared"),
        ]
    };

    for shared_parent in shared_dirs {
        if shared_parent.exists() {
            if let Ok(runtime_types) = std::fs::read_dir(&shared_parent) {
                for entry in runtime_types.flatten() {
                    if entry.path().is_dir() {
                        // e.g. Microsoft.NETCore.App
                        check_directory_for_sdks(&entry.path(), &mut runtimes_by_location);
                    }
                }
            }
        }
    }

    Ok(runtimes_by_location)
}

fn check_directory_for_sdks(
    sdk_dir: &std::path::Path,
    sdks_by_location: &mut HashMap<String, Vec<SdkLocation>>,
) {
    if !sdk_dir.exists() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(sdk_dir) {
        // Location key is usually the parent of the sdk dir for display niceness, checking consistency with dotnet output
        // dotnet --list-sdks output: [C:\Program Files\dotnet\sdk]
        // so we use the parent of the version folder, which is sdk_dir
        let location_key = sdk_dir.display().to_string();

        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(version_name) = entry.file_name().to_str() {
                    let version = version_name.to_string();

                    // Simple validation: must start with digit
                    if !version.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        continue;
                    }

                    // Check if this version is already listed for this specific location
                    let already_exists = sdks_by_location
                        .get(&location_key)
                        .map(|sdks| sdks.iter().any(|sdk| sdk.version == version))
                        .unwrap_or(false);

                    if !already_exists {
                        sdks_by_location
                            .entry(location_key.clone())
                            .or_default()
                            .push(SdkLocation {
                                version,
                                path: entry.path(),
                            });
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
pub fn list_installed_sdks() -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    let grouped = list_installed_sdks_grouped()?;
    let mut all_sdks = Vec::new();

    for sdks in grouped.values() {
        for sdk in sdks {
            all_sdks.push((sdk.version.clone(), sdk.path.clone()));
        }
    }

    // Sort by version
    all_sdks.sort_by(|a, b| {
        let a_parts: Vec<u32> = a.0.split('.').filter_map(|s| s.parse().ok()).collect();
        let b_parts: Vec<u32> = b.0.split('.').filter_map(|s| s.parse().ok()).collect();

        for i in 0..a_parts.len().max(b_parts.len()) {
            let a_val = a_parts.get(i).copied().unwrap_or(0);
            let b_val = b_parts.get(i).copied().unwrap_or(0);
            match a_val.cmp(&b_val) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        std::cmp::Ordering::Equal
    });

    // Deduplicate by version (keep first occurrence)
    let mut seen = std::collections::HashSet::new();
    all_sdks.retain(|(v, _)| seen.insert(v.clone()));

    Ok(all_sdks)
}

#[allow(dead_code)]
pub fn find_matching_versions(
    pattern: &str,
) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    let sdks = list_installed_sdks()?;

    // Exact match first
    let exact_matches: Vec<_> = sdks.iter().filter(|(v, _)| v == pattern).cloned().collect();

    if !exact_matches.is_empty() {
        return Ok(exact_matches);
    }

    // Partial match (prefix)
    let prefix_matches: Vec<_> = sdks
        .into_iter()
        .filter(|(v, _)| v.starts_with(pattern))
        .collect();

    Ok(prefix_matches)
}

#[allow(dead_code)]
pub fn prompt_user_selection(
    matches: &[(String, PathBuf)],
) -> Result<usize, Box<dyn std::error::Error>> {
    println!("\nMultiple matching versions found:");
    for (i, (ver, _)) in matches.iter().enumerate() {
        println!("  {}. {}", i + 1, ver);
    }

    print!("\nSelect a version (1-{}): ", matches.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let selection: usize = input
        .trim()
        .parse()
        .map_err(|_| "Invalid input: please enter a number")?;

    if selection < 1 || selection > matches.len() {
        return Err("Selection out of range".into());
    }

    Ok(selection - 1)
}
