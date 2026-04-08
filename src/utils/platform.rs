pub fn path_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

pub fn install_script_url() -> &'static str {
    if cfg!(windows) {
        "https://dot.net/v1/dotnet-install.ps1"
    } else {
        "https://dot.net/v1/dotnet-install.sh"
    }
}

pub fn install_script_file_name() -> &'static str {
    if cfg!(windows) {
        "dotnet-install.ps1"
    } else {
        "dotnet-install.sh"
    }
}
