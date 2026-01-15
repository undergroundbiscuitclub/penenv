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

# Install PolicyKit policy file (requires sudo for system-wide installation)
# On immutable systems like Silverblue, /usr is read-only, so we use /etc instead
POLKIT_DIR_USR="/usr/share/polkit-1/actions"
POLKIT_DIR_ETC="/etc/polkit-1/actions"

if [ -f "com.penenv.policy" ]; then
    echo "Installing PolicyKit policy file..."

    # Check if we're on an immutable system (Silverblue, Kinoite, etc.)
    if [ -f "/run/ostree-booted" ]; then
        echo "Detected OSTree-based system (Silverblue/Kinoite)..."
        # Use /etc which is writable on immutable systems
        sudo mkdir -p "$POLKIT_DIR_ETC"
        sudo cp com.penenv.policy "$POLKIT_DIR_ETC/"
        echo "PolicyKit policy installed to $POLKIT_DIR_ETC/com.penenv.policy"
    elif [ -d "$POLKIT_DIR_USR" ] && [ -w "$POLKIT_DIR_USR" ] || sudo test -w "$POLKIT_DIR_USR"; then
        # Standard mutable system with /usr/share writable
        sudo cp com.penenv.policy "$POLKIT_DIR_USR/"
        echo "PolicyKit policy installed to $POLKIT_DIR_USR/com.penenv.policy"
    elif [ -d "$POLKIT_DIR_ETC" ] || sudo mkdir -p "$POLKIT_DIR_ETC" 2>/dev/null; then
        # Fallback to /etc if /usr/share is not available
        sudo cp com.penenv.policy "$POLKIT_DIR_ETC/"
        echo "PolicyKit policy installed to $POLKIT_DIR_ETC/com.penenv.policy"
    else
        echo "⚠️  Could not find a writable PolicyKit directory"
        echo "   You may need to manually install com.penenv.policy for the authentication dialog to work."
        echo "   Try: sudo mkdir -p /etc/polkit-1/actions && sudo cp com.penenv.policy /etc/polkit-1/actions/"
    fi
fi

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

echo "ℹ️  Container management uses PolicyKit for authentication."
echo "   You will see a native GNOME authentication dialog when managing containers."
