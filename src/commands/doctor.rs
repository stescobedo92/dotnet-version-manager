use crate::utils::common::{
    current_working_dir, get_dver_root, get_shims_dir, managed_dotnet_path,
};
use crate::utils::platform::path_separator;
use crate::utils::sdk::{
    find_nearest_global_json, get_default_version, list_managed_sdks, read_global_json_version,
    resolve_version_selection, VersionSource,
};
use std::env;
use std::path::PathBuf;
use std::process::Command;

pub fn run_doctor_checks() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = current_working_dir()?;
    let dver_root = get_dver_root().ok_or("Could not determine dver root")?;
    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;
    let managed_sdks = list_managed_sdks()?;

    println!("dver root: {}", dver_root.display());
    println!("dver shims: {}", shims_dir.display());
    println!("Managed SDKs installed: {}", managed_sdks.len());

    if let Some(default_version) = get_default_version()? {
        println!("Default managed version: {default_version}");
    } else {
        println!("Default managed version: none");
    }

    if let Some(global_json) = find_nearest_global_json(&current_dir) {
        match read_global_json_version(&global_json)? {
            Some(version) => {
                println!(
                    "Nearest global.json: {} -> {}",
                    global_json.display(),
                    version
                );
            }
            None => {
                println!(
                    "Nearest global.json exists but does not declare sdk.version: {}",
                    global_json.display()
                );
            }
        }
    } else {
        println!("Nearest global.json: none");
    }

    let path_entries = env::var("PATH")
        .unwrap_or_default()
        .split(path_separator())
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let shims_in_path = path_entries.iter().position(|entry| entry == &shims_dir);
    match shims_in_path {
        Some(index) => println!("PATH contains dver shims at position {}.", index + 1),
        None => println!("PATH does not contain the dver shims directory. Run 'dver setup'."),
    }

    if let Some(active_dotnet) = resolve_dotnet_on_path() {
        println!("Active 'dotnet' on PATH: {}", active_dotnet.display());
        if active_dotnet.parent() == Some(shims_dir.as_path()) {
            println!("The active dotnet command is managed by dver.");
        } else {
            println!("The active dotnet command is not managed by dver.");
        }
    } else {
        println!("No 'dotnet' command was found on PATH.");
    }

    if let Some(selection) = resolve_version_selection(&current_dir)? {
        match selection.source {
            VersionSource::LocalGlobalJson(path) => {
                println!(
                    "dver will resolve dotnet from local global.json: {} ({})",
                    selection.requested_version,
                    path.display()
                );
            }
            VersionSource::DefaultAlias => {
                println!(
                    "dver will resolve dotnet from the default managed version: {}",
                    selection.requested_version
                );
            }
        }

        let managed_root = dver_root
            .join("versions")
            .join(&selection.requested_version);
        if managed_dotnet_path(&managed_root).exists() {
            println!(
                "Resolved managed dotnet exists at {}.",
                managed_root.display()
            );
        } else {
            println!(
                "The requested version {} is not installed under dver. Run 'dver install {}'.",
                selection.requested_version, selection.requested_version
            );
        }
    } else {
        println!("dver has no local or default version selected.");
    }

    Ok(())
}

fn resolve_dotnet_on_path() -> Option<PathBuf> {
    let command = if cfg!(windows) { "where" } else { "which" };
    let output = Command::new(command).arg("dotnet").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(PathBuf::from)
}
