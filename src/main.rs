mod cli;
mod commands;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use std::path::Path;
use std::process::Command;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os();
    let program_name = args.next();

    if is_dotnet_shim_invocation(program_name.as_deref()) {
        let exit_code = commands::shim::handle_dotnet_shim(args)?;
        std::process::exit(exit_code);
    }

    if args
        .next()
        .as_deref()
        .is_some_and(|arg| arg == "__internal_shim")
    {
        let exit_code = commands::shim::handle_dotnet_shim(args)?;
        std::process::exit(exit_code);
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Current => {
            handle_current_command()?;
        }
        Commands::List => {
            handle_list_command()?;
        }
        Commands::ListRemote => {
            handle_list_remote_command().await?;
        }
        Commands::Use {
            version,
            global,
            clear,
        } => {
            commands::use_cmd::handle_use(version.clone(), *global, *clear).await?;
        }
        Commands::Install {
            version,
            version_flag,
            channel,
            lts,
            force,
        } => {
            let request = version
                .clone()
                .or_else(|| version_flag.clone())
                .or_else(|| channel.clone());
            commands::install::handle_install(*lts, request, *force).await?;
        }
        Commands::Uninstall {
            version,
            version_flag,
            all,
            system,
        } => {
            let request = version.clone().or_else(|| version_flag.clone());
            commands::uninstall::handle_uninstall(request, *all, *system).await?;
        }
        Commands::Doctor => {
            commands::doctor::run_doctor_checks()?;
        }
        Commands::Setup { remove } => {
            if *remove {
                commands::setup::remove_setup()?;
            } else {
                commands::setup::move_to_top_of_path()?;
            }
        }
    }

    Ok(())
}

fn is_dotnet_shim_invocation(program_name: Option<&std::ffi::OsStr>) -> bool {
    let Some(program_name) = program_name else {
        return false;
    };

    let stem = Path::new(program_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    stem.eq_ignore_ascii_case("dotnet")
}

fn handle_current_command() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = crate::utils::common::current_working_dir()?;

    if let Some(selection) = crate::utils::sdk::resolve_version_selection(&current_dir)? {
        let source = match &selection.source {
            crate::utils::sdk::VersionSource::LocalGlobalJson(path) => {
                format!("global.json ({})", path.display())
            }
            crate::utils::sdk::VersionSource::DefaultAlias => "default managed version".to_string(),
        };

        match crate::utils::sdk::resolve_managed_sdk(&selection.requested_version) {
            Ok(sdk) => {
                let output = Command::new(crate::utils::common::managed_dotnet_path(&sdk.root))
                    .arg("--version")
                    .env("DOTNET_ROOT", &sdk.root)
                    .env("DOTNET_MULTILEVEL_LOOKUP", "0")
                    .output()?;

                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    print_table_section(
                        "Current .NET SDK",
                        &["Kind", "Version", "Source", "Path"],
                        &[vec![
                            "managed".to_string(),
                            version.trim().to_string(),
                            source,
                            sdk.root.display().to_string(),
                        ]],
                    );
                    return Ok(());
                }
            }
            Err(_) => {
                print_table_section(
                    "Current .NET SDK",
                    &["Kind", "Version", "Source", "Path"],
                    &[vec![
                        "missing".to_string(),
                        selection.requested_version.clone(),
                        source,
                        "-".to_string(),
                    ]],
                );
                println!(
                    "Selected SDK {} is not installed. Run 'dver install {}'.",
                    selection.requested_version, selection.requested_version
                );
                return Ok(());
            }
        }
    }

    let Some(system_dotnet) = crate::utils::sdk::find_system_dotnet_on_path() else {
        return Err(
            "No managed SDK is selected and no system dotnet was found on PATH. Run 'dver install <version>'."
                .into(),
        );
    };

    let output = Command::new(&system_dotnet).arg("--version").output()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        print_table_section(
            "Current .NET SDK",
            &["Kind", "Version", "Source", "Path"],
            &[vec![
                "system".to_string(),
                version.trim().to_string(),
                "active PATH resolution".to_string(),
                system_dotnet.display().to_string(),
            ]],
        );
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "Failed to determine the active dotnet version: {}",
        stderr.trim()
    )
    .into())
}

fn handle_list_command() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = crate::utils::common::current_working_dir()?;
    let managed_sdks = crate::utils::sdk::list_managed_sdks()?;
    let system_sdks = crate::utils::sdk::list_system_sdks()?;
    let selected_version = crate::utils::sdk::resolve_version_selection(&current_dir)?
        .map(|selection| selection.requested_version);
    let default_version = crate::utils::sdk::get_default_version()?;

    let system_versions = system_sdks
        .iter()
        .map(|sdk| sdk.version.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let managed_versions = managed_sdks
        .iter()
        .map(|sdk| sdk.version.clone())
        .collect::<std::collections::BTreeSet<_>>();

    let managed_rows = if managed_sdks.is_empty() {
        vec![vec![
            "none".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
        ]]
    } else {
        managed_sdks
            .into_iter()
            .map(|sdk| {
                let mut markers = Vec::new();
                if selected_version.as_deref() == Some(&sdk.version) {
                    markers.push("selected");
                }
                if default_version.as_deref() == Some(&sdk.version) {
                    markers.push("default");
                }
                if system_versions.contains(&sdk.version) {
                    markers.push("also in system");
                }

                vec![
                    sdk.version,
                    if markers.is_empty() {
                        "-".to_string()
                    } else {
                        markers.join(", ")
                    },
                    "dver".to_string(),
                    sdk.root.display().to_string(),
                ]
            })
            .collect::<Vec<_>>()
    };
    print_table_section(
        "Managed .NET SDK versions",
        &["Version", "Status", "Owner", "Location"],
        &managed_rows,
    );

    if managed_rows.len() == 1 && managed_rows[0][0] == "none" {
        println!("Tip: run 'dver install 8.0.406' to install one.");
    }

    println!();
    let system_rows = if system_sdks.is_empty() {
        vec![vec![
            "none detected".to_string(),
            "-".to_string(),
            "-".to_string(),
        ]]
    } else {
        system_sdks
            .into_iter()
            .map(|sdk| {
                let status = if managed_versions.contains(&sdk.version) {
                    "duplicated by dver".to_string()
                } else {
                    "-".to_string()
                };
                vec![sdk.version, status, sdk.location.display().to_string()]
            })
            .collect::<Vec<_>>()
    };
    print_table_section(
        "System/global .NET SDK versions",
        &["Version", "Status", "Location"],
        &system_rows,
    );

    let duplicates = crate::utils::sdk::find_duplicated_versions()?;
    if !duplicates.is_empty() {
        println!();
        for version in &duplicates {
            println!("Duplicate: SDK {version} is installed twice (dver + system). Keep one:");
            println!("  dver uninstall {version}            removes the dver-managed copy (recommended if you rely on the system install)");
            println!("  dver uninstall {version} --system   removes the system copy (needs an Administrator terminal)");
        }
    }

    Ok(())
}

async fn handle_list_remote_command() -> Result<(), Box<dyn std::error::Error>> {
    let channels = crate::utils::releases::list_remote_channels().await?;
    let managed_versions = crate::utils::sdk::list_managed_sdks()?
        .into_iter()
        .map(|sdk| sdk.version)
        .collect::<std::collections::BTreeSet<_>>();
    let system_versions = crate::utils::sdk::list_system_sdks()?
        .into_iter()
        .map(|sdk| sdk.version)
        .collect::<std::collections::BTreeSet<_>>();

    let rows = channels
        .into_iter()
        .map(|channel| {
            let latest_sdk = channel.latest_sdk.unwrap_or_else(|| "-".to_string());
            let mut markers = Vec::new();
            if managed_versions.contains(&latest_sdk) {
                markers.push("installed (dver)");
            }
            if system_versions.contains(&latest_sdk) {
                markers.push("installed (system)");
            }

            vec![
                channel.channel,
                latest_sdk,
                channel.release_type.unwrap_or_else(|| "-".to_string()),
                channel.support_phase.unwrap_or_else(|| "-".to_string()),
                channel.eol_date.unwrap_or_else(|| "-".to_string()),
                if markers.is_empty() {
                    "-".to_string()
                } else {
                    markers.join(", ")
                },
            ]
        })
        .collect::<Vec<_>>();

    print_table_section(
        "Available .NET channels (releases-index)",
        &["Channel", "Latest SDK", "Type", "Phase", "EOL", "Status"],
        &rows,
    );
    println!("Install with: dver install <channel|version>, e.g. dver install 8.0 or dver install 8.0.423");

    Ok(())
}

fn print_table_section(title: &str, headers: &[&str], rows: &[Vec<String>]) {
    let table = build_table(headers, rows);
    println!(
        "{}",
        color(&center_text(title, table.width), TableStyle::Title)
    );
    for line in table.lines {
        println!("{line}");
    }
}

fn build_table(headers: &[&str], rows: &[Vec<String>]) -> TableRender {
    let mut widths = headers
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index >= widths.len() {
                widths.push(cell.len());
            } else {
                widths[index] = widths[index].max(cell.len());
            }
        }
    }

    let separator = build_separator(&widths);
    let mut lines = Vec::new();
    lines.push(color(&separator, TableStyle::Border));
    lines.push(build_row(
        headers.iter().map(|value| value.to_string()).collect(),
        &widths,
        true,
    ));
    lines.push(color(&separator, TableStyle::Border));
    for row in rows {
        lines.push(build_row(row.clone(), &widths, false));
    }
    lines.push(color(&separator, TableStyle::Border));

    TableRender {
        width: separator.len(),
        lines,
    }
}

fn build_separator(widths: &[usize]) -> String {
    let mut separator = String::from("+");
    for width in widths {
        separator.push_str(&"-".repeat(*width + 2));
        separator.push('+');
    }
    separator
}

fn build_row(cells: Vec<String>, widths: &[usize], is_header: bool) -> String {
    let mut row = color("|", TableStyle::Border);
    for (index, width) in widths.iter().enumerate() {
        let cell = cells.get(index).map(String::as_str).unwrap_or("");
        let padded = format!("{cell:<width$}", width = *width);
        let styled_cell = if is_header {
            color(&padded, TableStyle::Header)
        } else {
            color_cell(index, cell, &padded)
        };

        row.push(' ');
        row.push_str(&styled_cell);
        row.push(' ');
        row.push_str(&color("|", TableStyle::Border));
    }
    row
}

fn color_cell(index: usize, raw: &str, padded: &str) -> String {
    if raw == "none" || raw == "none detected" || raw == "-" {
        return color(padded, TableStyle::Dim);
    }

    if index == 1 && raw.contains("selected") {
        return color(padded, TableStyle::Highlight);
    }

    color(padded, TableStyle::Cell)
}

fn center_text(text: &str, width: usize) -> String {
    if text.len() >= width {
        return text.to_string();
    }

    let left_padding = (width - text.len()) / 2;
    format!("{}{}", " ".repeat(left_padding), text)
}

struct TableRender {
    width: usize,
    lines: Vec<String>,
}

#[derive(Clone, Copy)]
enum TableStyle {
    Title,
    Header,
    Border,
    Cell,
    Highlight,
    Dim,
}

fn color(value: &str, style: TableStyle) -> String {
    let code = match style {
        TableStyle::Title => "1;96",
        TableStyle::Header => "1;97",
        TableStyle::Border => "96",
        TableStyle::Cell => "37",
        TableStyle::Highlight => "1;92",
        TableStyle::Dim => "90",
    };

    format!("\x1b[{code}m{value}\x1b[0m")
}
