use crate::utils::common::current_working_dir;
use crate::utils::sdk::{clear_default_version, resolve_managed_sdk, set_default_version};
use serde_json::json;
use std::fs;
use std::path::Path;

pub async fn handle_use(
    version_arg: Option<String>,
    global: bool,
    clear: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if clear {
        return clear_use_target(global);
    }

    let version_arg =
        version_arg.ok_or("Please provide a version, for example: dver use 8.0.406")?;
    let sdk = resolve_managed_sdk(&version_arg)?;

    if global {
        set_default_version(&sdk.version)?;
        println!("Default managed .NET SDK set to {}.", sdk.version);
        println!("This is the version dver will use when no local global.json is present.");
        return Ok(());
    }

    let target_path = current_working_dir()?.join("global.json");
    write_global_json(&target_path, &sdk.version)?;
    println!("Created local global.json with SDK {}.", sdk.version);
    println!("Location: {}", target_path.display());

    Ok(())
}

fn clear_use_target(global: bool) -> Result<(), Box<dyn std::error::Error>> {
    if global {
        clear_default_version()?;
        println!("Cleared the default managed SDK.");
        return Ok(());
    }

    let global_json = current_working_dir()?.join("global.json");
    if global_json.exists() {
        fs::remove_file(&global_json)?;
        println!("Removed local global.json at {}.", global_json.display());
    } else {
        println!("No local global.json was found in the current directory.");
    }

    Ok(())
}

fn write_global_json(path: &Path, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let json_data = json!({
        "sdk": {
            "version": version
        }
    });

    let parent = path
        .parent()
        .ok_or("Could not determine the parent directory for global.json")?;
    fs::create_dir_all(parent)?;
    fs::write(path, serde_json::to_string_pretty(&json_data)?)?;
    Ok(())
}
