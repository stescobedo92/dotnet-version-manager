use crate::utils::sdk::list_managed_sdks;
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

const RELEASES_INDEX_URL: &str =
    "https://builds.dotnet.microsoft.com/dotnet/release-metadata/releases-index.json";

#[derive(Debug, Deserialize)]
struct Index {
    #[serde(rename = "releases-index")]
    releases_index: Vec<Channel>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(rename = "channel-version")]
    channel_version: String,
    #[serde(rename = "latest-sdk")]
    latest_sdk: String,
    #[serde(rename = "support-phase")]
    support_phase: String,
    #[serde(rename = "release-type")]
    release_type: String,
    #[serde(rename = "eol-date")]
    eol_date: Option<String>,
}

pub async fn handle_ls_remote(lts_only: bool, include_eol: bool) -> Result<(), Box<dyn std::error::Error>> {
    let installed_sdks = list_managed_sdks()?;
    let installed_majors: HashSet<String> = installed_sdks
        .iter()
        .filter_map(|sdk| sdk.version.split('.').next().map(str::to_string))
        .collect();
    let installed_exact: HashSet<String> = installed_sdks
        .into_iter()
        .map(|sdk| sdk.version)
        .collect();

    let client = reqwest::Client::builder()
        .user_agent(concat!("dver/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client.get(RELEASES_INDEX_URL).send().await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch the .NET releases index: HTTP {}",
            response.status()
        )
        .into());
    }
    let index: Index = response.json().await?;

    println!(
        "{:<3} {:<8} {:<32} {:<6} {:<8} {:<12}",
        "", "Channel", "Latest SDK", "Type", "Phase", "EOL"
    );

    for channel in &index.releases_index {
        if lts_only && !channel.release_type.eq_ignore_ascii_case("lts") {
            continue;
        }
        if !include_eol && channel.support_phase.eq_ignore_ascii_case("eol") {
            continue;
        }

        let major = channel.channel_version.split('.').next().unwrap_or("");
        let marker = if installed_exact.contains(&channel.latest_sdk) {
            "*"
        } else if installed_majors.contains(major) {
            "."
        } else {
            ""
        };

        println!(
            "{:<3} {:<8} {:<32} {:<6} {:<8} {:<12}",
            marker,
            channel.channel_version,
            channel.latest_sdk,
            channel.release_type.to_uppercase(),
            channel.support_phase,
            channel.eol_date.as_deref().unwrap_or("-"),
        );
    }

    println!();
    println!("Legend: '*' latest SDK installed, '.' another SDK in this channel installed");
    println!("Install with: dver install <channel> (e.g. 8.0), dver install <major> (e.g. 8), or dver install lts");

    Ok(())
}
