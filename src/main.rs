//! PenEnv - Pentesting Environment Manager
//!
//! A modern GTK4 desktop application for managing penetration testing environments
//! with integrated shells, note-taking, target management, and command templates.
//!
//! # Features
//! - Multiple shell tabs with full bash functionality
//! - Command templates drawer with 30+ pre-configured pentesting commands
//! - Split view mode for notes and shell side-by-side
//! - Target management with quick insertion
//! - Automatic command logging
//! - Markdown syntax highlighting for notes
//! - Base directory selection for project organization

mod gtk_app;

use gtk4::prelude::*;
use gtk4::{Application, glib};

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("com.penenv.app")
        .build();

    app.connect_activate(gtk_app::build_ui);

    app.run()
}
