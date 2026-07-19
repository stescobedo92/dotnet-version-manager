use crate::utils::common::{ensure_dir, get_versions_dir, managed_dotnet_path};
use crate::utils::releases::{resolve_install_target, ResolvedSdkRelease};
use crate::utils::sdk::get_managed_sdk;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha512};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tar::Archive;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub enum InstallRequest {
    Version(String),
    Channel(String),
}

impl InstallRequest {
    pub fn label(&self) -> String {
        match self {
            Self::Version(version) => crate::utils::common::normalize_version_input(version),
            Self::Channel(channel) => channel.trim().to_string(),
        }
    }
}

pub async fn install_sdk(
    request: InstallRequest,
    force: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let progress = create_install_progress_bar();

    progress.set_position(1);
    progress.set_message("Resolving release metadata");
    let release = match resolve_install_target(&request.label()).await {
        Ok(release) => release,
        Err(error) => {
            progress.abandon_with_message("Release resolution failed");
            return Err(error);
        }
    };

    progress.println(format!(
        "Resolved {} -> channel {} / SDK {} ({})",
        request.label(),
        release.channel,
        release.version,
        release.rid
    ));

    if get_managed_sdk(&release.version)?.is_some() {
        progress.finish_with_message(format!(
            "Managed .NET SDK {} is already installed",
            release.version
        ));
        return Ok(release.version);
    }

    if let Ok(system_sdks) = crate::utils::sdk::list_system_sdks() {
        if let Some(system) = system_sdks
            .iter()
            .find(|sdk| sdk.version == release.version)
        {
            if !force {
                progress.abandon_with_message("Install skipped: already installed system-wide");
                return Err(format!(
                    "SDK {} is already installed system-wide at {}.\nNothing to do: the system copy is usable as-is.\nIf you specifically want an isolated dver-managed copy too, re-run with --force:\n  dver install {} --force",
                    system.version,
                    system.location.display(),
                    system.version
                )
                .into());
            }

            progress.println(format!(
                "Note: SDK {} also exists system-wide at {} (installing isolated managed copy because of --force).",
                system.version,
                system.location.display()
            ));
        }
    }

    let versions_dir =
        get_versions_dir().ok_or("Could not determine managed versions directory")?;
    ensure_dir(&versions_dir)?;

    let unique_suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let staging_dir = versions_dir.join(format!(".tmp-install-{unique_suffix}"));
    let archive_path = staging_dir.join(&release.file_name);
    let extract_dir = staging_dir.join("extract");
    let final_dir = versions_dir.join(&release.version);

    ensure_dir(&staging_dir)?;
    ensure_dir(&extract_dir)?;

    let install_result =
        install_resolved_release(&release, &archive_path, &extract_dir, &progress).await;

    if let Err(error) = install_result {
        progress.abandon_with_message("Install failed");
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }

    if final_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
        progress.finish_with_message(format!("Managed .NET SDK {} is ready", release.version));
        return Ok(release.version);
    }

    progress.set_position(5);
    progress.set_message("Finalizing managed SDK layout");
    fs::rename(&extract_dir, &final_dir)?;
    let _ = fs::remove_dir_all(&staging_dir);

    if !managed_dotnet_path(&final_dir).exists() {
        let _ = fs::remove_dir_all(&final_dir);
        return Err("The archive was extracted but the dotnet executable is missing.".into());
    }

    progress.finish_with_message(format!("Managed .NET SDK {} is ready", release.version));
    Ok(release.version)
}

async fn install_resolved_release(
    release: &ResolvedSdkRelease,
    archive_path: &Path,
    extract_dir: &Path,
    progress: &ProgressBar,
) -> Result<(), Box<dyn std::error::Error>> {
    progress.set_position(2);
    progress.set_message(format!("Downloading {}", release.file_name));
    download_file(&release.url, archive_path, progress).await?;

    progress.set_position(3);
    progress.set_message("Verifying archive checksum");
    verify_sha512(archive_path, &release.hash)?;

    progress.set_position(4);
    progress.set_message("Extracting SDK archive");
    extract_archive(archive_path, extract_dir)?;

    if !managed_dotnet_path(extract_dir).exists() {
        return Err(
            "Extraction finished but the dotnet executable was not found in the archive.".into(),
        );
    }

    let _ = fs::remove_file(archive_path);
    Ok(())
}

async fn download_file(
    url: &str,
    target_path: &Path,
    progress: &ProgressBar,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download SDK archive: HTTP {} ({url})",
            response.status()
        )
        .into());
    }

    let total = response.content_length();
    if let Some(total) = total {
        progress.set_length(total.max(1));
        progress.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:28.cyan/blue}] {bytes}/{total_bytes} {msg}",
            )
            .expect("valid download progress template")
            .progress_chars("##-"),
        );
    }

    let mut file = File::create(target_path)?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        progress.set_position(downloaded);
    }

    file.flush()?;
    progress.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:28.cyan/blue}] {pos:>1}/{len:1} {msg}",
        )
        .expect("valid progress template")
        .progress_chars("##-"),
    );
    progress.set_length(5);
    Ok(())
}

fn verify_sha512(path: &Path, expected_hex: &str) -> Result<(), Box<dyn std::error::Error>> {
    let expected = decode_hex(expected_hex)?;
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
    io::copy(&mut file, &mut hasher)?;
    let actual = hasher.finalize();

    if expected.as_slice() != actual.as_slice() {
        return Err(
            "Archive checksum verification failed. The download is corrupt or incomplete.".into(),
        );
    }

    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let normalized = value.trim();
    if normalized.len() % 2 != 0 {
        return Err("Invalid checksum encoding.".into());
    }

    let mut bytes = Vec::with_capacity(normalized.len() / 2);
    let chars = normalized.as_bytes();
    for index in (0..chars.len()).step_by(2) {
        let hex = std::str::from_utf8(&chars[index..index + 2])?;
        bytes.push(u8::from_str_radix(hex, 16)?);
    }
    Ok(bytes)
}

fn extract_archive(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    if name.ends_with(".zip") {
        extract_zip(archive_path, destination)
    } else if name.ends_with(".tar.gz") {
        extract_tar_gz(archive_path, destination)
    } else {
        Err(format!("Unsupported archive format: {name}").into())
    }
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name().map(PathBuf::from) else {
            continue;
        };
        let out_path = destination.join(enclosed);

        if entry.name().ends_with('/') {
            ensure_dir(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            ensure_dir(parent)?;
        }

        let mut outfile = File::create(&out_path)?;
        io::copy(&mut entry, &mut outfile)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

fn extract_tar_gz(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive.unpack(destination)?;
    Ok(())
}

fn create_install_progress_bar() -> ProgressBar {
    let progress = ProgressBar::new(5);
    progress.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:28.cyan/blue}] {pos:>1}/{len:1} {msg}",
        )
        .expect("valid progress template")
        .progress_chars("##-"),
    );
    progress
}
