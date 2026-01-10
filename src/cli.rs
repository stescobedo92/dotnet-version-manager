use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Get current dotnet version
    Current,
    /// List available SDK versions
    List,
    /// Set SDK version
    Use { 
        /// SDK version to set (e.g., 8.0.100)
        version: String 
    },
    /// Check if dotnet is installed and install if not
    Install {
        /// Install LTS version
        #[arg(long)]
        lts: bool,
        /// Specific version to install
        #[arg(long)]
        version: Option<String>,
        /// The path to install the SDK to
        #[arg(long)]
        install_path: Option<String>,
    },
    Uninstall {
        /// Version to uninstall. Can be a full version like 8.0.406 or a major version like 8
        version: Option<String>,
        /// Remove all installed SDK versions managed by dotnet
        #[arg(long)]
        all: bool,
    },
    /// Check for common issues
    Doctor,
}
