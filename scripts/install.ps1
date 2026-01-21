# dver Installation Script for Windows
# Usage: .\install.ps1 [-BinaryPath <path-to-binary-or-archive>]
#
# Run in PowerShell as Administrator for system-wide install,
# or as regular user for user-only install.

param(
    [string]$BinaryPath
)

$ErrorActionPreference = "Stop"

$BINARY_NAME = "dver.exe"
$INSTALL_DIR_SYSTEM = "$env:ProgramFiles\dver"
$INSTALL_DIR_USER = "$env:LOCALAPPDATA\Programs\dver"

function Write-Step { param($msg) Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Ok { param($msg) Write-Host "✓ $msg" -ForegroundColor Green }
function Write-Err { param($msg) Write-Host "✗ $msg" -ForegroundColor Red }

# Check if running as admin
function Test-Admin {
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    return $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Detect architecture
function Get-Platform {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch ($arch) {
        "AMD64" { return "windows-x64" }
        "ARM64" { return "windows-arm64" }
        default { throw "Unsupported architecture: $arch" }
    }
}

# Extract archive if needed
function Expand-Binary {
    param([string]$InputPath)
    
    if (-not (Test-Path $InputPath)) {
        throw "File not found: $InputPath"
    }
    
    $tempDir = New-Item -ItemType Directory -Path ([System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid().ToString())
    
    if ($InputPath -match "\.zip$") {
        Write-Step "Extracting zip archive..."
        Expand-Archive -Path $InputPath -DestinationPath $tempDir.FullName -Force
    } else {
        # Assume raw binary
        Copy-Item $InputPath -Destination (Join-Path $tempDir.FullName $BINARY_NAME)
    }
    
    # Find the binary
    $binary = Get-ChildItem -Path $tempDir.FullName -Recurse -Filter $BINARY_NAME | Select-Object -First 1
    if (-not $binary) {
        $binary = Get-ChildItem -Path $tempDir.FullName -Recurse -Filter "*.exe" | Select-Object -First 1
    }
    
    if (-not $binary) {
        Remove-Item $tempDir.FullName -Recurse -Force
        throw "Could not find $BINARY_NAME in archive"
    }
    
    return $binary.FullName
}

# Add to PATH
function Add-ToPath {
    param([string]$NewPath, [bool]$IsSystem)
    
    if ($IsSystem) {
        $envPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
        $scope = "Machine"
    } else {
        $envPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $scope = "User"
    }
    
    if ($envPath -notlike "*$NewPath*") {
        $newEnvPath = "$NewPath;$envPath"
        [Environment]::SetEnvironmentVariable("Path", $newEnvPath, $scope)
        Write-Ok "Added $NewPath to $scope PATH"
    } else {
        Write-Ok "$NewPath already in PATH"
    }
    
    # Update current session
    $env:Path = "$NewPath;$env:Path"
}

# Main installation
function Install-Dver {
    Write-Step "Installing dver..."
    
    $platform = Get-Platform
    Write-Ok "Detected platform: $platform"
    
    $isAdmin = Test-Admin
    $installDir = if ($isAdmin) { $INSTALL_DIR_SYSTEM } else { $INSTALL_DIR_USER }
    Write-Ok "Install location: $installDir"
    
    # Find input file
    if (-not $BinaryPath) {
        $currentDir = Get-Location
        if (Test-Path (Join-Path $currentDir $BINARY_NAME)) {
            $BinaryPath = Join-Path $currentDir $BINARY_NAME
        } elseif (Test-Path (Join-Path $currentDir "dver-$platform.zip")) {
            $BinaryPath = Join-Path $currentDir "dver-$platform.zip"
        } else {
            throw "No binary or archive found. Use -BinaryPath parameter."
        }
    }
    
    Write-Ok "Using: $BinaryPath"
    
    # Extract if needed
    $binary = Expand-Binary -InputPath $BinaryPath
    
    # Create install directory
    if (-not (Test-Path $installDir)) {
        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    }
    
    # Copy binary
    Write-Step "Installing to $installDir..."
    Copy-Item $binary -Destination (Join-Path $installDir $BINARY_NAME) -Force
    
    # Cleanup temp files
    $tempDir = Split-Path $binary -Parent
    if ($tempDir -like "*Temp*") {
        Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    
    # Add to PATH
    Write-Step "Configuring PATH..."
    Add-ToPath -NewPath $installDir -IsSystem $isAdmin
    
    # Verify installation
    Write-Step "Verifying installation..."
    $dverPath = Join-Path $installDir $BINARY_NAME
    try {
        $version = & $dverPath --version
        Write-Ok "dver installed successfully: $version"
    } catch {
        Write-Ok "Binary installed to $dverPath"
    }
    
    # Run setup with intercept
    Write-Step "Running dver setup --intercept..."
    try {
        & $dverPath setup --intercept
    } catch {
        Write-Err "Setup failed, you may need to run 'dver setup --intercept' manually"
    }
    
    Write-Host ""
    Write-Host "═══════════════════════════════════════════════════════════════════" -ForegroundColor Yellow
    Write-Host "  Installation complete!" -ForegroundColor Green
    Write-Host "  Restart your terminal for PATH changes to take effect." -ForegroundColor Yellow
    Write-Host "═══════════════════════════════════════════════════════════════════" -ForegroundColor Yellow
}

Install-Dver
