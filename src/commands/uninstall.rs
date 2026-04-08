use crate::commands::setup;
use crate::utils::common::{get_dver_root, get_versions_dir};
use crate::utils::sdk::{
    clear_default_version, get_default_version, list_managed_sdks, resolve_managed_sdk,
};
use std::fs;

pub async fn handle_uninstall(
    version: Option<String>,
    all: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if all {
        return uninstall_all();
    }

    let version = version
        .ok_or("Please provide a version to uninstall, for example: dver uninstall 8.0.406")?;
    let sdk = resolve_managed_sdk(&version)?;
    let default_version = get_default_version()?;

    fs::remove_dir_all(&sdk.root)?;
    println!("Removed managed .NET SDK {}.", sdk.version);

    if default_version.as_deref() == Some(&sdk.version) {
        clear_default_version()?;
        println!("Cleared the default managed SDK because it matched the removed version.");
    }

    Ok(())
}

fn uninstall_all() -> Result<(), Box<dyn std::error::Error>> {
    let versions_dir =
        get_versions_dir().ok_or("Could not determine managed versions directory")?;
    let managed_sdks = list_managed_sdks()?;

    if managed_sdks.is_empty() {
        println!("No managed .NET SDKs are currently installed.");
    } else if versions_dir.exists() {
        fs::remove_dir_all(&versions_dir)?;
        println!("Removed all managed .NET SDK versions.");
    }

    clear_default_version()?;
    setup::remove_configuration()?;

    if let Some(root) = get_dver_root() {
        let shims_dir = root.join("bin");
        if shims_dir.exists() {
            fs::remove_dir_all(shims_dir)?;
        }
    }

    println!("Removed dver-managed SDK state. System or package-manager .NET installations were left untouched.");
    Ok(())
}
