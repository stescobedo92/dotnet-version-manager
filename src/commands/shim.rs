use crate::utils::common::{
    current_working_dir, dotnet_binary_name, get_shims_dir, managed_dotnet_path,
};
use crate::utils::platform::path_separator;
use crate::utils::sdk::{resolve_managed_sdk, resolve_version_selection};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn handle_dotnet_shim<I, S>(args: I) -> Result<i32, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let current_dir = current_working_dir()?;

    if let Some(selection) = resolve_version_selection(&current_dir)? {
        let sdk = resolve_managed_sdk(&selection.requested_version)?;
        return run_dotnet(&managed_dotnet_path(&sdk.root), &sdk.root, &args);
    }

    if let Some(system_dotnet) = find_system_dotnet_on_path()? {
        return run_dotnet(
            &system_dotnet,
            system_dotnet.parent().unwrap_or(Path::new("")),
            &args,
        );
    }

    Err(
        "No managed version is selected and no system dotnet command was found. Run 'dver install <version>' or 'dver setup'."
            .into(),
    )
}

fn run_dotnet(
    dotnet_path: &Path,
    dotnet_root: &Path,
    args: &[std::ffi::OsString],
) -> Result<i32, Box<dyn std::error::Error>> {
    let existing_path = env::var("PATH").unwrap_or_default();
    let separator = path_separator().to_string();
    let new_path = if dotnet_root.as_os_str().is_empty() {
        existing_path
    } else if existing_path.is_empty() {
        dotnet_root.display().to_string()
    } else {
        format!("{}{}{}", dotnet_root.display(), separator, existing_path)
    };

    let status = Command::new(dotnet_path)
        .args(args)
        .env("DOTNET_ROOT", dotnet_root)
        .env("DOTNET_MULTILEVEL_LOOKUP", "0")
        .env("PATH", new_path)
        .status()?;

    Ok(status.code().unwrap_or(1))
}

fn find_system_dotnet_on_path() -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let path = env::var("PATH").unwrap_or_default();
    let shims_dir = get_shims_dir();
    let names = if cfg!(windows) {
        vec!["dotnet.exe", "dotnet.cmd", "dotnet.bat"]
    } else {
        vec![dotnet_binary_name()]
    };

    for dir in path.split(path_separator()) {
        if dir.is_empty() {
            continue;
        }

        let directory = PathBuf::from(dir);
        if shims_dir.as_ref().is_some_and(|shim| shim == &directory) {
            continue;
        }

        for name in &names {
            let candidate = directory.join(name);
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }
    }

    Ok(None)
}
