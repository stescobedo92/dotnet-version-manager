use crate::utils::platform;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub async fn resolve_sdk_version(channel: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!(
        "https://dotnetcli.azureedge.net/dotnet/Sdk/{}/latest.version",
        channel
    );
    let client = Client::new();
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to resolve version for channel {}: HTTP {}",
            channel,
            response.status()
        )
        .into());
    }

    let version = response.text().await?;
    Ok(version.trim().to_string())
}

pub async fn download_and_extract(
    version: &str,
    install_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let rid = platform::get_rid();
    let ext = platform::get_archive_extension();
    let filename = format!("dotnet-sdk-{}-{}.{}", version, rid, ext);
    let url = format!(
        "https://dotnetcli.azureedge.net/dotnet/Sdk/{}/{}",
        version, filename
    );

    println!("Downloading .NET SDK {} ({}) from Azure...", version, rid);

    let client = Client::new();
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download SDK: HTTP {} ({})",
            response.status(),
            url
        )
        .into());
    }

    let total_size = response.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
        .progress_chars("#>-"));

    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(&filename);

    // Scope for file writing to ensure it's closed before extraction
    {
        let mut file = File::create(&temp_file_path)?;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item?;
            file.write_all(&chunk)?;
            pb.inc(chunk.len() as u64);
        }
    }

    pb.finish_with_message("Download complete");

    println!("Extracting to {:?}...", install_dir);
    if !install_dir.exists() {
        fs::create_dir_all(install_dir)?;
    }

    extract_archive(&temp_file_path, install_dir, ext)?;

    // Cleanup
    let _ = fs::remove_file(&temp_file_path);

    Ok(())
}

fn extract_archive(
    archive_path: &Path,
    target_dir: &Path,
    ext: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(archive_path)?;

    if ext == "zip" {
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(target_dir)?;
    } else {
        let tar = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(target_dir)?;
    }

    Ok(())
}
