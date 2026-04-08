use semver::Version;
use std::env;
use std::path::{Path, PathBuf};

pub fn get_home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        env::var_os("HOME").map(PathBuf::from)
    }
}

pub fn get_dver_root() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(get_home_dir)
            .map(|path| path.join("dver"))
    } else {
        get_home_dir().map(|path| path.join(".dver"))
    }
}

pub fn get_versions_dir() -> Option<PathBuf> {
    get_dver_root().map(|root| root.join("versions"))
}

pub fn get_shims_dir() -> Option<PathBuf> {
    get_dver_root().map(|root| root.join("bin"))
}

pub fn get_default_version_file() -> Option<PathBuf> {
    get_dver_root().map(|root| root.join("default-version"))
}

pub fn get_managed_version_dir(version: &str) -> Option<PathBuf> {
    get_versions_dir().map(|dir| dir.join(version))
}

pub fn managed_dotnet_path(version_root: &Path) -> PathBuf {
    version_root.join(dotnet_binary_name())
}

pub fn dotnet_binary_name() -> &'static str {
    if cfg!(windows) {
        "dotnet.exe"
    } else {
        "dotnet"
    }
}

pub fn ensure_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

pub fn normalize_version_input(value: &str) -> String {
    value.trim().trim_start_matches('v').to_string()
}

pub fn is_probably_specific_version(value: &str) -> bool {
    let normalized = normalize_version_input(value);
    Version::parse(&normalized).is_ok() || normalized.split('.').count() >= 3
}

pub fn compare_versions_desc(left: &str, right: &str) -> std::cmp::Ordering {
    match (
        Version::parse(&normalize_version_input(left)),
        Version::parse(&normalize_version_input(right)),
    ) {
        (Ok(a), Ok(b)) => b.cmp(&a),
        _ => right.cmp(left),
    }
}

pub fn current_working_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(env::current_dir()?)
}
