use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs::{self, File, remove_file, remove_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use reqwest::{self, header};
use std::time::{SystemTime, Duration};

#[derive(Parser, Debug)]
#[command(
    name = "dver",
    version,
    about = "A modern .NET SDK version manager",
    long_about = "dver is a CLI tool for managing multiple .NET SDK versions on your system.\nInstall, switch between, and manage .NET SDKs with ease."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Get the current active dotnet version
    Current,

    /// List all installed SDK versions
    #[command(alias = "ls")]
    List {
        /// Show detailed information (size, install date)
        #[arg(short, long)]
        detailed: bool,
    },

    /// Set SDK version for the current directory via global.json
    Use {
        /// The SDK version to use
        version: String
    },

    /// Install a .NET SDK version
    Install {
        /// Install the latest LTS version
        #[arg(long)]
        lts: bool,

        /// Specific version to install
        #[arg(long)]
        version: Option<String>,

        /// Custom installation path
        #[arg(long)]
        install_path: Option<String>,

        /// Use cached installer script (default: true)
        #[arg(long, default_value = "true")]
        use_cache: bool,
    },

    /// Uninstall SDK version(s)
    #[command(alias = "rm")]
    Uninstall {
        /// Version to uninstall (full: 8.0.406, or major: 8)
        version: Option<String>,

        /// Remove all installed SDK versions
        #[arg(long)]
        all: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Show detailed information about an SDK version
    Info {
        /// The SDK version to get info about
        version: String,
    },

    /// Manage cache (info or clear)
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Run diagnostic checks
    Doctor,
}

#[derive(Subcommand, Debug)]
enum CacheAction {
    /// Show cache information
    Info,

    /// Clear all cached files
    Clear,
}

// Optimized: Better error type
#[derive(Debug)]
enum DverError {
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
    Command(String),
    NotFound(String),
    Installation(String),
}

impl std::fmt::Display for DverError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DverError::Io(e) => write!(f, "IO error: {}", e),
            DverError::Http(e) => write!(f, "HTTP error: {}", e),
            DverError::Json(e) => write!(f, "JSON error: {}", e),
            DverError::Command(s) => write!(f, "Command error: {}", s),
            DverError::NotFound(s) => write!(f, "Not found: {}", s),
            DverError::Installation(s) => write!(f, "Installation error: {}", s),
        }
    }
}

impl std::error::Error for DverError {}

impl From<std::io::Error> for DverError {
    fn from(e: std::io::Error) -> Self {
        DverError::Io(e)
    }
}

impl From<reqwest::Error> for DverError {
    fn from(e: reqwest::Error) -> Self {
        DverError::Http(e)
    }
}

impl From<serde_json::Error> for DverError {
    fn from(e: serde_json::Error) -> Self {
        DverError::Json(e)
    }
}

type Result<T> = std::result::Result<T, DverError>;

// Optimized: Better cross-platform home directory handling
fn get_home_dir() -> Result<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    }
    .map(PathBuf::from)
    .ok_or_else(|| DverError::NotFound("Home directory not found".to_string()))
}

// Optimized: Get cache directory with auto-creation
fn get_cache_dir() -> Result<PathBuf> {
    let cache_dir = if cfg!(windows) {
        get_home_dir()?.join("AppData").join("Local").join("dver").join("cache")
    } else {
        get_home_dir()?.join(".cache").join("dver")
    };

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}

// Optimized: Inline and simplified
fn is_dotnet_installed() -> bool {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

// Optimized: Better return type with structured data
#[derive(Debug, Clone)]
struct SdkInfo {
    version: String,
    path: PathBuf,
    size: Option<u64>,
}

// Optimized: Returns structured data instead of just strings
fn list_installed_sdks() -> Result<Vec<SdkInfo>> {
    let output = Command::new("dotnet")
        .args(["--list-sdks"])
        .output()
        .map_err(|e| DverError::Command(e.to_string()))?;

    if !output.status.success() {
        return Err(DverError::Command("Failed to list SDKs".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sdks: Vec<SdkInfo> = stdout
        .lines()
        .filter_map(|line| {
            line.split_once('[')
                .and_then(|(ver_part, path_part)| {
                    let version = ver_part.trim().split_whitespace().next()?.to_string();
                    let base = path_part.trim().trim_end_matches(']').trim();

                    if version.is_empty() || base.is_empty() {
                        return None;
                    }

                    let mut path = PathBuf::from(base);
                    path.push(&version);

                    Some(SdkInfo {
                        version,
                        path,
                        size: None,
                    })
                })
        })
        .collect();

    Ok(sdks)
}

// New: Calculate directory size recursively
fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;

    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if metadata.is_dir() {
                total += calculate_dir_size(&entry.path())?;
            } else {
                total += metadata.len();
            }
        }
    }

    Ok(total)
}

// New: Format bytes to human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

// Optimized: Better cache validation with expiry
const CACHE_EXPIRY_DAYS: u64 = 7;

fn is_cache_valid() -> Result<bool> {
    let cache_path = get_cache_dir()?.join(
        if cfg!(windows) { "dotnet-install.ps1" } else { "dotnet-install.sh" }
    );

    if !cache_path.exists() {
        return Ok(false);
    }

    let metadata = fs::metadata(&cache_path)?;
    let modified = metadata.modified()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::from_secs(u64::MAX));

    Ok(age < Duration::from_secs(CACHE_EXPIRY_DAYS * 24 * 60 * 60))
}

// Optimized: Download with caching support
async fn download_install_script(use_cache: bool) -> Result<PathBuf> {
    if use_cache && is_cache_valid()? {
        println!("Using cached installer script...");
        let cache_path = get_cache_dir()?.join(
            if cfg!(windows) { "dotnet-install.ps1" } else { "dotnet-install.sh" }
        );
        return Ok(cache_path);
    }

    let script_url = if cfg!(windows) {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.ps1"
    } else {
        "https://dotnet.microsoft.com/download/dotnet/scripts/v1/dotnet-install.sh"
    };

    println!("Downloading installer script...");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    let response = client
        .get(script_url)
        .header(header::USER_AGENT, "dver/0.2 (https://github.com/stescobedo92/dotnet-version-manager)")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(DverError::Http(
            reqwest::Error::from(response.error_for_status().unwrap_err())
        ));
    }

    let script_content = response.bytes().await?;

    let file_path = if use_cache {
        let cache_dir = get_cache_dir()?;
        cache_dir.join(
            if cfg!(windows) { "dotnet-install.ps1" } else { "dotnet-install.sh" }
        )
    } else {
        let mut path = std::env::temp_dir();
        let script_name = if cfg!(windows) { "dotnet-install.ps1" } else { "dotnet-install.sh" };
        path.push(format!("{}_{}", script_name, std::process::id()));
        path
    };

    let mut file = File::create(&file_path)?;
    file.write_all(&script_content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&file_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&file_path, perms)?;
    }

    if use_cache {
        println!("Installer script cached for future use.");
    }

    Ok(file_path)
}

// Optimized: Cleaner installation logic
async fn install_dotnet(
    lts: bool,
    version: Option<String>,
    install_path: Option<String>,
    use_cache: bool,
) -> Result<()> {
    let script_path = download_install_script(use_cache).await?;

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

    if lts {
        command.arg("-Channel").arg("LTS");
    } else if let Some(v) = version {
        command.arg("-Version").arg(v);
    }

    if let Some(path) = install_path {
        command.arg("-InstallDir").arg(path);
    }

    println!("Running installer...");
    let output = command.output()?;

    // Cleanup temp file if not using cache
    if !use_cache {
        let _ = remove_file(&script_path);
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        eprintln!("Installation failed:");
        if !stderr.is_empty() {
            eprintln!("{}", stderr.trim());
        }
        if !stdout.is_empty() {
            eprintln!("{}", stdout.trim());
        }

        return Err(DverError::Installation("SDK installation failed".to_string()));
    }

    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("Installation completed successfully!");

    Ok(())
}

// New: Enhanced doctor checks
fn run_doctor_checks() -> Result<()> {
    println!("Running diagnostic checks...\n");

    let mut issues = 0;

    // Check 1: dotnet availability
    if is_dotnet_installed() {
        println!("✓ dotnet command is available");

        // Show current version
        if let Ok(output) = Command::new("dotnet").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("  Current version: {}", version.trim());
            }
        }
    } else {
        println!("✗ dotnet command not found");
        println!("  → Install .NET SDK using: dver install --lts");
        issues += 1;
    }

    // Check 2: PATH configuration
    if let Ok(home) = get_home_dir() {
        let dotnet_dir = home.join(".dotnet");
        if let Ok(path_var) = std::env::var("PATH") {
            let sep = if cfg!(windows) { ';' } else { ':' };

            if path_var.split(sep).any(|p| Path::new(p) == dotnet_dir) {
                println!("✓ .NET SDK directory is in PATH");
            } else {
                println!("⚠ .NET SDK directory may not be in PATH");
                if cfg!(windows) {
                    println!("  → Add: $env:Path += \";$HOME\\.dotnet\"");
                } else {
                    println!("  → Add: export PATH=\"$HOME/.dotnet:$PATH\"");
                }
                issues += 1;
            }
        }
    }

    // Check 3: Installed SDKs
    match list_installed_sdks() {
        Ok(sdks) if !sdks.is_empty() => {
            println!("✓ {} SDK(s) installed", sdks.len());
        }
        Ok(_) => {
            println!("⚠ No SDKs installed");
            issues += 1;
        }
        Err(e) => {
            println!("✗ Failed to list SDKs: {}", e);
            issues += 1;
        }
    }

    // Check 4: Runtimes
    if let Ok(output) = Command::new("dotnet").args(["--list-runtimes"]).output() {
        if output.status.success() {
            let runtimes = String::from_utf8_lossy(&output.stdout);
            let count = runtimes.lines().count();
            if count > 0 {
                println!("✓ {} runtime(s) installed", count);
            } else {
                println!("⚠ No runtimes installed");
            }
        }
    }

    // Check 5: global.json
    if let Ok(current_dir) = std::env::current_dir() {
        let global_json = current_dir.join("global.json");
        if global_json.exists() {
            println!("ℹ global.json found in current directory");

            if let Ok(content) = fs::read_to_string(&global_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(version) = json.get("sdk").and_then(|s| s.get("version")) {
                        println!("  SDK version pinned to: {}", version);
                    }
                }
            }
        }
    }

    println!();
    if issues == 0 {
        println!("All checks passed! ✨");
    } else {
        println!("Found {} issue(s) that may need attention.", issues);
    }

    Ok(())
}

// New: Cache management
fn clear_cache() -> Result<()> {
    let cache_dir = get_cache_dir()?;

    if !cache_dir.exists() {
        println!("Cache directory does not exist.");
        return Ok(());
    }

    let mut removed_count = 0;
    let mut total_size = 0u64;

    for entry in fs::read_dir(&cache_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_file() {
            total_size += metadata.len();
            fs::remove_file(entry.path())?;
            removed_count += 1;
        }
    }

    if removed_count > 0 {
        println!("Removed {} cached file(s), freed {}", removed_count, format_size(total_size));
    } else {
        println!("No cached files to remove.");
    }

    Ok(())
}

fn show_cache_info() -> Result<()> {
    let cache_dir = get_cache_dir()?;

    println!("Cache Information:");
    println!("  Location: {}", cache_dir.display());

    if !cache_dir.exists() {
        println!("  Status: Empty (directory does not exist)");
        return Ok(());
    }

    let mut file_count = 0;
    let mut total_size = 0u64;

    for entry in fs::read_dir(&cache_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_file() {
            file_count += 1;
            total_size += metadata.len();
        }
    }

    println!("  Files: {}", file_count);
    println!("  Total Size: {}", format_size(total_size));

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Current => {
            if !is_dotnet_installed() {
                return Err(DverError::NotFound("dotnet is not installed".to_string()));
            }

            let output = Command::new("dotnet").arg("--version").output()?;

            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("Current .NET SDK version: {}", version.trim());

                // Check for global.json
                if let Ok(current_dir) = std::env::current_dir() {
                    let global_json = current_dir.join("global.json");
                    if global_json.exists() {
                        println!("(pinned by {})", global_json.display());
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Failed to get current dotnet version: {}", stderr.trim());
            }
        }

        Commands::List { detailed } => {
            if !is_dotnet_installed() {
                return Err(DverError::NotFound("dotnet is not installed".to_string()));
            }

            let mut sdks = list_installed_sdks()?;

            if sdks.is_empty() {
                println!("No .NET SDKs found.");
                return Ok(());
            }

            // Sort by version (newest first)
            sdks.sort_by(|a, b| b.version.cmp(&a.version));

            println!("Installed .NET SDKs:\n");

            for mut sdk in sdks {
                if detailed {
                    if let Ok(size) = calculate_dir_size(&sdk.path) {
                        sdk.size = Some(size);
                    }
                }

                println!("  {} {}", "•", sdk.version);
                if detailed {
                    println!("    Path: {}", sdk.path.display());
                    if let Some(size) = sdk.size {
                        println!("    Size: {}", format_size(size));
                    }
                    println!();
                }
            }
        }

        Commands::Use { version } => {
            let json_data = json!({
                "sdk": {
                    "version": version
                }
            });

            let file_path = std::env::current_dir()?.join("global.json");

            // Backup existing file
            if file_path.exists() {
                let backup = file_path.with_extension("json.bak");
                let _ = fs::copy(&file_path, &backup);
                println!("Backed up existing global.json");
            }

            let file = File::create(&file_path)?;
            serde_json::to_writer_pretty(file, &json_data)?;

            println!("✓ SDK version set to {} in {}", version, file_path.display());
        }

        Commands::Install { lts, version, install_path, use_cache } => {
            if is_dotnet_installed() && version.is_none() && !lts {
                println!("dotnet is already installed.");
                if let Ok(output) = Command::new("dotnet").arg("--version").output() {
                    let ver = String::from_utf8_lossy(&output.stdout);
                    println!("Current version: {}", ver.trim());
                }
                println!("\nTo install a specific version:");
                println!("  dver install --version <VERSION>");
                println!("  dver install --lts");
                return Ok(());
            }

            install_dotnet(lts, version, install_path, use_cache).await?;
        }

        Commands::Uninstall { version, all, yes } => {
            let sdks = list_installed_sdks()?;
            let mut roots: Vec<PathBuf> = sdks
                .iter()
                .filter_map(|sdk| sdk.path.parent().map(|p| p.to_path_buf()))
                .collect();
            roots.sort();
            roots.dedup();

            let targets: Vec<SdkInfo> = if all {
                sdks
            } else if let Some(v) = version {
                if v.contains('.') {
                    sdks.into_iter().filter(|sdk| sdk.version == v).collect()
                } else {
                    let prefix = format!("{}.", v);
                    sdks.into_iter().filter(|sdk| sdk.version.starts_with(&prefix)).collect()
                }
            } else {
                println!("Please provide a version or use --all");
                println!("\nExamples:");
                println!("  dver uninstall 8.0.406");
                println!("  dver uninstall 8");
                println!("  dver uninstall --all");
                return Ok(());
            };

            if targets.is_empty() {
                println!("No matching SDK versions found.");
                return Ok(());
            }

            // Show what will be removed
            println!("The following SDKs will be removed:");
            for sdk in &targets {
                println!("  • {} ({})", sdk.version, sdk.path.display());
            }

            if !yes {
                println!("\nProceed? (y/N): ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let mut removed = 0;
            for sdk in targets {
                let is_under_root = roots.iter().any(|r| sdk.path.starts_with(r));
                if !is_under_root {
                    eprintln!("Skipping {}: path is outside known SDK roots", sdk.version);
                    continue;
                }

                if sdk.path.exists() {
                    match remove_dir_all(&sdk.path) {
                        Ok(_) => {
                            println!("✓ Removed {}", sdk.version);
                            removed += 1;
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to remove {}: {}", sdk.version, e);
                        }
                    }
                }
            }

            println!("\nRemoved {} SDK(s).", removed);
        }

        Commands::Info { version } => {
            let sdks = list_installed_sdks()?;

            let mut sdk = sdks.into_iter()
                .find(|s| s.version == version)
                .ok_or_else(|| DverError::NotFound(format!("SDK version {} not found", version)))?;

            println!("\nSDK Information:");
            println!("  Version: {}", sdk.version);
            println!("  Path: {}", sdk.path.display());

            if let Ok(size) = calculate_dir_size(&sdk.path) {
                sdk.size = Some(size);
                println!("  Size: {}", format_size(size));
            }

            if let Ok(metadata) = fs::metadata(&sdk.path) {
                if let Ok(created) = metadata.created() {
                    println!("  Installed: {:?}", created);
                }
            }

            println!();
        }

        Commands::Cache { action } => {
            match action {
                CacheAction::Info => show_cache_info()?,
                CacheAction::Clear => {
                    println!("Clear all cached files? (y/N): ");
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;

                    if input.trim().eq_ignore_ascii_case("y") {
                        clear_cache()?;
                    } else {
                        println!("Cancelled.");
                    }
                }
            }
        }

        Commands::Doctor => {
            run_doctor_checks()?;
        }
    }

    Ok(())
}
