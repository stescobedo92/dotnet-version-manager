pub fn path_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

/// RID used by Microsoft release metadata (e.g. win-x64, linux-arm64, osx-arm64).
pub fn dotnet_rid() -> String {
    let os = if cfg!(windows) {
        "win"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        "arm" => "arm",
        other => other,
    };

    format!("{os}-{arch}")
}
