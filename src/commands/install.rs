use crate::commands::setup;
use crate::utils::downloader::{self, InstallRequest};
use crate::utils::sdk::{get_default_version, get_managed_sdk, set_default_version};

pub async fn handle_install(
    lts: bool,
    version_or_channel: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = resolve_install_request(lts, version_or_channel)?;

    if let InstallRequest::Version(version) = &request {
        let normalized = crate::utils::common::normalize_version_input(version);
        if get_managed_sdk(&normalized)?.is_some() {
            let short = use_hint_selector(&normalized);
            println!("Managed .NET SDK {normalized} is already installed.");
            println!("Run 'dver use {short}' to select it (or 'dver use {normalized}').");
            return Ok(());
        }
    }

    println!("Installing .NET SDK via the official dotnet installer...");
    println!("Request: {}", request.label());

    let installed_version = downloader::install_sdk(request).await?;
    setup::ensure_shims_exist()?;

    let was_first_install = get_default_version()?.is_none();
    if was_first_install {
        set_default_version(&installed_version)?;
        println!("Default managed version set to {installed_version}.");
    }

    let short = use_hint_selector(&installed_version);
    println!();
    println!("Next steps:");
    println!("  dver use {short}                 # write ./global.json (project-local)");
    println!("  dver use --global {short}        # set as the default managed SDK");
    if short != installed_version {
        println!(
            "  (tip: '{short}' is a fuzzy alias for {installed_version} — \
             '{installed_version}' also works)"
        );
    }
    if was_first_install {
        println!("  dver setup                      # let dver provide the active 'dotnet' command");
    }

    Ok(())
}

fn use_hint_selector(version: &str) -> String {
    version
        .split('.')
        .next()
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| version.to_string())
}

fn resolve_install_request(
    lts: bool,
    version_or_channel: Option<String>,
) -> Result<InstallRequest, Box<dyn std::error::Error>> {
    if let Some(value) = version_or_channel {
        let normalized = crate::utils::common::normalize_version_input(&value);

        if let Some(alias) = match_channel_alias(&normalized) {
            return Ok(InstallRequest::Channel(alias.to_string()));
        }

        if crate::utils::common::is_probably_specific_version(&normalized) {
            return Ok(InstallRequest::Version(normalized));
        }

        if let Some(channel) = expand_short_channel(&normalized) {
            return Ok(InstallRequest::Channel(channel));
        }

        return Ok(InstallRequest::Channel(normalized));
    }

    if lts {
        return Ok(InstallRequest::Channel("LTS".to_string()));
    }

    Ok(InstallRequest::Channel("LTS".to_string()))
}

fn match_channel_alias(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "lts" => Some("LTS"),
        "sts" => Some("STS"),
        "latest" | "current" | "stable" => Some("Current"),
        _ => None,
    }
}

fn expand_short_channel(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    if value.contains('.') {
        return None;
    }
    if value.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{value}.0"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(input: &str) -> InstallRequest {
        resolve_install_request(false, Some(input.to_string())).unwrap()
    }

    fn channel(req: &InstallRequest) -> &str {
        match req {
            InstallRequest::Channel(c) => c.as_str(),
            InstallRequest::Version(v) => panic!("expected channel, got version {v}"),
        }
    }

    fn version(req: &InstallRequest) -> &str {
        match req {
            InstallRequest::Version(v) => v.as_str(),
            InstallRequest::Channel(c) => panic!("expected version, got channel {c}"),
        }
    }

    #[test]
    fn lts_alias_is_case_insensitive() {
        assert_eq!(channel(&req("lts")), "LTS");
        assert_eq!(channel(&req("LTS")), "LTS");
    }

    #[test]
    fn sts_alias_is_case_insensitive() {
        assert_eq!(channel(&req("sts")), "STS");
    }

    #[test]
    fn latest_and_current_map_to_current_channel() {
        assert_eq!(channel(&req("latest")), "Current");
        assert_eq!(channel(&req("current")), "Current");
    }

    #[test]
    fn bare_major_expands_to_minor_zero_channel() {
        assert_eq!(channel(&req("8")), "8.0");
        assert_eq!(channel(&req("9")), "9.0");
    }

    #[test]
    fn major_minor_passes_through_as_channel() {
        assert_eq!(channel(&req("8.0")), "8.0");
    }

    #[test]
    fn exact_version_resolves_to_version_request() {
        assert_eq!(version(&req("8.0.406")), "8.0.406");
        assert_eq!(version(&req("v8.0.406")), "8.0.406");
    }

    #[test]
    fn no_arg_defaults_to_lts() {
        let r = resolve_install_request(false, None).unwrap();
        assert_eq!(channel(&r), "LTS");
    }
}
