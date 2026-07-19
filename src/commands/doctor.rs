#[cfg(windows)]
use crate::commands::setup::{has_windows_profile_hook, windows_powershell_profile_paths};
use crate::utils::common::{
    current_working_dir, get_dver_root, get_shims_dir, managed_dotnet_path,
};
use crate::utils::platform::path_separator;
use crate::utils::sdk::{
    find_duplicated_versions, find_nearest_global_json, get_default_version, list_managed_sdks,
    read_global_json_version, resolve_managed_sdk, resolve_version_selection, VersionSource,
};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub fn run_doctor_checks() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = current_working_dir()?;
    let dver_root = get_dver_root().ok_or("Could not determine dver root")?;
    let shims_dir = get_shims_dir().ok_or("Could not determine dver shims directory")?;

    print_header("dver doctor");

    let progress = ProgressBar::new(7);
    progress.set_style(
        ProgressStyle::with_template("{spinner:.cyan} [{bar:24.cyan/blue}] {pos}/{len} {msg}")
            .expect("valid doctor progress template")
            .progress_chars("##-"),
    );
    progress.enable_steady_tick(Duration::from_millis(80));

    let mut diagnostics = Vec::new();

    progress.set_position(1);
    progress.set_message("Inspecting managed workspace");
    diagnostics.push(check_workspace(&dver_root, &shims_dir)?);

    progress.set_position(2);
    progress.set_message("Inspecting version selection");
    diagnostics.push(check_version_selection(&current_dir)?);

    progress.set_position(3);
    progress.set_message("Inspecting PATH order");
    diagnostics.push(check_path(&shims_dir));

    progress.set_position(4);
    progress.set_message("Inspecting Windows shell hook");
    diagnostics.push(check_shell_hook(&shims_dir));

    progress.set_position(5);
    progress.set_message("Inspecting active dotnet");
    diagnostics.push(check_active_dotnet(&shims_dir));

    progress.set_position(6);
    progress.set_message("Inspecting resolved SDK");
    diagnostics.push(check_resolved_sdk(&current_dir)?);

    progress.set_position(7);
    progress.set_message("Checking for duplicated installations");
    diagnostics.push(check_duplicated_installs()?);

    progress.finish_and_clear();

    let ok_count = diagnostics
        .iter()
        .filter(|item| matches!(item.status, DiagnosticStatus::Ok))
        .count();
    let warn_count = diagnostics
        .iter()
        .filter(|item| matches!(item.status, DiagnosticStatus::Warn))
        .count();
    let fail_count = diagnostics
        .iter()
        .filter(|item| matches!(item.status, DiagnosticStatus::Fail))
        .count();

    println!(
        "{}  {} ok  {} warn  {} fail",
        color("Doctor Summary", Style::Accent),
        color(ok_count.to_string(), Style::Ok),
        color(warn_count.to_string(), Style::Warn),
        color(fail_count.to_string(), Style::Fail)
    );
    println!();

    for diagnostic in &diagnostics {
        print_diagnostic(diagnostic);
    }

    if let Some(next_step) = recommend_next_step(&diagnostics) {
        println!("{}", color("Next step", Style::Accent));
        println!("  {}", next_step);
    }

    Ok(())
}

fn check_workspace(
    dver_root: &PathBuf,
    shims_dir: &PathBuf,
) -> Result<Diagnostic, Box<dyn std::error::Error>> {
    let managed_sdks = list_managed_sdks()?;
    let default_version = get_default_version()?;

    let mut details = vec![
        format!("dver root: {}", dver_root.display()),
        format!("shims dir: {}", shims_dir.display()),
        format!("managed SDK count: {}", managed_sdks.len()),
    ];

    details.push(match default_version {
        Some(version) => format!("default managed version: {version}"),
        None => "default managed version: none".to_string(),
    });

    let status = if managed_sdks.is_empty() {
        DiagnosticStatus::Warn
    } else {
        DiagnosticStatus::Ok
    };

    let summary = if managed_sdks.is_empty() {
        "No managed SDKs are installed yet.".to_string()
    } else {
        format!("{} managed SDK(s) installed.", managed_sdks.len())
    };

    Ok(
        Diagnostic::new("Managed workspace", status, summary, details)
            .with_hint("Use `dver install <version>` if you want dver to manage a specific SDK."),
    )
}

fn check_version_selection(
    current_dir: &PathBuf,
) -> Result<Diagnostic, Box<dyn std::error::Error>> {
    let mut details = vec![format!("current directory: {}", current_dir.display())];

    if let Some(global_json) = find_nearest_global_json(current_dir) {
        match read_global_json_version(&global_json)? {
            Some(version) => {
                details.push(format!("nearest global.json: {}", global_json.display()));
                details.push(format!("requested SDK version: {version}"));

                // A global.json in the home directory (or any ancestor far above
                // the project) silently pins every project under it.
                if is_home_level_global_json(&global_json) {
                    return Ok(Diagnostic::new(
                        "Version selection",
                        DiagnosticStatus::Warn,
                        format!(
                            "A global.json in your home directory is pinning SDK {version} for ALL projects."
                        ),
                        details,
                    )
                    .with_hint(
                        "Move it into the specific project or delete it: it overrides every repository below your home directory.",
                    ));
                }

                return Ok(Diagnostic::new(
                    "Version selection",
                    DiagnosticStatus::Ok,
                    "A local global.json is controlling SDK resolution.".to_string(),
                    details,
                ));
            }
            None => {
                details.push(format!("nearest global.json: {}", global_json.display()));
                return Ok(Diagnostic::new(
                    "Version selection",
                    DiagnosticStatus::Warn,
                    "A global.json exists, but it does not declare sdk.version.".to_string(),
                    details,
                )
                .with_hint("Update the project's global.json or remove it if it is stale."));
            }
        }
    }

    if let Some(default_version) = get_default_version()? {
        details.push(format!("default managed version: {default_version}"));
        return Ok(Diagnostic::new(
            "Version selection",
            DiagnosticStatus::Ok,
            "dver will use the default managed version.".to_string(),
            details,
        ));
    }

    Ok(Diagnostic::new(
        "Version selection",
        DiagnosticStatus::Warn,
        "No local global.json or default managed version is set.".to_string(),
        details,
    )
    .with_hint(
        "Run `dver use <version>` for a project or `dver use <version> --global` for a default.",
    ))
}

fn check_path(shims_dir: &PathBuf) -> Diagnostic {
    let path_entries = env::var("PATH")
        .unwrap_or_default()
        .split(path_separator())
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let mut details = vec![format!("expected shim dir: {}", shims_dir.display())];

    match path_entries
        .iter()
        .position(|entry| paths_equal(entry, shims_dir))
    {
        Some(index) if index == 0 => Diagnostic::new(
            "PATH order",
            DiagnosticStatus::Ok,
            "dver shims are first in PATH.".to_string(),
            {
                details.push("shim precedence: first".to_string());
                details
            },
        ),
        Some(index) => {
            let mut diagnostic = Diagnostic::new(
                "PATH order",
                DiagnosticStatus::Warn,
                format!(
                    "dver shims are present, but only at PATH position {}.",
                    index + 1
                ),
                {
                    details.push(format!("shim precedence: position {}", index + 1));
                    details
                },
            );

            diagnostic = if cfg!(windows) {
                diagnostic.with_hint(
                    "On Windows, user PATH entries often do not beat system dotnet. Use the PowerShell profile hook created by `dver setup` and open a new PowerShell tab.",
                )
            } else {
                diagnostic.with_hint(
                    "Open a new terminal after `dver setup` so the updated PATH is applied.",
                )
            };

            diagnostic
        }
        None => Diagnostic::new(
            "PATH order",
            DiagnosticStatus::Fail,
            "dver shims are not present in PATH.".to_string(),
            details,
        )
        .with_hint("Run `dver setup` and restart your shell."),
    }
}

fn check_shell_hook(shims_dir: &PathBuf) -> Diagnostic {
    #[cfg(windows)]
    {
        let mut details = vec![format!("shim dir: {}", shims_dir.display())];
        let profile_paths = windows_powershell_profile_paths();
        for profile_path in &profile_paths {
            details.push(format!("PowerShell profile: {}", profile_path.display()));
        }

        if has_windows_profile_hook(shims_dir) {
            return Diagnostic::new(
                "PowerShell hook",
                DiagnosticStatus::Ok,
                "The PowerShell profile contains the dver dotnet override.".to_string(),
                details,
            )
            .with_hint("Open a new PowerShell tab to load the updated profile.");
        }

        return Diagnostic::new(
            "PowerShell hook",
            DiagnosticStatus::Fail,
            "The PowerShell profile does not contain the dver dotnet override.".to_string(),
            details,
        )
        .with_hint("Run `dver setup` from PowerShell to install the profile hook.");
    }

    #[cfg(not(windows))]
    {
        let _ = shims_dir;
        Diagnostic::new(
            "Shell hook",
            DiagnosticStatus::Ok,
            "Unix shells rely on PATH + shim scripts, no extra shell hook is required.".to_string(),
            Vec::new(),
        )
    }
}

fn check_active_dotnet(shims_dir: &PathBuf) -> Diagnostic {
    let mut details = Vec::new();
    #[cfg(windows)]
    let profile_hook_installed = has_windows_profile_hook(shims_dir);
    #[cfg(not(windows))]
    let profile_hook_installed = false;

    match resolve_dotnet_on_path() {
        Some(active_dotnet) => {
            details.push(format!("active dotnet: {}", active_dotnet.display()));
            let active_parent = active_dotnet.parent().map(PathBuf::from);
            if active_parent
                .as_ref()
                .is_some_and(|parent| paths_equal(parent, shims_dir))
            {
                Diagnostic::new(
                    "Active dotnet",
                    DiagnosticStatus::Ok,
                    "The active dotnet command is managed by dver.".to_string(),
                    details,
                )
            } else {
                let competing = detect_competing_manager(&active_dotnet);
                if let Some(name) = &competing {
                    details.push(format!("competing manager detected: {name}"));
                }

                let summary = if let Some(name) = &competing {
                    format!(
                        "The active dotnet command comes from another manager ({name}), not dver."
                    )
                } else if cfg!(windows) && profile_hook_installed {
                    "The current shell still resolves dotnet from the system, but new PowerShell tabs should use the dver hook.".to_string()
                } else {
                    "The active dotnet command still comes from the system.".to_string()
                };

                let diagnostic =
                    Diagnostic::new("Active dotnet", DiagnosticStatus::Warn, summary, details);

                if competing.is_some() {
                    diagnostic.with_hint(
                        "Remove the other manager's shims from PATH or put the dver shims first, then open a new shell.",
                    )
                } else if cfg!(windows) {
                    if profile_hook_installed {
                        diagnostic.with_hint(
                            "Close this tab and open a brand-new PowerShell tab. You can verify the hook with `Get-Command dotnet`.",
                        )
                    } else {
                        diagnostic.with_hint(
                            "Run `dver setup` from PowerShell so dver can install the profile hook, then open a new PowerShell tab.",
                        )
                    }
                } else {
                    diagnostic.with_hint(
                        "Restart the terminal or move the dver shim directory to the beginning of PATH.",
                    )
                }
            }
        }
        None => Diagnostic::new(
            "Active dotnet",
            DiagnosticStatus::Fail,
            "No dotnet command was found on PATH.".to_string(),
            details,
        )
        .with_hint("Install a managed SDK with dver or install .NET system-wide."),
    }
}

fn check_resolved_sdk(current_dir: &PathBuf) -> Result<Diagnostic, Box<dyn std::error::Error>> {
    let mut details = Vec::new();

    let Some(selection) = resolve_version_selection(current_dir)? else {
        return Ok(Diagnostic::new(
            "Resolved SDK",
            DiagnosticStatus::Warn,
            "There is no current dver SDK selection to validate.".to_string(),
            details,
        )
        .with_hint(
            "Set a project version with `dver use <version>` or a default with `--global`.",
        ));
    };

    match selection.source {
        VersionSource::LocalGlobalJson(ref path) => {
            details.push(format!("source: global.json ({})", path.display()));
        }
        VersionSource::DefaultAlias => {
            details.push("source: default managed version".to_string());
        }
    }
    details.push(format!(
        "requested version: {}",
        selection.requested_version
    ));

    match resolve_managed_sdk(&selection.requested_version) {
        Ok(sdk) => {
            details.push(format!("resolved version: {}", sdk.version));
            details.push(format!("managed SDK root: {}", sdk.root.display()));
            if managed_dotnet_path(&sdk.root).exists() {
                Ok(Diagnostic::new(
                    "Resolved SDK",
                    DiagnosticStatus::Ok,
                    "The selected SDK exists in the dver managed store.".to_string(),
                    details,
                ))
            } else {
                Ok(Diagnostic::new(
                    "Resolved SDK",
                    DiagnosticStatus::Fail,
                    format!(
                        "Managed SDK {} is registered but the dotnet host is missing.",
                        sdk.version
                    ),
                    details,
                )
                .with_hint(format!("Reinstall with `dver install {}`.", sdk.version)))
            }
        }
        Err(_) => Ok(Diagnostic::new(
            "Resolved SDK",
            DiagnosticStatus::Fail,
            format!(
                "The selected SDK {} is not installed under dver.",
                selection.requested_version
            ),
            details,
        )
        .with_hint(format!(
            "Run `dver install {}` to install the missing SDK.",
            selection.requested_version
        ))),
    }
}

fn check_duplicated_installs() -> Result<Diagnostic, Box<dyn std::error::Error>> {
    let duplicates = find_duplicated_versions()?;

    if duplicates.is_empty() {
        return Ok(Diagnostic::new(
            "Duplicated installs",
            DiagnosticStatus::Ok,
            "No SDK version is installed both by dver and the system.".to_string(),
            Vec::new(),
        ));
    }

    let details = duplicates
        .iter()
        .map(|version| format!("duplicated version: {version} (dver + system)"))
        .collect::<Vec<_>>();
    let first = &duplicates[0];

    Ok(Diagnostic::new(
        "Duplicated installs",
        DiagnosticStatus::Warn,
        format!(
            "{} SDK version(s) are installed twice (dver and system).",
            duplicates.len()
        ),
        details,
    )
    .with_hint(format!(
        "Keep one copy per version. Example: 'dver uninstall {first}' removes the managed copy, or 'dver uninstall {first} --system' removes the system copy (Administrator terminal required)."
    )))
}

fn is_home_level_global_json(global_json: &Path) -> bool {
    let Some(home) = crate::utils::common::get_home_dir() else {
        return false;
    };
    global_json
        .parent()
        .is_some_and(|parent| paths_equal(&parent.to_path_buf(), &home))
}

fn detect_competing_manager(active_dotnet: &PathBuf) -> Option<&'static str> {
    let haystack = normalize_path_key(active_dotnet);
    if haystack.contains("\\.dotver\\") || haystack.contains("/.dotver/") {
        return Some("dotver");
    }
    if haystack.contains("\\fnm\\") || haystack.contains("/fnm/") {
        return Some("fnm");
    }
    if haystack.contains("\\nvm\\") || haystack.contains("/nvm/") {
        return Some("nvm");
    }
    None
}

fn paths_equal(left: &PathBuf, right: &PathBuf) -> bool {
    if left == right {
        return true;
    }

    let left_normalized = normalize_path_key(left);
    let right_normalized = normalize_path_key(right);
    left_normalized == right_normalized
}

fn normalize_path_key(path: &PathBuf) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    while value.ends_with('\\') || value.ends_with('/') {
        value.pop();
    }
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn resolve_dotnet_on_path() -> Option<PathBuf> {
    let command = if cfg!(windows) { "where" } else { "which" };
    let output = Command::new(command).arg("dotnet").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(PathBuf::from)
}

fn recommend_next_step(diagnostics: &[Diagnostic]) -> Option<String> {
    diagnostics
        .iter()
        .find(|item| matches!(item.status, DiagnosticStatus::Fail))
        .and_then(|item| item.hint.clone())
        .or_else(|| {
            diagnostics
                .iter()
                .find(|item| matches!(item.status, DiagnosticStatus::Warn))
                .and_then(|item| item.hint.clone())
        })
}

fn print_header(title: &str) {
    println!("{}", color(&format!("== {title} =="), Style::Accent));
    println!(
        "{}",
        color(
            "Checking managed SDKs, version selection, PATH, and active dotnet resolution.",
            Style::Dim
        )
    );
    println!();
}

fn print_diagnostic(diagnostic: &Diagnostic) {
    println!(
        "{} {}",
        diagnostic.status.label(),
        color(diagnostic.title, diagnostic.status.style())
    );
    println!("  {}", diagnostic.summary);

    for detail in &diagnostic.details {
        println!("  {}", color(detail, Style::Dim));
    }

    if let Some(hint) = &diagnostic.hint {
        println!("  {}", color(format!("Hint: {hint}"), Style::Accent));
    }

    println!();
}

#[derive(Clone, Copy)]
enum DiagnosticStatus {
    Ok,
    Warn,
    Fail,
}

impl DiagnosticStatus {
    fn label(self) -> String {
        match self {
            Self::Ok => color("[OK]", Style::Ok),
            Self::Warn => color("[WARN]", Style::Warn),
            Self::Fail => color("[FAIL]", Style::Fail),
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Ok => Style::Ok,
            Self::Warn => Style::Warn,
            Self::Fail => Style::Fail,
        }
    }
}

struct Diagnostic {
    title: &'static str,
    status: DiagnosticStatus,
    summary: String,
    details: Vec<String>,
    hint: Option<String>,
}

impl Diagnostic {
    fn new(
        title: &'static str,
        status: DiagnosticStatus,
        summary: String,
        details: Vec<String>,
    ) -> Self {
        Self {
            title,
            status,
            summary,
            details,
            hint: None,
        }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

#[derive(Clone, Copy)]
enum Style {
    Ok,
    Warn,
    Fail,
    Accent,
    Dim,
}

fn color(value: impl AsRef<str>, style: Style) -> String {
    let code = match style {
        Style::Ok => "32",
        Style::Warn => "33",
        Style::Fail => "31",
        Style::Accent => "36",
        Style::Dim => "90",
    };

    format!("\x1b[{code}m{}\x1b[0m", value.as_ref())
}
