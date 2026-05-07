use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show the currently active dotnet version
    Current,
    /// List managed SDK versions installed by dver
    List,
    /// Select a managed SDK version
    Use {
        /// Managed SDK version to select
        version: Option<String>,
        /// Set the default managed version instead of writing a local global.json
        #[arg(long, short)]
        global: bool,
        /// Remove the local global.json or the default managed version when used with --global
        #[arg(long)]
        clear: bool,
    },
    /// Install a managed .NET SDK version or channel
    Install {
        /// Version or channel to install, for example 8.0.406, 8.0, STS
        version: Option<String>,
        /// Backward-compatible alias for the positional version argument
        #[arg(long = "version", hide = true)]
        version_flag: Option<String>,
        /// Explicit channel to install, for example 8.0 or STS
        #[arg(long)]
        channel: Option<String>,
        /// Install the latest LTS SDK
        #[arg(long)]
        lts: bool,
    },
    /// Uninstall a managed SDK version
    Uninstall {
        /// Managed SDK version to uninstall
        version: Option<String>,
        /// Backward-compatible alias for the positional version argument
        #[arg(long = "version", hide = true)]
        version_flag: Option<String>,
        /// Remove every managed SDK version and dver PATH configuration
        #[arg(long)]
        all: bool,
    },
    /// Diagnose PATH and SDK resolution issues
    Doctor,
    /// Create the dver dotnet shim and add it to PATH
    Setup {
        /// Undo what `dver setup` did: remove the PATH entry, PowerShell profile hook, and shim directory
        #[arg(long)]
        remove: bool,
    },
}
