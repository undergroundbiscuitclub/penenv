#!/bin/bash

# Build script for creating DEB and RPM packages

set -e

# Extract version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -n1 | sed 's/version = "\(.*\)"/\1/')

if [ -z "$VERSION" ]; then
    echo "❌ Failed to extract version from Cargo.toml"
    exit 1
fi

echo "PenEnv Package Builder"
echo "======================"
echo "Version: $VERSION"
echo ""

# Check for required tools
check_deb_tools() {
    if ! command -v dpkg-deb &> /dev/null; then
        echo "❌ dpkg-deb not found. Install with: sudo apt install dpkg-dev"
        return 1
    fi
    echo "✓ DEB tools available"
    return 0
}

check_rpm_tools() {
    if ! command -v rpmbuild &> /dev/null; then
        echo "❌ rpmbuild not found. Install with: sudo dnf install rpm-build"
        return 1
    fi
    echo "✓ RPM tools available"
    return 0
}

# Build DEB package
build_deb() {
    echo ""
    echo "Building DEB package..."
    echo "----------------------"
    
    if ! check_deb_tools; then
        return 1
    fi
    
    # Build the binary
    echo "Building release binary..."
    cargo build --release
    
    # Create package directory structure
    PKG_DIR="target/debian/penenv_${VERSION}-1_amd64"
    mkdir -p "$PKG_DIR/DEBIAN"
    mkdir -p "$PKG_DIR/usr/bin"
    mkdir -p "$PKG_DIR/usr/share/applications"
    mkdir -p "$PKG_DIR/usr/share/icons/hicolor/256x256/apps"
    mkdir -p "$PKG_DIR/usr/share/icons/hicolor/scalable/apps"
    mkdir -p "$PKG_DIR/usr/share/doc/penenv"
    
    # Copy files
    cp target/release/penenv "$PKG_DIR/usr/bin/"
    cp penenv.desktop "$PKG_DIR/usr/share/applications/"
    cp images/penenv-icon.png "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/penenv.png"
    cp images/penenv-icon.svg "$PKG_DIR/usr/share/icons/hicolor/scalable/apps/penenv.svg"
    cp README.md "$PKG_DIR/usr/share/doc/penenv/"
    cp LICENSE "$PKG_DIR/usr/share/doc/penenv/"
    
    # Create control file
    cat > "$PKG_DIR/DEBIAN/control" << EOF
Package: penenv
Version: ${VERSION}-1
Section: utils
Priority: optional
Architecture: amd64
Depends: libgtk-4-1, libadwaita-1-0, libvte-2.91-gtk4-0, bash
Maintainer: undergroundbiscuitclub <noreply@example.com>
Description: Pentesting environment with integrated shells and note-taking
 PenEnv is a modern GTK4 desktop application for managing penetration testing
 environments with integrated shells, note-taking, and target management.
 .
 Features multiple shell tabs with full bash functionality, markdown notes
 with syntax highlighting, and automatic command logging.
EOF
    
    # Create postinst script
    cat > "$PKG_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e
if [ "$1" = "configure" ]; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi
EOF
    
    chmod 755 "$PKG_DIR/DEBIAN/postinst"
    
    # Build package
    dpkg-deb --build "$PKG_DIR"
    
    echo "✅ DEB package created: target/debian/penenv_${VERSION}-1_amd64.deb"
    echo ""
    echo "Install with: sudo dpkg -i target/debian/penenv_${VERSION}-1_amd64.deb"
    echo "             sudo apt-get install -f  # to fix dependencies if needed"
}

# Build RPM package
build_rpm() {
    echo ""
    echo "Building RPM package..."
    echo "----------------------"
    
    if ! check_rpm_tools; then
        return 1
    fi
    
    # Create RPM build structure
    mkdir -p ~/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    
    # Create source tarball
    TARBALL="penenv-${VERSION}.tar.gz"
    
    echo "Creating source tarball..."
    tar --exclude='target' \
        --exclude='.git' \
        --exclude='*.deb' \
        --exclude='*.rpm' \
        --transform "s,^,penenv-${VERSION}/," \
        -czf ~/rpmbuild/SOURCES/$TARBALL \
        *
    
    # Copy spec file and update version
    sed "s/^Version:.*/Version:        $VERSION/" penenv.spec > ~/rpmbuild/SPECS/penenv.spec
    
    # Build RPM
    echo "Building RPM..."
    rpmbuild -bb ~/rpmbuild/SPECS/penenv.spec
    
    # Copy RPM to local directory
    mkdir -p target/rpm
    cp ~/rpmbuild/RPMS/x86_64/penenv-${VERSION}-*.rpm target/rpm/ 2>/dev/null || \
    cp ~/rpmbuild/RPMS/aarch64/penenv-${VERSION}-*.rpm target/rpm/ 2>/dev/null || true
    
    if ls target/rpm/penenv-${VERSION}-*.rpm 1> /dev/null 2>&1; then
        echo "✅ RPM package created: target/rpm/penenv-${VERSION}-*.rpm"
        echo ""
        echo "Install with: sudo dnf install target/rpm/penenv-${VERSION}-*.rpm"
    else
        echo "❌ RPM build may have failed. Check ~/rpmbuild/RPMS/"
    fi
}

# Main menu
echo "Select package type to build:"
echo "1) DEB (Ubuntu/Debian)"
echo "2) RPM (Fedora/RHEL)"
echo "3) Both"
echo ""
read -p "Choice [1-3]: " choice

case $choice in
    1)
        build_deb
        ;;
    2)
        build_rpm
        ;;
    3)
        build_deb
        build_rpm
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac

echo ""
echo "Done! 🎉"
