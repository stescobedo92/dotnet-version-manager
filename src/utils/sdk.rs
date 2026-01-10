use std::path::PathBuf;
use std::process::Command;
use std::io::{self, Write};
use std::collections::HashMap;

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

pub fn list_installed_sdks_grouped() -> Result<HashMap<String, Vec<SdkLocation>>, Box<dyn std::error::Error>> {
    use crate::utils::common::get_home_dir;
    
    let mut sdks_by_location: HashMap<String, Vec<SdkLocation>> = HashMap::new();
    
    // First, check system dotnet
    if let Ok(output) = Command::new("dotnet").args(["--list-sdks"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Expected format: "8.0.406 [C:\\Program Files\\dotnet\\sdk]"
                if let Some((ver_part, path_part)) = line.split_once('[') {
                    let version = ver_part.split_whitespace().next().unwrap_or("").to_string();
                    let base = path_part.trim().trim_end_matches(']').trim();
                    if version.is_empty() || base.is_empty() { continue; }
                    
                    let mut pb = PathBuf::from(base);
                    pb.push(&version);
                    
                    let location_key = base.to_string();
                    sdks_by_location.entry(location_key.clone()).or_default().push(SdkLocation {
                        version,
                        path: pb,
                    });
                }
            }
        }
    }
    
    // Also check user-installed dotnet (e.g., ~/.dotnet on Linux/Mac, %LOCALAPPDATA%\Microsoft\dotnet on Windows)
    if let Some(home_dir) = get_home_dir() {
        let user_dotnet_path = if cfg!(windows) {
            // On Windows, check %LOCALAPPDATA%\Microsoft\dotnet
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                PathBuf::from(local_app_data).join("Microsoft").join("dotnet").join("dotnet.exe")
            } else {
                home_dir.join(".dotnet").join("dotnet.exe")
            }
        } else {
            // On Linux/Mac, check ~/.dotnet
            home_dir.join(".dotnet").join("dotnet")
        };
        
        // Try to use the user dotnet if it exists
        if user_dotnet_path.exists() {
            if let Ok(output) = Command::new(&user_dotnet_path).args(["--list-sdks"]).output() {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if let Some((ver_part, path_part)) = line.split_once('[') {
                            let version = ver_part.split_whitespace().next().unwrap_or("").to_string();
                            let base = path_part.trim().trim_end_matches(']').trim();
                            if version.is_empty() || base.is_empty() { continue; }
                            
                            let mut pb = PathBuf::from(base);
                            pb.push(&version);
                            
                            let location_key = base.to_string();
                            sdks_by_location.entry(location_key.clone()).or_default().push(SdkLocation {
                                version,
                                path: pb,
                            });
                        }
                    }
                }
            }
        }
        
        // Also manually check the user SDK directory in case dotnet isn't there
        let user_sdk_dir = if cfg!(windows) {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                PathBuf::from(local_app_data).join("Microsoft").join("dotnet").join("sdk")
            } else {
                home_dir.join(".dotnet").join("sdk")
            }
        } else {
            home_dir.join(".dotnet").join("sdk")
        };
        
        if user_sdk_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&user_sdk_dir) {
                let location_key = user_sdk_dir.display().to_string();
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(version_name) = entry.file_name().to_str() {
                            let version = version_name.to_string();
                            // Check if this version is already listed for this location
                            let already_exists = sdks_by_location
                                .get(&location_key)
                                .map(|sdks| sdks.iter().any(|sdk| sdk.version == version))
                                .unwrap_or(false);
                            
                            if !already_exists {
                                sdks_by_location.entry(location_key.clone()).or_default().push(SdkLocation {
                                    version,
                                    path: entry.path(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Sort versions within each location
    for sdks in sdks_by_location.values_mut() {
        sdks.sort_by(|a, b| {
            let a_parts: Vec<u32> = a.version.split('.').filter_map(|s| s.parse().ok()).collect();
            let b_parts: Vec<u32> = b.version.split('.').filter_map(|s| s.parse().ok()).collect();
            
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


pub fn find_matching_versions(pattern: &str) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    let sdks = list_installed_sdks()?;
    
    // Exact match first
    let exact_matches: Vec<_> = sdks.iter()
        .filter(|(v, _)| v == pattern)
        .cloned()
        .collect();
    
    if !exact_matches.is_empty() {
        return Ok(exact_matches);
    }
    
    // Partial match (prefix)
    let prefix_matches: Vec<_> = sdks.into_iter()
        .filter(|(v, _)| v.starts_with(pattern))
        .collect();
    
    Ok(prefix_matches)
}

pub fn prompt_user_selection(matches: &[(String, PathBuf)]) -> Result<usize, Box<dyn std::error::Error>> {
    println!("\nMultiple matching versions found:");
    for (i, (ver, _)) in matches.iter().enumerate() {
        println!("  {}. {}", i + 1, ver);
    }
    
    print!("\nSelect a version (1-{}): ", matches.len());
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let selection: usize = input.trim().parse()
        .map_err(|_| "Invalid input: please enter a number")?;
    
    if selection < 1 || selection > matches.len() {
        return Err("Selection out of range".into());
    }
    
    Ok(selection - 1)
}
