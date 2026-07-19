use crate::utils::common::{
    compare_versions_desc, ensure_dir, get_default_version_file, get_managed_version_dir,
    get_versions_dir, managed_dotnet_path, normalize_version_input,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSdk {
    pub version: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSdk {
    pub version: String,
    pub location: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSource {
    LocalGlobalJson(PathBuf),
    DefaultAlias,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSelection {
    pub requested_version: String,
    pub source: VersionSource,
}

#[derive(Debug, Deserialize)]
struct GlobalJsonFile {
    sdk: Option<GlobalJsonSdk>,
}

#[derive(Debug, Deserialize)]
struct GlobalJsonSdk {
    version: Option<String>,
}

pub fn list_managed_sdks() -> Result<Vec<ManagedSdk>, Box<dyn std::error::Error>> {
    let versions_dir = get_versions_dir().ok_or("Could not determine dver versions directory")?;
    ensure_dir(&versions_dir)?;

    let mut versions = Vec::new();
    for entry in fs::read_dir(&versions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };

        // Skip staging directories left by interrupted installs (SDKMAN-style tmp).
        if name.starts_with('.') {
            continue;
        }

        if !managed_dotnet_path(&path).exists() {
            continue;
        }

        versions.push(ManagedSdk {
            version: name,
            root: path,
        });
    }

    versions.sort_by(|left, right| compare_versions_desc(&left.version, &right.version));
    Ok(versions)
}

pub fn get_managed_sdk(version: &str) -> Result<Option<ManagedSdk>, Box<dyn std::error::Error>> {
    let normalized = normalize_version_input(version);
    let path = get_managed_version_dir(&normalized)
        .ok_or("Could not determine managed version directory")?;

    if path.exists() && managed_dotnet_path(&path).exists() {
        return Ok(Some(ManagedSdk {
            version: normalized,
            root: path,
        }));
    }

    Ok(None)
}

pub fn resolve_managed_sdk(selector: &str) -> Result<ManagedSdk, Box<dyn std::error::Error>> {
    let selector = normalize_version_input(selector);
    let sdks = list_managed_sdks()?;
    resolve_managed_sdk_from_list(&selector, &sdks)
}

fn resolve_managed_sdk_from_list(
    selector: &str,
    sdks: &[ManagedSdk],
) -> Result<ManagedSdk, Box<dyn std::error::Error>> {
    if let Some(exact) = sdks.iter().find(|sdk| sdk.version == selector) {
        return Ok(exact.clone());
    }

    let matches: Vec<_> = sdks
        .iter()
        .filter(|sdk| version_matches_selector(&sdk.version, selector))
        .cloned()
        .collect();

    match matches.len() {
        0 => Err(format!(
            "Managed .NET SDK version '{selector}' is not installed. Run 'dver install {selector}'."
        )
        .into()),
        1 => Ok(matches[0].clone()),
        _ => {
            let versions = matches
                .iter()
                .map(|sdk| sdk.version.clone())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Version selector '{selector}' is ambiguous. Matching managed versions: {versions}"
            )
            .into())
        }
    }
}

pub fn find_nearest_global_json(start_dir: &Path) -> Option<PathBuf> {
    let mut current = Some(start_dir);
    while let Some(dir) = current {
        let candidate = dir.join("global.json");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }

    None
}

pub fn read_global_json_version(path: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let parsed: GlobalJsonFile = serde_json::from_str(&contents)?;
    Ok(parsed
        .sdk
        .and_then(|sdk| sdk.version)
        .map(|version| normalize_version_input(&version)))
}

pub fn get_default_version() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let Some(file) = get_default_version_file() else {
        return Ok(None);
    };

    if !file.exists() {
        return Ok(None);
    }

    let version = fs::read_to_string(file)?;
    let version = normalize_version_input(version.trim());
    if version.is_empty() {
        return Ok(None);
    }

    Ok(Some(version))
}

pub fn set_default_version(version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = get_default_version_file().ok_or("Could not determine default version file")?;
    let parent = file
        .parent()
        .ok_or("Could not determine default version file parent")?;
    ensure_dir(parent)?;
    fs::write(file, format!("{}\n", normalize_version_input(version)))?;
    Ok(())
}

pub fn clear_default_version() -> Result<(), Box<dyn std::error::Error>> {
    let Some(file) = get_default_version_file() else {
        return Ok(());
    };

    if file.exists() {
        fs::remove_file(file)?;
    }

    Ok(())
}

pub fn resolve_version_selection(
    current_dir: &Path,
) -> Result<Option<VersionSelection>, Box<dyn std::error::Error>> {
    if let Some(global_json) = find_nearest_global_json(current_dir) {
        if let Some(version) = read_global_json_version(&global_json)? {
            return Ok(Some(VersionSelection {
                requested_version: version,
                source: VersionSource::LocalGlobalJson(global_json),
            }));
        }
    }

    if let Some(version) = get_default_version()? {
        return Ok(Some(VersionSelection {
            requested_version: version,
            source: VersionSource::DefaultAlias,
        }));
    }

    Ok(None)
}

pub fn list_system_sdks() -> Result<Vec<SystemSdk>, Box<dyn std::error::Error>> {
    let mut discovered = BTreeMap::<(String, PathBuf), SystemSdk>::new();

    collect_sdks_from_dotnet_command(&mut discovered);

    for sdk_dir in standard_system_sdk_dirs() {
        collect_sdks_from_directory(&sdk_dir, &mut discovered);
    }

    let mut sdks = discovered
        .into_values()
        .filter(|sdk| !is_under_dver_root(&sdk.location))
        .collect::<Vec<_>>();
    sdks.sort_by(|left, right| {
        compare_versions_desc(&left.version, &right.version)
            .then_with(|| left.location.cmp(&right.location))
    });
    Ok(sdks)
}

/// Locates a real system dotnet executable on PATH, skipping dver shims and
/// other version-manager shim directories so listings never go through a proxy.
pub fn find_system_dotnet_on_path() -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let shims_dir = crate::utils::common::get_shims_dir();
    let binary = crate::utils::common::dotnet_binary_name();

    for dir in path.split(crate::utils::platform::path_separator()) {
        if dir.is_empty() {
            continue;
        }

        let directory = PathBuf::from(dir);
        if shims_dir.as_ref().is_some_and(|shim| shim == &directory) {
            continue;
        }

        let lower = dir.to_ascii_lowercase();
        if lower.contains(".dotver") || (lower.contains("dver") && lower.ends_with("bin")) {
            continue;
        }

        let candidate = directory.join(binary);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Resolves a system-installed SDK by exact version or unique boundary prefix,
/// using the same matching rules as managed SDK resolution.
pub fn resolve_system_sdk(selector: &str) -> Result<SystemSdk, Box<dyn std::error::Error>> {
    let selector = normalize_version_input(selector);
    let sdks = list_system_sdks()?;

    if let Some(exact) = sdks.iter().find(|sdk| sdk.version == selector) {
        return Ok(exact.clone());
    }

    let matches: Vec<_> = sdks
        .iter()
        .filter(|sdk| version_matches_selector(&sdk.version, &selector))
        .cloned()
        .collect();

    match matches.len() {
        0 => Err(format!(
            "No system-installed .NET SDK matches '{selector}'. Run 'dver list' to see what is installed."
        )
        .into()),
        1 => Ok(matches[0].clone()),
        _ => {
            let versions = matches
                .iter()
                .map(|sdk| sdk.version.clone())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Version selector '{selector}' is ambiguous. Matching system versions: {versions}"
            )
            .into())
        }
    }
}

/// Returns versions installed both as dver-managed copies and system-wide.
pub fn find_duplicated_versions() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let managed = list_managed_sdks()?
        .into_iter()
        .map(|sdk| sdk.version)
        .collect::<std::collections::BTreeSet<_>>();
    let system = list_system_sdks()?
        .into_iter()
        .map(|sdk| sdk.version)
        .collect::<std::collections::BTreeSet<_>>();

    Ok(managed.intersection(&system).cloned().collect())
}

fn is_under_dver_root(location: &Path) -> bool {
    let Some(root) = crate::utils::common::get_dver_root() else {
        return false;
    };

    let location = location.to_string_lossy().to_ascii_lowercase();
    let root = root.to_string_lossy().to_ascii_lowercase();
    location.starts_with(&root)
}

fn version_matches_selector(version: &str, selector: &str) -> bool {
    version == selector
        || version.starts_with(&format!("{selector}."))
        || version.starts_with(&format!("{selector}-"))
}

fn collect_sdks_from_dotnet_command(target: &mut BTreeMap<(String, PathBuf), SystemSdk>) {
    // Use a real system dotnet, never the shim, so managed SDKs are not
    // reported back as "system" installations.
    let Some(dotnet) = find_system_dotnet_on_path() else {
        return;
    };
    let Ok(output) = Command::new(dotnet).arg("--list-sdks").output() else {
        return;
    };
    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let Some((version_part, location_part)) = line.split_once('[') else {
            continue;
        };

        let version = version_part
            .split_whitespace()
            .next()
            .map(normalize_version_input)
            .unwrap_or_default();

        let location = location_part.trim().trim_end_matches(']').trim();
        if version.is_empty() || location.is_empty() {
            continue;
        }

        let location = PathBuf::from(location);
        let key = (version.clone(), location.clone());
        target.entry(key).or_insert(SystemSdk { version, location });
    }
}

fn collect_sdks_from_directory(
    sdk_dir: &Path,
    target: &mut BTreeMap<(String, PathBuf), SystemSdk>,
) {
    if !sdk_dir.exists() {
        return;
    }

    let Ok(entries) = fs::read_dir(sdk_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(version) = entry.file_name().to_str().map(normalize_version_input) else {
            continue;
        };

        if version.is_empty()
            || !version
                .chars()
                .next()
                .is_some_and(|value| value.is_ascii_digit())
        {
            continue;
        }

        let key = (version.clone(), sdk_dir.to_path_buf());
        target.entry(key).or_insert(SystemSdk {
            version,
            location: sdk_dir.to_path_buf(),
        });
    }
}

fn standard_system_sdk_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\dotnet\sdk"),
            PathBuf::from(r"C:\Program Files (x86)\dotnet\sdk"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/usr/local/share/dotnet/sdk"),
            PathBuf::from("/opt/homebrew/share/dotnet/sdk"),
            PathBuf::from("/usr/local/share/dotnet/x64/sdk"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/dotnet/sdk"),
            PathBuf::from("/usr/lib/dotnet/sdk"),
            PathBuf::from("/usr/local/share/dotnet/sdk"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sdk(version: &str) -> ManagedSdk {
        ManagedSdk {
            version: version.to_string(),
            root: PathBuf::from(format!("/tmp/{version}")),
        }
    }

    #[test]
    fn resolves_exact_version_first() {
        let sdks = vec![sdk("8.0.406"), sdk("8.0.407")];
        let resolved = resolve_managed_sdk_from_list("8.0.406", &sdks).unwrap();
        assert_eq!(resolved.version, "8.0.406");
    }

    #[test]
    fn resolves_unique_prefix() {
        let sdks = vec![sdk("8.0.406"), sdk("9.0.100")];
        let resolved = resolve_managed_sdk_from_list("8.0", &sdks).unwrap();
        assert_eq!(resolved.version, "8.0.406");
    }

    #[test]
    fn rejects_ambiguous_prefix() {
        let sdks = vec![sdk("8.0.406"), sdk("8.0.407")];
        let err = resolve_managed_sdk_from_list("8.0", &sdks).unwrap_err();
        assert!(err.to_string().contains("ambiguous"));
    }

    #[test]
    fn rejects_partial_patch_prefix_without_boundary() {
        let sdks = vec![sdk("8.0.406")];
        let err = resolve_managed_sdk_from_list("8.0.40", &sdks).unwrap_err();
        assert!(err.to_string().contains("not installed"));
    }
}
