use crate::utils::common::{
    ensure_dir, get_versions_dir, managed_dotnet_path, normalize_version_input,
};
use crate::utils::platform::{install_script_file_name, install_script_url};
use crate::utils::sdk::get_managed_sdk;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use reqwest::Client;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub enum InstallRequest {
    Version(String),
    Channel(String),
}

impl InstallRequest {
    pub fn label(&self) -> String {
        match self {
            Self::Version(version) => normalize_version_input(version),
            Self::Channel(channel) => channel.trim().to_string(),
        }
    }
}

pub async fn install_sdk(request: InstallRequest) -> Result<String, Box<dyn std::error::Error>> {
    if let InstallRequest::Version(version) = &request {
        let version = normalize_version_input(version);
        if get_managed_sdk(&version)?.is_some() {
            return Ok(version);
        }
    }

    let versions_dir =
        get_versions_dir().ok_or("Could not determine managed versions directory")?;
    ensure_dir(&versions_dir)?;

    let unique_suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let temp_install_dir = versions_dir.join(format!(".tmp-install-{unique_suffix}"));
    let script_path = temp_install_dir.join(install_script_file_name());
    let progress = create_install_progress_bar();

    progress.set_position(1);
    progress.set_message("Preparing managed install workspace");
    ensure_dir(&temp_install_dir)?;

    progress.set_position(2);
    progress.set_message("Downloading official dotnet installer");
    download_install_script(&script_path).await?;

    progress.set_message(format!("Installing {}", request.label()));
    let install_result = run_install_script(&script_path, &request, &temp_install_dir, &progress);
    if let Err(error) = install_result {
        progress.abandon_with_message("Install failed");
        let _ = fs::remove_dir_all(&temp_install_dir);
        return Err(error);
    }

    progress.set_position(5);
    progress.set_message("Finalizing managed SDK layout");
    let installed_version = detect_installed_version(&temp_install_dir)?;
    let final_dir = versions_dir.join(&installed_version);

    if final_dir.exists() {
        fs::remove_dir_all(&temp_install_dir)?;
        progress.finish_with_message(format!("Managed .NET SDK {} is ready", installed_version));
        return Ok(installed_version);
    }

    fs::rename(&temp_install_dir, &final_dir)?;
    progress.finish_with_message(format!("Managed .NET SDK {} is ready", installed_version));
    Ok(installed_version)
}

async fn download_install_script(target_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let response = client.get(install_script_url()).send().await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download the official dotnet-install script: HTTP {}",
            response.status()
        )
        .into());
    }

    let bytes = response.bytes().await?;
    fs::write(target_path, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(target_path, permissions)?;
    }

    Ok(())
}

fn run_install_script(
    script_path: &Path,
    request: &InstallRequest,
    install_dir: &Path,
    progress: &ProgressBar,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut command = if cfg!(windows) {
        let mut command = build_windows_install_command(script_path);
        command.arg("-InstallDir").arg(install_dir).arg("-NoPath");

        match request {
            InstallRequest::Version(version) => {
                command
                    .arg("-Version")
                    .arg(normalize_version_input(version));
            }
            InstallRequest::Channel(channel) => {
                command.arg("-Channel").arg(channel.trim());
            }
        }

        command
    } else {
        let mut command = Command::new("bash");
        command
            .arg(script_path)
            .arg("--install-dir")
            .arg(install_dir)
            .arg("--no-path");

        match request {
            InstallRequest::Version(version) => {
                command
                    .arg("--version")
                    .arg(normalize_version_input(version));
            }
            InstallRequest::Channel(channel) => {
                command.arg("--channel").arg(channel.trim());
            }
        }

        command
    };

    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture installer stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture installer stderr")?;

    let (sender, receiver) = mpsc::channel::<(bool, String)>();

    let stdout_sender = sender.clone();
    let stdout_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = stdout_sender.send((false, line));
        }
    });

    let stderr_sender = sender.clone();
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = stderr_sender.send((true, line));
        }
    });

    drop(sender);

    let mut last_messages = Vec::new();
    while let Ok((is_stderr, line)) = receiver.recv() {
        track_install_progress(progress, &line);

        if should_surface_line(&line, is_stderr) {
            progress.println(line.clone());
        }

        if is_stderr || is_important_line(&line) {
            last_messages.push(line);
            if last_messages.len() > 8 {
                last_messages.remove(0);
            }
        }
    }

    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let status = child.wait()?;
    if !status.success() {
        let details = if last_messages.is_empty() {
            "No additional installer output was captured.".to_string()
        } else {
            last_messages.join("\n")
        };
        return Err(format!("The official dotnet installer script failed.\n{details}").into());
    }

    Ok(())
}

#[cfg(windows)]
fn build_windows_install_command(script_path: &Path) -> Command {
    let mut command = if executable_exists("pwsh") {
        let mut command = Command::new("pwsh");
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass");
        command
    } else {
        let mut command = Command::new("powershell");
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass");
        command
    };

    command.arg("-File").arg(script_path);
    command
}

#[cfg(not(windows))]
fn build_windows_install_command(_script_path: &Path) -> Command {
    unreachable!("Only used on Windows")
}

fn executable_exists(binary: &str) -> bool {
    Command::new(binary)
        .arg("--version")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn detect_installed_version(install_dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let sdk_dir = install_dir.join("sdk");
    if !sdk_dir.exists() {
        return Err("The installer completed but no SDK directory was created".into());
    }

    let mut versions = Vec::new();
    for entry in fs::read_dir(sdk_dir)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }

        let Some(version) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        versions.push(version);
    }

    versions.sort_by(|left, right| crate::utils::common::compare_versions_desc(left, right));

    let version = versions
        .into_iter()
        .next()
        .ok_or("The installer completed but no SDK version folder was found")?;

    let dotnet_path = managed_dotnet_path(install_dir);
    if !dotnet_path.exists() {
        return Err("The installer completed but the dotnet executable is missing".into());
    }

    Ok(version)
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

fn track_install_progress(progress: &ProgressBar, line: &str) {
    let normalized = line.trim();

    if normalized.contains("Downloaded file") {
        progress.set_position(3);
        progress.set_message(downloaded_message(normalized));
    } else if normalized.contains("Extracting the archive") {
        progress.set_position(4);
        progress.set_message("Extracting SDK archive");
    } else if normalized.contains("Installed version is") {
        let version = normalized
            .split("Installed version is")
            .nth(1)
            .map(str::trim)
            .unwrap_or_default();
        progress.set_message(format!("Installed SDK {}", version));
    } else if normalized.contains("Installation finished") {
        progress.set_position(5);
        progress.set_message("Installer finished successfully");
    }
}

fn downloaded_message(line: &str) -> String {
    let bytes = line
        .split("size is")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok());

    match bytes {
        Some(bytes) => format!("Downloaded SDK archive ({})", HumanBytes(bytes)),
        None => "Downloaded SDK archive".to_string(),
    }
}

fn should_surface_line(line: &str, is_stderr: bool) -> bool {
    is_stderr || line.contains("Note that") || line.contains("Warning") || line.contains("Error")
}

fn is_important_line(line: &str) -> bool {
    line.contains("Downloaded file")
        || line.contains("Extracting the archive")
        || line.contains("Installed version is")
        || line.contains("Installation finished")
        || line.contains("Error")
        || line.contains("Warning")
}
