#!/bin/bash
# dver Installation Script for macOS and Linux
# Usage: ./install.sh [path-to-binary-or-archive]
#
# If no argument is provided, looks for dver binary in current directory.
# Supports .zip, .tar.gz, .tar files, or raw binary.

set -e

INSTALL_DIR="/usr/local/bin"
BINARY_NAME="dver"

print_step() { echo -e "\n\033[1;34m==>\033[0m \033[1m$1\033[0m"; }
print_ok() { echo -e "\033[1;32m✓\033[0m $1"; }
print_error() { echo -e "\033[1;31m✗\033[0m $1" >&2; }

# Detect OS and architecture
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)
    
    case "$ARCH" in
        x86_64) ARCH="x64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *) print_error "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    
    case "$OS" in
        darwin) OS="macos" ;;
        linux) OS="linux" ;;
        *) print_error "Unsupported OS: $OS"; exit 1 ;;
    esac
    
    echo "$OS-$ARCH"
}

# Extract archive if needed
extract_binary() {
    local input="$1"
    local tmpdir=$(mktemp -d)
    
    if [[ ! -f "$input" ]]; then
        print_error "File not found: $input"
        exit 1
    fi
    
    case "$input" in
        *.tar.gz|*.tgz)
            print_step "Extracting tar.gz archive..."
            tar -xzf "$input" -C "$tmpdir"
            ;;
        *.tar)
            print_step "Extracting tar archive..."
            tar -xf "$input" -C "$tmpdir"
            ;;
        *.zip)
            print_step "Extracting zip archive..."
            unzip -q "$input" -d "$tmpdir"
            ;;
        *)
            # Assume it's the raw binary
            cp "$input" "$tmpdir/$BINARY_NAME"
            ;;
    esac
    
    # Find the binary in extracted files
    local binary=$(find "$tmpdir" -type f -name "$BINARY_NAME" | head -1)
    if [[ -z "$binary" ]]; then
        binary=$(find "$tmpdir" -type f -perm +111 | head -1)
    fi
    
    if [[ -z "$binary" ]]; then
        print_error "Could not find $BINARY_NAME binary in archive"
        rm -rf "$tmpdir"
        exit 1
    fi
    
    echo "$binary"
}

# Main installation
main() {
    print_step "Installing dver..."
    
    local platform=$(detect_platform)
    print_ok "Detected platform: $platform"
    
    # Find input file
    local input="$1"
    if [[ -z "$input" ]]; then
        # Look for dver in current directory
        if [[ -f "./$BINARY_NAME" ]]; then
            input="./$BINARY_NAME"
        elif [[ -f "./dver-$platform.tar.gz" ]]; then
            input="./dver-$platform.tar.gz"
        elif [[ -f "./dver-$platform.zip" ]]; then
            input="./dver-$platform.zip"
        else
            print_error "No binary or archive found. Usage: $0 [path-to-binary-or-archive]"
            exit 1
        fi
    fi
    
    print_ok "Using: $input"
    
    # Extract if needed
    local binary=$(extract_binary "$input")
    
    # Make executable
    chmod +x "$binary"
    
    # Install to system location
    print_step "Installing to $INSTALL_DIR..."
    
    if [[ -w "$INSTALL_DIR" ]]; then
        cp "$binary" "$INSTALL_DIR/$BINARY_NAME"
    else
        print_step "Requesting sudo access..."
        sudo cp "$binary" "$INSTALL_DIR/$BINARY_NAME"
        sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi
    
    # Cleanup
    if [[ "$binary" == /tmp/* ]]; then
        rm -rf "$(dirname "$binary")"
    fi
    
    # Verify installation
    print_step "Verifying installation..."
    if command -v dver &> /dev/null; then
        local version=$(dver --version 2>/dev/null || echo "unknown")
        print_ok "dver installed successfully: $version"
    else
        print_ok "Binary installed to $INSTALL_DIR/$BINARY_NAME"
        echo "    You may need to add $INSTALL_DIR to your PATH"
    fi
    
    # Run setup with intercept
    print_step "Running dver setup --intercept..."
    "$INSTALL_DIR/$BINARY_NAME" setup --intercept || true
    
    echo ""
    echo "════════════════════════════════════════════════════════════════"
    echo "  Installation complete!"
    echo "  Restart your terminal or run: source ~/.zshrc (or ~/.bashrc)"
    echo "════════════════════════════════════════════════════════════════"
}

main "$@"
