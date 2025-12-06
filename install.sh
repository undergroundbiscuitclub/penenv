#!/bin/bash

# PenEnv Installation Script

set -e

echo "Installing PenEnv..."

# Build the application
echo "Building application..."
cargo build --release

# Create directories if they don't exist
mkdir -p ~/.local/bin
mkdir -p ~/.local/share/applications
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
mkdir -p ~/.local/share/icons/hicolor/scalable/apps

# Copy binary
echo "Installing binary to ~/.local/bin/penenv..."
cp target/release/penenv ~/.local/bin/

# Copy icon
echo "Installing icon..."
cp images/penenv-icon.png ~/.local/share/icons/hicolor/256x256/apps/penenv.png
cp images/penenv-icon.svg ~/.local/share/icons/hicolor/scalable/apps/penenv.svg

# Copy desktop file
echo "Installing desktop file..."
cp penenv.desktop ~/.local/share/applications/

# Update desktop database
if command -v update-desktop-database &> /dev/null; then
    echo "Updating desktop database..."
    update-desktop-database ~/.local/share/applications
fi

# Update icon cache
if command -v gtk-update-icon-cache &> /dev/null; then
    echo "Updating icon cache..."
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
fi

echo ""
echo "✅ Installation complete!"
echo ""
echo "PenEnv has been installed to: ~/.local/bin/penenv"
echo "You can now:"
echo "  1. Run 'penenv' from the terminal (make sure ~/.local/bin is in your PATH)"
echo "  2. Launch it from your application menu"
echo "  3. Pin it to your favorites"
echo ""

# Check if ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo "⚠️  Note: ~/.local/bin is not in your PATH"
    echo "   Add this line to your ~/.bashrc or ~/.zshrc:"
    echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi
