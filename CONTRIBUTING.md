# Contributing to PenEnv

Thank you for your interest in contributing to PenEnv! This is an educational project focused on learning GTK4, Rust, and building practical penetration testing tools.

## Development Setup

### Prerequisites

1. **Rust Toolchain**:
   - Rust 1.70 or later
   - Cargo (comes with Rust)

2. **System Dependencies**:
   - GTK4 development libraries
   - libadwaita development libraries
   - VTE4 (Virtual Terminal Emulator) development libraries

   **Install on Fedora/RHEL**:
   ```bash
   sudo dnf install gtk4-devel libadwaita-devel vte291-gtk4-devel
   ```

   **Install on Ubuntu/Debian**:
   ```bash
   sudo apt install libgtk-4-dev libadwaita-1-dev libvte-2.91-gtk4-dev
   ```

   **Install on Arch Linux**:
   ```bash
   sudo pacman -S gtk4 libadwaita vte4
   ```

### Clone and Build

```bash
git clone https://github.com/undergroundbiscuitclub/penenv.git
cd penenv
cargo build
```

### Run in Development Mode

```bash
cargo run
```

This will compile and launch PenEnv with debug symbols and more verbose error messages.

## Project Structure

```
penenv/
├── src/
│   ├── main.rs           - Entry point, GTK application initialization
│   └── gtk_app.rs        - Main application UI, tabs, editors, terminals
├── images/
│   ├── penenv-icon.png   - Application icon (256x256)
│   ├── penenv-icon.svg   - Application icon (scalable)
│   └── screenshot.png    - Application screenshot
├── debian/               - Debian package configuration
│   ├── control
│   ├── rules
│   └── changelog
├── Cargo.toml           - Rust dependencies and project metadata
├── penenv.spec          - RPM package specification
├── penenv.desktop       - Desktop entry file
├── build-packages.sh    - Script to build DEB and RPM packages
├── install.sh           - User installation script
├── commands.yaml        - Built-in command templates (embedded in binary)
├── README.md            - User documentation
├── CONTRIBUTING.md      - This file
└── LICENSE              - MIT license
```

### Key Source Files

- **`main.rs`**: Application entry point, GTK app initialization, and CSS styling
- **`gtk_app.rs`**: Main UI implementation including:
  - Tab management (Targets, Notes, Command Log, Shell tabs)
  - Text editors with syntax highlighting (using ropey)
  - VTE4 terminal integration for bash shells
  - Command drawer with search functionality
  - Custom command management
  - Settings dialog
  - System monitoring (CPU, RAM, Network)
  - Split view mode (notes + shell side-by-side)

## Code Style

- Follow standard Rust formatting: `cargo fmt`
- Run linter before submitting: `cargo clippy`
- Keep functions focused and single-purpose
- Add doc comments for public APIs
- Use descriptive variable names
- Add inline comments for complex logic

## Rust Dependencies

PenEnv uses the following crates (automatically managed by Cargo):

- **gtk4** (0.9) - GTK4 Rust bindings
- **libadwaita** (0.7) - Adwaita styling and widgets
- **vte4** (0.8) - Terminal emulator widget
- **ropey** (1.6) - Efficient text rope for editor
- **chrono** (0.4) - Date and time handling for command logging
- **serde** (1.0) - Serialization/deserialization
- **serde_yaml** (0.9) - YAML file handling for commands and settings
- **sysinfo** (0.32) - System monitoring (CPU, RAM, network)

## Development Workflow

### Making Changes

1. Fork the repository
2. Create a feature branch: `git checkout -b feature-name`
3. Make your changes
4. Format code: `cargo fmt`
5. Check for issues: `cargo clippy`
6. Test thoroughly (see manual testing checklist below)
7. Commit with clear, descriptive messages
8. Push to your fork
9. Create a pull request

### Testing

Currently, the project uses manual testing. Before submitting a PR, verify:

- [ ] Application launches without errors
- [ ] All tabs render correctly (Targets, Notes, Log, Shell, Split View)
- [ ] Tab navigation works (`Ctrl+1` through `Ctrl+9`)
- [ ] File operations work (save, load, auto-save)
- [ ] Text editing in Targets and Notes tabs works
- [ ] Markdown syntax highlighting displays correctly
- [ ] Shell tabs execute commands properly
- [ ] Command logging writes to `commands.log` with timestamps
- [ ] Command drawer opens/closes and search works
- [ ] Custom commands can be added, edited, and deleted
- [ ] Target selector dropdown and popup work
- [ ] Target insertion works in shells and notes
- [ ] Split view mode displays both panels correctly
- [ ] System monitors display correct values
- [ ] Settings persist between sessions
- [ ] Tab renaming works (double-click)
- [ ] No crashes on edge cases (empty files, large files, invalid input)
- [ ] Application closes cleanly

### Building Packages

Test DEB and RPM package creation:

```bash
./build-packages.sh
```

Verify the packages install and run correctly on target distributions.

## Adding Features

### Adding New UI Elements

The main UI is in `gtk_app.rs`. To add new widgets or tabs:

1. Add widget creation in the appropriate section (e.g., `setup_notebook()`)
2. Connect signal handlers for user interactions
3. Update state management if needed
4. Test across different screen sizes and themes

### Adding Command Templates

Built-in commands are in `commands.yaml` at the project root:

```yaml
- name: "Nmap Full Scan"
  command: "nmap -A -T4 {target}"
  description: "Aggressive scan with OS detection"
  category: "Network Scanning"
```

Custom commands are stored in `~/.config/penenv/custom_commands.yaml` and managed through the Settings dialog.

### Extending System Monitoring

System monitors are in `gtk_app.rs` using the `sysinfo` crate. To add new monitors:

1. Add monitor initialization in `setup_system_monitors()`
2. Implement update logic in the periodic update callback
3. Add visibility toggle in settings dialog
4. Persist settings in `~/.config/penenv/settings.yaml`

### Improving Text Editor

The text editor uses `ropey` for efficient text manipulation. Enhancements might include:

- Additional syntax highlighting rules
- Auto-completion
- Find/replace functionality
- Line numbers
- Code folding

### Keyboard Shortcuts

Shortcuts are defined in `gtk_app.rs`:

- Global shortcuts in the main event controller
- Tab-specific shortcuts in individual tab handlers
- Customizable shortcuts stored in settings.yaml

## Feature Ideas

Potential enhancements (see README.md for full list):

- **Session Management**: Save/restore entire workspace state
- **Command History**: Navigate command history with Ctrl+R
- **tmux Integration**: Integrate with tmux for advanced multiplexing
- **Export Functionality**: Export notes and logs to various formats
- **Search**: Global search across all tabs
- **Themes**: Custom color schemes and themes
- **Plugins**: Extension system for user-created features
- **Remote Targets**: SSH integration for remote system testing
- **Automated Tests**: Unit and integration testing

## Bug Reports and Issues

When reporting bugs, please include:

1. **PenEnv version**: Check `Cargo.toml` or package version
2. **Operating system**: Distribution and version
3. **GTK version**: `gtk4-demo --version` or `pkg-config --modversion gtk4`
4. **Steps to reproduce**: Clear, numbered steps
5. **Expected behavior**: What should happen
6. **Actual behavior**: What actually happens
7. **Error messages**: Full error output or logs
8. **Screenshots**: If relevant to UI issues

## Pull Request Guidelines

Good pull requests:

- **Focus on one feature/fix**: Keep changes atomic
- **Include description**: Explain what and why
- **Reference issues**: Link related issue numbers
- **Pass linting**: Clean `cargo clippy` output
- **Maintain style**: Consistent with existing code
- **Add comments**: Explain complex logic
- **Test thoroughly**: Follow manual testing checklist

## Questions and Discussion

- Open an issue for feature discussion before major work
- Check existing issues to avoid duplicates
- Be respectful and constructive in all interactions

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

## Acknowledgments

Thank you for helping make PenEnv better! Educational projects like this thrive on community contributions.
