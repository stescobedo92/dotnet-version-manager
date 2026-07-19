use crate::commands::setup;
use crate::utils::downloader::{self, InstallRequest};
use crate::utils::sdk::{get_default_version, set_default_version};

pub async fn handle_install(
    lts: bool,
    version_or_channel: Option<String>,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = resolve_install_request(lts, version_or_channel)?;

    println!("Installing .NET SDK (SDKMAN-style resolve → download → verify → extract)...");
    println!("Request: {}", request.label());

    let installed_version = downloader::install_sdk(request, force).await?;
    setup::ensure_shims_exist()?;

    if get_default_version()?.is_none() {
        set_default_version(&installed_version)?;
        println!("Default managed version set to {installed_version}.");
    }

    println!("Installed managed .NET SDK {installed_version}.");
    println!("Run 'dver use {installed_version}' for this directory (writes global.json).");
    println!("Run 'dver use {installed_version} --global' to set the default (like sdk default).");
    println!("Run 'dver setup' once if you want dver to provide the active 'dotnet' command.");

    Ok(())
}

fn resolve_install_request(
    lts: bool,
    version_or_channel: Option<String>,
) -> Result<InstallRequest, Box<dyn std::error::Error>> {
    if let Some(value) = version_or_channel {
        let normalized = crate::utils::common::normalize_version_input(&value);
        if crate::utils::common::is_probably_specific_version(&normalized) {
            return Ok(InstallRequest::Version(normalized));
        }

        return Ok(InstallRequest::Channel(value.trim().to_string()));
    }

    if lts {
        return Ok(InstallRequest::Channel("LTS".to_string()));
    }

    Ok(InstallRequest::Channel("LTS".to_string()))
}
