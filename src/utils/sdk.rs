use std::path::PathBuf;
use std::process::Command;
use std::io::{self, Write};

pub fn is_dotnet_installed() -> bool {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn list_installed_sdks() -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
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
            let version = ver_part.split_whitespace().next().unwrap_or("").to_string();
            let base = path_part.trim().trim_end_matches(']').trim();
            if version.is_empty() || base.is_empty() { continue; }
            let mut pb = PathBuf::from(base);
            pb.push(&version);
            sdks.push((version, pb));
        }
    }
    Ok(sdks)
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
