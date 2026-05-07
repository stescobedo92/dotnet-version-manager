mod cli;
mod commands;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use std::path::Path;
use std::process::Command;
use utils::common::display_width;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        } => {
            let request = version
                .clone()
                .or_else(|| version_flag.clone())
                .or_else(|| channel.clone());
            commands::install::handle_install(*lts, request).await?;
        }
        Commands::Uninstall {
            version,
            version_flag,
            all,
            force,
        } => {
            let request = version.clone().or_else(|| version_flag.clone());
            commands::uninstall::handle_uninstall(request, *all, *force).await?;
        }
        Commands::Doctor => {
            commands::doctor::run_doctor_checks()?;
        }
        Commands::LsRemote { lts, all } => {
            commands::ls_remote::handle_ls_remote(*lts, *all).await?;
        }
        Commands::Setup => {
            commands::setup::move_to_top_of_path()?;
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
        let sdk = crate::utils::sdk::resolve_managed_sdk(&selection.requested_version)?;
        let output = Command::new(crate::utils::common::managed_dotnet_path(&sdk.root))
            .arg("--version")
            .env("DOTNET_ROOT", &sdk.root)
            .env("DOTNET_MULTILEVEL_LOOKUP", "0")
            .output()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let source = match selection.source {
                crate::utils::sdk::VersionSource::LocalGlobalJson(path) => {
                    format!("global.json ({})", path.display())
                }
                crate::utils::sdk::VersionSource::DefaultAlias => {
                    "default managed version".to_string()
                }
            };

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

    let output = Command::new("dotnet").arg("--version").output()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        print_table_section(
            "Current .NET SDK",
            &["Kind", "Version", "Source", "Path"],
            &[vec![
                "system".to_string(),
                version.trim().to_string(),
                "active PATH resolution".to_string(),
                "dotnet".to_string(),
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

    let managed_rows = if managed_sdks.is_empty() {
        vec![vec![
            " ".to_string(),
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

                let arrow = if selected_version.as_deref() == Some(&sdk.version) {
                    "->"
                } else {
                    "  "
                };

                vec![
                    arrow.to_string(),
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
        &["", "Version", "Status", "Owner", "Location"],
        &managed_rows,
    );

    if managed_rows.len() == 1 && managed_rows[0][0] == "none" {
        println!("Tip: run 'dver install 8.0.406' to install one.");
    }

    println!();
    let system_rows = if system_sdks.is_empty() {
        vec![vec!["none detected".to_string(), "-".to_string()]]
    } else {
        system_sdks
            .into_iter()
            .map(|sdk| vec![sdk.version, sdk.location.display().to_string()])
            .collect::<Vec<_>>()
    };
    print_table_section(
        "System/global .NET SDK versions",
        &["Version", "Location"],
        &system_rows,
    );

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
        .map(|header| display_width(header))
        .collect::<Vec<_>>();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let width = display_width(cell);
            if index >= widths.len() {
                widths.push(width);
            } else {
                widths[index] = widths[index].max(width);
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

fn color_cell(_index: usize, raw: &str, padded: &str) -> String {
    if raw == "none" || raw == "none detected" || raw == "-" {
        return color(padded, TableStyle::Dim);
    }

    if raw == "->" || raw.contains("selected") {
        return color(padded, TableStyle::Highlight);
    }

    color(padded, TableStyle::Cell)
}

fn center_text(text: &str, width: usize) -> String {
    let text_width = display_width(text);
    if text_width >= width {
        return text.to_string();
    }

    let left_padding = (width - text_width) / 2;
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
