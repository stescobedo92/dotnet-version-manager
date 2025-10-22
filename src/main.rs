use clap::{Args, Parser, Subcommand};
use reqwest::{self, header};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::{self, remove_dir_all, remove_file, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const RELEASE_INDEX_URL: &str =
    "https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/releases-index.json";
const USER_AGENT: &str = "dver/0.2 (https://github.com/stescobedo92/dotnet-version-manager)";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Get current dotnet version
    Current,
    /// List installed SDK versions
    List,
    /// Set SDK version via a global.json file
    Use(UseArgs),
    /// Install .NET SDK versions
    Install(InstallArgs),
    /// Uninstall installed .NET SDK versions
    Uninstall(UninstallArgs),
    /// Show the installation path for SDK versions
    Which(WhichArgs),
    /// List available remote release channels and versions
    Remote(RemoteArgs),
    /// Check for common issues
    Doctor,
}

#[derive(Args, Debug)]
struct UseArgs {
    /// Version to set in the global.json file
    version: String,
    /// Optional rollForward strategy
    #[arg(long)]
    roll_forward: Option<String>,
    /// Allow prerelease SDK versions when resolving
    #[arg(long)]
    allow_prerelease: bool,
    /// Destination directory or file for the global.json file
    #[arg(long)]
    path: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct InstallArgs {
    /// Install the latest Long-Term Support channel
    #[arg(long, conflicts_with_all = ["version", "channel"])]
    lts: bool,
    /// Install a specific SDK version
    #[arg(long, conflicts_with_all = ["lts", "channel"])]
    version: Option<String>,
    /// Install from a specific channel (e.g. 8.0)
    #[arg(long, conflicts_with_all = ["lts", "version"])]
    channel: Option<String>,
    /// Install directory for the SDK
    #[arg(long)]
    install_path: Option<PathBuf>,
    /// Install runtime components instead of the SDK
    #[arg(long)]
    runtime: Option<String>,
    /// Target architecture (x64, arm64, ...)
    #[arg(long)]
    architecture: Option<String>,
    /// Skip non-versioned files during install
    #[arg(long)]
    skip_non_versioned_files: bool,
}

impl InstallArgs {
    fn requests_specific_install(&self) -> bool {
        self.lts
            || self.version.is_some()
            || self.channel.is_some()
            || self.runtime.is_some()
            || self.install_path.is_some()
            || self.architecture.is_some()
            || self.skip_non_versioned_files
    }
}

#[derive(Args, Debug)]
struct UninstallArgs {
    /// Version to uninstall. Can be a full version or a major version like 8
    version: Option<String>,
    /// Remove all installed SDK versions managed by dotnet
    #[arg(long, conflicts_with = "version")]
    all: bool,
}

#[derive(Args, Debug)]
struct WhichArgs {
    /// Version to search for. When omitted, all installed versions are shown
    version: Option<String>,
}

#[derive(Args, Debug)]
struct RemoteArgs {
    /// Filter by channel version (for example, 8.0)
    #[arg(long)]
    channel: Option<String>,
    /// Include preview channels in the output
    #[arg(long)]
    include_preview: bool,
    /// Show the most recent releases for each channel (0 disables fetching release lists)
    #[arg(long, default_value_t = 0)]
    show_releases: usize,
}

#[derive(Debug, Clone)]
struct InstalledSdk {
    version: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ReleasesIndex {
    #[serde(rename = "releases-index")]
    releases_index: Vec<ReleaseChannel>,
}

#[derive(Debug, Deserialize, Clone)]
struct ReleaseChannel {
    #[serde(rename = "channel-version")]
    channel_version: String,
    #[serde(rename = "latest-sdk")]
    latest_sdk: Option<String>,
    #[serde(rename = "latest-runtime")]
    latest_runtime: Option<String>,
    #[serde(rename = "support-phase")]
    support_phase: String,
    #[serde(rename = "releases.json")]
    releases_json: String,
}

#[derive(Debug, Deserialize)]
struct ChannelReleases {
    #[serde(default)]
    releases: Vec<ChannelRelease>,
}

#[derive(Debug, Deserialize)]
struct ChannelRelease {
    #[serde(rename = "release-version")]
    release_version: Option<String>,
    sdk: Option<ReleaseSdk>,
}

#[derive(Debug, Deserialize)]
struct ReleaseSdk {
    version: Option<String>,
}

fn get_home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var("HOMEDRIVE").ok()?;
                let path = std::env::var("HOMEPATH").ok()?;
                Some(PathBuf::from(format!("{drive}{path}")))
            })
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn is_dotnet_installed() -> bool {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn list_installed_sdks() -> Result<Vec<InstalledSdk>, Box<dyn Error>> {
    let output = Command::new("dotnet")
        .args(["--list-sdks"])
        .output()
        .map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => {
                io::Error::new(io::ErrorKind::NotFound, "dotnet command not found in PATH")
            }
            _ => err,
        })?;

    if !output.status.success() {
        return Err("Failed to list SDKs".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sdks = Vec::new();
    for line in stdout.lines() {
        if let Some((ver_part, path_part)) = line.split_once('[') {
            let version = ver_part
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            let base = path_part.trim().trim_end_matches(']').trim();
            if version.is_empty() || base.is_empty() {
                continue;
            }
            let mut pb = PathBuf::from(base);
            pb.push(&version);
            sdks.push(InstalledSdk { version, path: pb });
        }
    }

    Ok(sdks)
}

async fn download_install_script() -> Result<PathBuf, Box<dyn Error>> {
    let script_url = if cfg!(windows) {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.ps1"
    } else {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.sh"
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let response = client
        .get(script_url)
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download installer script: HTTP {}",
            response.status()
        )
        .into());
    }

    let script_content = response.bytes().await?;

    let mut file_path = std::env::temp_dir();
    let script_name = if cfg!(windows) {
        "dotnet-install.ps1"
    } else {
        "dotnet-install.sh"
    };
    let unique = format!("{}_{}", script_name, std::process::id());
    file_path.push(unique);

    let mut file = File::create(&file_path)?;
    file.write_all(&script_content)?;

    if !cfg!(windows) {
        let mut perms = fs::metadata(&file_path)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&file_path, perms)?;
        }
    }

    Ok(file_path)
}

fn script_flag(name: &str) -> String {
    if cfg!(windows) {
        format!("-{}", name)
    } else {
        let mut flag = String::from("--");
        for (i, ch) in name.chars().enumerate() {
            if ch.is_uppercase() {
                if i != 0 {
                    flag.push('-');
                }
                flag.push(ch.to_ascii_lowercase());
            } else {
                flag.push(ch);
            }
        }
        flag
    }
}

fn push_script_value<S: AsRef<OsStr>>(command: &mut Command, name: &str, value: S) {
    command.arg(script_flag(name));
    command.arg(value);
}

fn push_script_flag(command: &mut Command, name: &str) {
    command.arg(script_flag(name));
}

async fn install_dotnet(args: &InstallArgs) -> Result<(), Box<dyn Error>> {
    let script_path = download_install_script().await?;

    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("powershell");
        cmd.arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&script_path);
        cmd
    } else {
        let mut cmd = Command::new("bash");
        cmd.arg(&script_path);
        cmd
    };

    if args.lts {
        push_script_value(&mut command, "Channel", OsStr::new("LTS"));
    }

    if let Some(channel) = &args.channel {
        push_script_value(&mut command, "Channel", channel);
    }

    if let Some(version) = &args.version {
        push_script_value(&mut command, "Version", version);
    }

    if let Some(path) = &args.install_path {
        push_script_value(&mut command, "InstallDir", path);
    }

    if let Some(runtime) = &args.runtime {
        push_script_value(&mut command, "Runtime", runtime);
    }

    if let Some(arch) = &args.architecture {
        push_script_value(&mut command, "Architecture", arch);
    }

    if args.skip_non_versioned_files {
        push_script_flag(&mut command, "SkipNonVersionedFiles");
    }

    let output = command.output()?;
    let _ = remove_file(&script_path);

    if !output.status.success() {
        eprintln!(
            "dotnet-install script failed with status: {:?}",
            output.status.code()
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            eprintln!("{}", stderr.trim());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            eprintln!("{}", stdout.trim());
        }
        return Err("dotnet installation failed".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        println!("{}", stdout.trim());
    }

    Ok(())
}

fn find_matching_sdks<'a>(
    sdks: &'a [InstalledSdk],
    version: Option<&str>,
) -> Vec<&'a InstalledSdk> {
    match version {
        Some(v) if v.contains('.') => sdks.iter().filter(|sdk| sdk.version == v).collect(),
        Some(v) => {
            let prefix = format!("{}.", v.trim());
            sdks.iter()
                .filter(|sdk| sdk.version.starts_with(&prefix))
                .collect()
        }
        None => sdks.iter().collect(),
    }
}

fn split_version_parts(version: &str) -> Vec<&str> {
    version
        .split(|c| c == '.' || c == '-')
        .filter(|part| !part.is_empty())
        .collect()
}

fn is_numeric(part: &str) -> bool {
    part.chars().all(|c| c.is_ascii_digit())
}

fn compare_version_part(a: &str, b: &str) -> Ordering {
    match (is_numeric(a), is_numeric(b)) {
        (true, true) => a
            .parse::<u64>()
            .unwrap_or(0)
            .cmp(&b.parse::<u64>().unwrap_or(0)),
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
    }
}

fn compare_versions(a: &str, b: &str) -> Ordering {
    let a_parts = split_version_parts(a);
    let b_parts = split_version_parts(b);

    for (a_part, b_part) in a_parts.iter().zip(b_parts.iter()) {
        let ord = compare_version_part(a_part, b_part);
        if ord != Ordering::Equal {
            return ord;
        }
    }

    match a_parts.len().cmp(&b_parts.len()) {
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => {
            if a_parts[b_parts.len()..].iter().all(|part| is_numeric(part)) {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        Ordering::Less => {
            if b_parts[a_parts.len()..].iter().all(|part| is_numeric(part)) {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }
}

async fn fetch_remote_channels(
    client: &reqwest::Client,
) -> Result<Vec<ReleaseChannel>, Box<dyn Error>> {
    let response = client
        .get(RELEASE_INDEX_URL)
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Failed to fetch releases index: {}", response.status()).into());
    }

    let body = response.json::<ReleasesIndex>().await?;
    Ok(body.releases_index)
}

async fn fetch_channel_releases(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<ChannelRelease>, Box<dyn Error>> {
    let response = client
        .get(url)
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch releases for channel: {}",
            response.status()
        )
        .into());
    }

    let body = response.json::<ChannelReleases>().await?;
    Ok(body.releases)
}

fn run_doctor_checks() {
    println!("Checking for common issues...");

    if is_dotnet_installed() {
        println!("✅ dotnet command is available in your PATH.");
    } else {
        println!(
            "❌ dotnet command not found. Please ensure .NET is installed and the installation directory is in your PATH."
        );
        return;
    }

    if let Some(home_dir) = get_home_dir() {
        let default_dir = if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|p| p.join("Microsoft").join("dotnet"))
        } else {
            Some(home_dir.join(".dotnet"))
        };

        if let Some(dotnet_dir) = default_dir {
            if let Some(path_var) = std::env::var_os("PATH") {
                let in_path = std::env::split_paths(&path_var).any(|p| p == dotnet_dir);
                if in_path {
                    println!("✅ .NET SDK installation directory is in your PATH.");
                } else {
                    println!(
                        "⚠️ .NET SDK installation directory ({}) might not be in your PATH.",
                        dotnet_dir.display()
                    );
                    println!("   Consider adding it to ensure the 'dotnet' command is available everywhere.");
                }
            }
        }
    }

    match std::env::var_os("DOTNET_ROOT") {
        Some(root) => {
            let root_path = PathBuf::from(&root);
            if root_path.exists() {
                println!("✅ DOTNET_ROOT is set to {}.", root_path.display());
            } else {
                println!(
                    "⚠️ DOTNET_ROOT is set to {}, but the directory was not found.",
                    PathBuf::from(root).display()
                );
            }
        }
        None => {
            println!("ℹ️ DOTNET_ROOT environment variable is not set. This is optional but can help tooling locate the SDK.");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Current => match Command::new("dotnet").arg("--version").output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("Current dotnet version: {}", version.trim());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "Failed to get current dotnet version{}{}",
                    if stderr.trim().is_empty() { "" } else { ": " },
                    stderr.trim()
                );
            }
            Err(err) => {
                eprintln!("Failed to run dotnet: {}", err);
            }
        },
        Commands::List => match list_installed_sdks() {
            Ok(sdks) if sdks.is_empty() => {
                println!("No SDK versions were found. Use 'dver install' to add one.");
            }
            Ok(sdks) => {
                let mut versions: Vec<String> = sdks.into_iter().map(|sdk| sdk.version).collect();
                versions.sort_by(|a, b| compare_versions(a, b));
                versions.dedup();
                for version in versions {
                    println!("{}", version);
                }
            }
            Err(err) => {
                eprintln!("{}", err);
            }
        },
        Commands::Use(args) => {
            let mut sdk_section = Map::new();
            sdk_section.insert("version".to_string(), json!(args.version));
            if let Some(roll_forward) = &args.roll_forward {
                sdk_section.insert("rollForward".to_string(), json!(roll_forward));
            }
            if args.allow_prerelease {
                sdk_section.insert("allowPrerelease".to_string(), Value::Bool(true));
            }

            let mut root = Map::new();
            root.insert("sdk".to_string(), Value::Object(sdk_section));
            let json_data = Value::Object(root);

            let file_path = if let Some(path) = &args.path {
                if path.is_dir() || path.extension().is_none() {
                    path.join("global.json")
                } else {
                    path.clone()
                }
            } else {
                std::env::current_dir()?.join("global.json")
            };

            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }

            if file_path.exists() {
                let backup = file_path.with_extension("json.bak");
                let _ = fs::copy(&file_path, &backup);
            }

            let file = File::create(&file_path)?;
            serde_json::to_writer_pretty(file, &json_data)?;
            println!(
                "SDK version set to {} in {}",
                args.version,
                file_path.display()
            );
        }
        Commands::Install(args) => {
            if !is_dotnet_installed() {
                println!("dotnet is not installed. Installing now...");
                if let Err(err) = install_dotnet(args).await {
                    eprintln!("Installation failed: {}", err);
                    return Err(err);
                }
                println!("dotnet installation completed.");
            } else if args.requests_specific_install() {
                println!("Installing requested dotnet components...");
                if let Err(err) = install_dotnet(args).await {
                    eprintln!("Installation failed: {}", err);
                    return Err(err);
                }
                println!("dotnet installation completed.");
            } else {
                println!("dotnet is already installed on your system.");
                if let Ok(output) = Command::new("dotnet").arg("--version").output() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    println!("Current version: {}", version.trim());
                }
                println!("Use --version, --channel or --lts to install additional SDKs.");
            }
        }
        Commands::Uninstall(args) => {
            let sdks = match list_installed_sdks() {
                Ok(list) => list,
                Err(err) => {
                    eprintln!("{}", err);
                    return Ok(());
                }
            };

            if sdks.is_empty() {
                println!("No installed SDK versions found.");
                return Ok(());
            }

            let targets: Vec<&InstalledSdk> = if args.all {
                sdks.iter().collect()
            } else if let Some(version) = &args.version {
                let matches = find_matching_sdks(&sdks, Some(version));
                if matches.is_empty() {
                    println!("No matching SDK versions found for {}.", version);
                }
                matches
            } else {
                eprintln!("Please provide a version or --all to uninstall.");
                Vec::new()
            };

            if targets.is_empty() {
                return Ok(());
            }

            let mut roots: Vec<PathBuf> = sdks
                .iter()
                .filter_map(|sdk| sdk.path.parent().map(|p| p.to_path_buf()))
                .collect();
            roots.sort();
            roots.dedup();

            for sdk in targets {
                let is_under_root = roots.iter().any(|root| sdk.path.starts_with(root));
                if !is_under_root {
                    eprintln!(
                        "Skipping {}: path {} is outside known SDK roots",
                        sdk.version,
                        sdk.path.display()
                    );
                    continue;
                }
                if sdk.path.exists() {
                    match remove_dir_all(&sdk.path) {
                        Ok(_) => println!("Removed {}", sdk.version),
                        Err(err) => eprintln!("Failed to remove {}: {}", sdk.version, err),
                    }
                } else {
                    println!("Directory for {} not found", sdk.version);
                }
            }
        }
        Commands::Which(args) => match list_installed_sdks() {
            Ok(sdks) => {
                let matches = find_matching_sdks(&sdks, args.version.as_deref());
                if matches.is_empty() {
                    if let Some(version) = &args.version {
                        println!("No SDK installations found for {}.", version);
                    } else {
                        println!("No SDK installations were found.");
                    }
                } else {
                    for sdk in matches {
                        println!("{} -> {}", sdk.version, sdk.path.display());
                    }
                }
            }
            Err(err) => {
                eprintln!("{}", err);
            }
        },
        Commands::Remote(args) => {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?;

            let mut channels = fetch_remote_channels(&client).await?;
            if !args.include_preview {
                channels.retain(|c| c.support_phase.to_lowercase() != "preview");
            }

            if let Some(filter) = &args.channel {
                channels.retain(|c| c.channel_version.starts_with(filter));
            }

            if channels.is_empty() {
                println!("No remote channels matched your filters.");
                return Ok(());
            }

            channels.sort_by(|a, b| compare_versions(&b.channel_version, &a.channel_version));

            println!(
                "{:<10} {:<15} {:<18} {}",
                "Channel", "Latest SDK", "Latest Runtime", "Support"
            );
            for channel in &channels {
                println!(
                    "{:<10} {:<15} {:<18} {}",
                    channel.channel_version,
                    channel.latest_sdk.as_deref().unwrap_or("-"),
                    channel.latest_runtime.as_deref().unwrap_or("-"),
                    channel.support_phase
                );
            }

            if args.show_releases > 0 {
                for channel in &channels {
                    println!("\n{} releases:", channel.channel_version);
                    match fetch_channel_releases(&client, &channel.releases_json).await {
                        Ok(releases) => {
                            let mut seen: HashSet<String> = HashSet::new();
                            for release in releases
                                .iter()
                                .filter_map(|r| r.release_version.as_ref().map(|v| (v, &r.sdk)))
                                .take(args.show_releases)
                            {
                                let version_str = release.0;
                                if !seen.insert(version_str.clone()) {
                                    continue;
                                }
                                if let Some(Some(sdk_version)) =
                                    release.1.as_ref().map(|sdk| sdk.version.as_ref())
                                {
                                    println!("  {} (SDK {})", version_str, sdk_version);
                                } else {
                                    println!("  {}", version_str);
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!(
                                "  Failed to fetch releases for channel {}: {}",
                                channel.channel_version, err
                            );
                        }
                    }
                }
            }
        }
        Commands::Doctor => {
            run_doctor_checks();
        }
    }

    Ok(())
}
