use crate::utils::common::{is_probably_specific_version, normalize_version_input};
use crate::utils::platform::dotnet_rid;
use serde::Deserialize;

const RELEASES_INDEX_URL: &str =
    "https://builds.dotnet.microsoft.com/dotnet/release-metadata/releases-index.json";

#[derive(Debug, Clone)]
pub struct ResolvedSdkRelease {
    pub version: String,
    pub channel: String,
    pub url: String,
    pub hash: String,
    pub file_name: String,
    pub rid: String,
}

#[derive(Debug, Deserialize)]
struct ReleasesIndexFile {
    #[serde(rename = "releases-index")]
    releases_index: Vec<ChannelIndexEntry>,
}

#[derive(Debug, Deserialize)]
struct ChannelIndexEntry {
    #[serde(rename = "channel-version")]
    channel_version: String,
    #[serde(rename = "latest-sdk")]
    latest_sdk: Option<String>,
    #[serde(rename = "release-type")]
    release_type: Option<String>,
    #[serde(rename = "support-phase")]
    support_phase: Option<String>,
    #[serde(rename = "eol-date")]
    eol_date: Option<String>,
    #[serde(rename = "releases.json")]
    releases_json: String,
}

#[derive(Debug, Clone)]
pub struct ChannelSummary {
    pub channel: String,
    pub latest_sdk: Option<String>,
    pub release_type: Option<String>,
    pub support_phase: Option<String>,
    pub eol_date: Option<String>,
}

/// Fetches the channel catalog from the releases index (like `sdk list`).
pub async fn list_remote_channels() -> Result<Vec<ChannelSummary>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let index = fetch_releases_index(&client).await?;

    Ok(index
        .releases_index
        .into_iter()
        .map(|entry| ChannelSummary {
            channel: entry.channel_version,
            latest_sdk: entry.latest_sdk,
            release_type: entry.release_type,
            support_phase: entry.support_phase,
            eol_date: entry.eol_date,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct ChannelReleasesFile {
    releases: Vec<ChannelRelease>,
}

#[derive(Debug, Deserialize)]
struct ChannelRelease {
    sdk: Option<SdkReleaseInfo>,
    sdks: Option<Vec<SdkReleaseInfo>>,
}

#[derive(Debug, Deserialize)]
struct SdkReleaseInfo {
    version: Option<String>,
    files: Option<Vec<SdkFile>>,
}

#[derive(Debug, Deserialize)]
struct SdkFile {
    name: Option<String>,
    rid: Option<String>,
    url: Option<String>,
    hash: Option<String>,
}

pub async fn resolve_install_target(
    request_label: &str,
) -> Result<ResolvedSdkRelease, Box<dyn std::error::Error>> {
    let normalized = normalize_version_input(request_label);
    if normalized.is_empty() {
        return Err("A version or channel is required.".into());
    }

    let client = reqwest::Client::new();
    let index = fetch_releases_index(&client).await?;
    let rid = dotnet_rid();

    if is_channel_alias(&normalized) || !is_probably_specific_version(&normalized) {
        let channel = resolve_channel_entry(&index, &normalized)?;
        let version = channel
            .latest_sdk
            .clone()
            .ok_or_else(|| format!("Channel '{}' does not publish a latest SDK.", normalized))?;
        return fetch_sdk_release(&client, &channel, &version, &rid).await;
    }

    let channel_version = channel_version_from_sdk(&normalized)
        .ok_or_else(|| format!("Could not determine the .NET channel for SDK '{normalized}'."))?;
    let channel = index
        .releases_index
        .iter()
        .find(|entry| entry.channel_version == channel_version)
        .ok_or_else(|| {
            format!(
                "No release metadata channel found for '{channel_version}' (from '{normalized}')."
            )
        })?;

    fetch_sdk_release(&client, channel, &normalized, &rid).await
}

async fn fetch_releases_index(
    client: &reqwest::Client,
) -> Result<ReleasesIndexFile, Box<dyn std::error::Error>> {
    let response = client.get(RELEASES_INDEX_URL).send().await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download .NET releases index: HTTP {}",
            response.status()
        )
        .into());
    }

    Ok(response.json().await?)
}

async fn fetch_sdk_release(
    client: &reqwest::Client,
    channel: &ChannelIndexEntry,
    version: &str,
    rid: &str,
) -> Result<ResolvedSdkRelease, Box<dyn std::error::Error>> {
    let response = client.get(&channel.releases_json).send().await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download release metadata for channel {}: HTTP {}",
            channel.channel_version,
            response.status()
        )
        .into());
    }

    let releases: ChannelReleasesFile = response.json().await?;
    let normalized = normalize_version_input(version);

    for release in &releases.releases {
        for sdk in collect_sdk_infos(release) {
            let Some(sdk_version) = sdk.version.as_deref().map(normalize_version_input) else {
                continue;
            };
            if sdk_version != normalized {
                continue;
            }

            let file =
                select_sdk_archive(sdk.files.as_deref().unwrap_or(&[]), rid).ok_or_else(|| {
                    format!("No downloadable SDK archive found for {normalized} on platform {rid}.")
                })?;

            return Ok(ResolvedSdkRelease {
                version: sdk_version,
                channel: channel.channel_version.clone(),
                url: file.url,
                hash: file.hash,
                file_name: file.name,
                rid: rid.to_string(),
            });
        }
    }

    Err(format!(
        "SDK version '{normalized}' was not found in channel {} release metadata.",
        channel.channel_version
    )
    .into())
}

fn collect_sdk_infos(release: &ChannelRelease) -> Vec<&SdkReleaseInfo> {
    let mut infos = Vec::new();
    if let Some(sdk) = &release.sdk {
        infos.push(sdk);
    }
    if let Some(sdks) = &release.sdks {
        infos.extend(sdks.iter());
    }
    infos
}

struct SelectedArchive {
    name: String,
    url: String,
    hash: String,
}

fn select_sdk_archive(files: &[SdkFile], rid: &str) -> Option<SelectedArchive> {
    let mut candidates = files
        .iter()
        .filter_map(|file| {
            let name = file.name.as_deref()?;
            let file_rid = file.rid.as_deref()?;
            let url = file.url.as_deref()?;
            let hash = file.hash.as_deref()?;
            if file_rid != rid {
                return None;
            }
            if !(name.ends_with(".zip") || name.ends_with(".tar.gz")) {
                return None;
            }
            Some(SelectedArchive {
                name: name.to_string(),
                url: url.to_string(),
                hash: hash.to_string(),
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        archive_preference(&left.name).cmp(&archive_preference(&right.name))
    });
    candidates.into_iter().next()
}

fn archive_preference(name: &str) -> u8 {
    if name.ends_with(".zip") {
        0
    } else if name.ends_with(".tar.gz") {
        1
    } else {
        2
    }
}

fn resolve_channel_entry<'a>(
    index: &'a ReleasesIndexFile,
    request: &str,
) -> Result<&'a ChannelIndexEntry, Box<dyn std::error::Error>> {
    let request = request.trim();
    let lower = request.to_ascii_lowercase();

    if lower == "lts" {
        return pick_channel_by_type(index, "lts")
            .ok_or_else(|| "No supported LTS channel was found in the releases index.".into());
    }

    if lower == "sts" || lower == "current" {
        return pick_channel_by_type(index, "sts")
            .ok_or_else(|| "No supported STS channel was found in the releases index.".into());
    }

    index
        .releases_index
        .iter()
        .find(|entry| entry.channel_version == request)
        .ok_or_else(|| {
            format!(
                "Unknown channel '{request}'. Use a channel like 8.0, LTS, STS, or a full SDK version."
            )
            .into()
        })
}

fn pick_channel_by_type<'a>(
    index: &'a ReleasesIndexFile,
    release_type: &str,
) -> Option<&'a ChannelIndexEntry> {
    let mut matches = index
        .releases_index
        .iter()
        .filter(|entry| {
            entry
                .release_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(release_type))
        })
        .filter(|entry| {
            matches!(
                entry.support_phase.as_deref(),
                Some("active") | Some("maintenance")
            )
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        support_phase_rank(left.support_phase.as_deref())
            .cmp(&support_phase_rank(right.support_phase.as_deref()))
            .then_with(|| {
                crate::utils::common::compare_versions_desc(
                    &format!("{}.0", left.channel_version),
                    &format!("{}.0", right.channel_version),
                )
            })
    });

    matches.into_iter().next()
}

fn support_phase_rank(phase: Option<&str>) -> u8 {
    match phase {
        Some("active") => 0,
        Some("maintenance") => 1,
        _ => 2,
    }
}

fn is_channel_alias(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "lts" | "sts" | "current"
    )
}

fn channel_version_from_sdk(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.is_empty() || minor.is_empty() {
        return None;
    }
    Some(format!("{major}.{minor}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_channel_from_sdk_version() {
        assert_eq!(channel_version_from_sdk("8.0.423").as_deref(), Some("8.0"));
        assert_eq!(
            channel_version_from_sdk("10.0.302").as_deref(),
            Some("10.0")
        );
    }

    #[test]
    fn prefers_zip_archives() {
        assert!(
            archive_preference("dotnet-sdk-win-x64.zip")
                < archive_preference("dotnet-sdk-linux-x64.tar.gz")
        );
    }

    #[tokio::test]
    async fn resolves_lts_channel_to_concrete_sdk() {
        let release = resolve_install_target("LTS")
            .await
            .expect("LTS should resolve via releases-index");
        assert!(!release.version.is_empty());
        assert!(!release.url.is_empty());
        assert_eq!(release.hash.len(), 128);
        assert!(
            release.file_name.ends_with(".zip") || release.file_name.ends_with(".tar.gz"),
            "unexpected archive {}",
            release.file_name
        );
    }
}
