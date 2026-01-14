#!/bin/bash
set -e

echo "=== PenEnv Flatpak Build Script ==="
echo ""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Configuration
APP_ID="com.penenv.app"
MANIFEST="com.penenv.app.webkit.json"
GNOME_VERSION="48"
RUST_SDK_VERSION="24.08"
BUILD_DIR="build-dir"
REPO_DIR="repo"
BUNDLE_NAME="penenv.flatpak"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Check if flatpak is installed
if ! command -v flatpak &> /dev/null; then
    error "flatpak is not installed. Please install flatpak first."
fi

# Check if org.flatpak.Builder is installed (preferred method on Silverblue)
if flatpak info org.flatpak.Builder &> /dev/null; then
    FLATPAK_BUILDER="flatpak run org.flatpak.Builder"
    info "Using org.flatpak.Builder Flatpak"
elif command -v flatpak-builder &> /dev/null; then
    FLATPAK_BUILDER="flatpak-builder"
    info "Using system flatpak-builder"
else
    warn "flatpak-builder not found. Installing org.flatpak.Builder..."
    flatpak install -y flathub org.flatpak.Builder
    FLATPAK_BUILDER="flatpak run org.flatpak.Builder"
fi

# Install required runtimes and SDKs
info "Checking for required Flatpak runtimes..."

install_if_missing() {
    local ref="$1"
    if ! flatpak info "$ref" &> /dev/null; then
        info "Installing $ref..."
        flatpak install -y flathub "$ref"
    else
        info "$ref is already installed"
    fi
}

install_if_missing "org.gnome.Platform//${GNOME_VERSION}"
install_if_missing "org.gnome.Sdk//${GNOME_VERSION}"
install_if_missing "org.freedesktop.Sdk.Extension.rust-stable//${RUST_SDK_VERSION}"

# Check for cargo-sources.json
if [ ! -f "cargo-sources.json" ]; then
    echo ""
    warn "cargo-sources.json not found. Attempting to generate..."

    # Download flatpak-cargo-generator if needed
    if [ ! -f "flatpak-cargo-generator.py" ]; then
        info "Downloading flatpak-cargo-generator.py..."
        curl -sL https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py -o flatpak-cargo-generator.py
    fi

    # Check for required Python modules
    if python3 -c "import tomlkit" 2>/dev/null; then
        info "Generating cargo-sources.json..."
        python3 flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json
    else
        echo ""
        error "Missing Python module 'tomlkit'. Please run:
    pip install --user tomlkit

Then run this script again, or manually generate cargo-sources.json:
    python3 flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json"
    fi
fi

# Check manifest exists
if [ ! -f "$MANIFEST" ]; then
    error "Manifest file $MANIFEST not found!"
fi

echo ""
info "Building Flatpak..."
echo ""

# Clean previous build artifacts
rm -rf "$BUILD_DIR" "$REPO_DIR"

# Build the Flatpak and export to repo
$FLATPAK_BUILDER --force-clean --repo="$REPO_DIR" --install-deps-from=flathub "$BUILD_DIR" "$MANIFEST"

echo ""
info "Creating distributable bundle: $BUNDLE_NAME"
echo ""

# Create the distributable .flatpak bundle file
flatpak build-bundle "$REPO_DIR" "$BUNDLE_NAME" "$APP_ID"

# Also install locally
echo ""
info "Installing locally for testing..."
read -p "Do you want to install the Flatpak locally for testing? (y/N) " -n 1 -r
echo ""
if [[ $REPLY =~ ^[Yy]$ ]]; then
    flatpak --user install -y --reinstall "$REPO_DIR" "$APP_ID"
else
    info "Skipping local installation."
fi

echo ""
echo "=============================================="
echo -e "${GREEN}Build complete!${NC}"
echo "=============================================="
echo ""
echo "Distributable bundle created: $BUNDLE_NAME"
echo ""
echo "Others can install it with:"
echo "    flatpak install $BUNDLE_NAME"
echo ""
echo "Run the app with:"
echo "    flatpak run $APP_ID"
echo ""
