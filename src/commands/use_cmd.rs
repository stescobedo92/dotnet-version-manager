use crate::utils::common::current_working_dir;
use crate::utils::sdk::{clear_default_version, resolve_managed_sdk, set_default_version};
use serde_json::{json, Map, Value};
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
        println!("Path: {}", sdk.root.display());
        println!("This is used when no local global.json is present (like `sdk default`).");
        return Ok(());
    }

    let target_path = current_working_dir()?.join("global.json");
    let merged = write_global_json(&target_path, &sdk.version)?;
    if merged {
        println!(
            "Updated local global.json with SDK {} (other keys preserved).",
            sdk.version
        );
    } else {
        println!("Created local global.json with SDK {}.", sdk.version);
    }
    println!("Location: {}", target_path.display());
    println!("Path: {}", sdk.root.display());

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

/// Writes/updates sdk.version while preserving unrelated global.json fields.
/// Returns true when an existing file was merged.
fn write_global_json(path: &Path, version: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let merged = path.exists();
    let mut root = if merged {
        let contents = fs::read_to_string(path)?;
        serde_json::from_str::<Value>(&contents).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    if !root.is_object() {
        root = json!({});
    }

    let root_object = root
        .as_object_mut()
        .ok_or("global.json root must be an object")?;
    let sdk_entry = root_object
        .entry("sdk")
        .or_insert_with(|| Value::Object(Map::new()));
    if !sdk_entry.is_object() {
        *sdk_entry = Value::Object(Map::new());
    }
    sdk_entry
        .as_object_mut()
        .ok_or("global.json sdk must be an object")?
        .insert("version".to_string(), Value::String(version.to_string()));

    let parent = path
        .parent()
        .ok_or("Could not determine the parent directory for global.json")?;
    fs::create_dir_all(parent)?;
    fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(merged)
}
