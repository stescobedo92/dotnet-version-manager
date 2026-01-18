pub enum Os {
    Windows,
    Linux,
    MacOs,
    Unknown,
}

pub enum Arch {
    X64,
    Arm64,
    X86,
    Unknown,
}

pub fn get_os() -> Os {
    if cfg!(target_os = "windows") {
        Os::Windows
    } else if cfg!(target_os = "linux") {
        Os::Linux
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Unknown
    }
}

pub fn get_arch() -> Arch {
    if cfg!(target_arch = "x86_64") {
        Arch::X64
    } else if cfg!(target_arch = "aarch64") {
        Arch::Arm64
    } else if cfg!(target_arch = "x86") {
        Arch::X86
    } else {
        Arch::Unknown
    }
}

pub fn get_rid() -> String {
    let os_str = match get_os() {
        Os::Windows => "win",
        Os::Linux => "linux",
        Os::MacOs => "osx", // .NET uses osx for macOS RIDs mostly, sometimes others but checks are standard
        Os::Unknown => "unknown",
    };

    let arch_str = match get_arch() {
        Arch::X64 => "x64",
        Arch::Arm64 => "arm64",
        Arch::X86 => "x86",
        Arch::Unknown => "unknown",
    };

    format!("{}-{}", os_str, arch_str)
}

pub fn get_archive_extension() -> &'static str {
    match get_os() {
        Os::Windows => "zip",
        _ => "tar.gz",
    }
}
