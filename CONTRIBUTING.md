# Contributing to PenEnv

Thank you for your interest in contributing to PenEnv!

## Development Setup

1. **Prerequisites**:
   - Rust 1.70 or later
   - Cargo

2. **Clone and Build**:
   ```bash
   git clone <repository-url>
   cd penenv
   cargo build
   ```

3. **Run Tests**:
   ```bash
   cargo test
   ```

4. **Run in Development Mode**:
   ```bash
   cargo run
   ```

## Project Structure

```
src/
├── main.rs      - Entry point, sets up the application
├── app.rs       - Main application loop, window management, input routing
├── window.rs    - Window abstraction, manages different window types
├── editor.rs    - Text editor implementation using ropey
└── shell.rs     - Shell window, command execution, command logging
```

## Code Style

- Follow standard Rust formatting (use `cargo fmt`)
- Run `cargo clippy` before submitting
- Keep functions focused and well-documented
- Add comments for complex logic

## Adding Features

### Adding a New Window Type

1. Add the type to `WindowType` enum in `window.rs`
2. Implement rendering logic in `Window::render()`
3. Add input handling in `Window::handle_input()`
4. Update `App::new()` in `app.rs` to create the window

### Adding New Keybindings

1. Global shortcuts: Add to `App::handle_input()` in `app.rs`
2. Window-specific: Add to appropriate handler in `window.rs`, `editor.rs`, or `shell.rs`

## Testing

Currently, the project doesn't have automated tests. Manual testing checklist:

- [ ] All windows render correctly
- [ ] Navigation between windows works
- [ ] File saving/loading works
- [ ] Command execution works
- [ ] Command logging works with correct timestamps
- [ ] Target selector inserts correct values
- [ ] Creating/deleting shell windows works
- [ ] No crashes on edge cases (empty files, large files, etc.)

## Submitting Changes

1. Fork the repository
2. Create a feature branch: `git checkout -b feature-name`
3. Make your changes
4. Run `cargo fmt` and `cargo clippy`
5. Test thoroughly
6. Commit with clear messages
7. Push and create a pull request

## Feature Ideas

See the "Future Considerations" section in README.md for ideas:

- Command history navigation
- Session save/restore
- Configurable keybindings
- Syntax highlighting
- Search functionality
- tmux integration

## Questions?

Open an issue for discussion before starting work on major features.
