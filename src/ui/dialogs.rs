//! Dialog components for PenEnv
//!
//! Contains settings dialog, command dialogs, and other popups using libadwaita 0.7 widgets.

use gtk4::prelude::*;
use gtk4::{self as gtk, Application, Box as GtkBox, Button, Label, Orientation, Entry,
          ScrolledWindow, ListBox, Frame, CheckButton, Notebook, ComboBoxText, PasswordEntry};
use libadwaita::{self as adw, prelude::*};
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::{
    get_app_settings, save_app_settings, get_keyboard_shortcuts, key_to_display,
    get_text_zoom_scale, get_terminal_zoom_scale, is_command_logging_enabled, zoom,
    is_notes_wrap_text_enabled, get_browser_settings, BrowserSettings, ProxyType,
    is_browser_enabled, is_containers_enabled,
};
use crate::commands::{load_custom_commands, save_custom_command, delete_custom_command,
                      update_custom_command, CommandTemplate};
use crate::container::{
    ContainerConfig, ContainerRuntime, load_container_config, save_container_config,
};

/// Shows the base directory selection dialog
pub fn show_base_dir_dialog<F>(app: &Application, callback: F)
where
    F: Fn(Option<PathBuf>) + 'static,
{
    let dialog = adw::Window::builder()
        .application(app)
        .title("Select Base Directory")
        .modal(true)
        .default_width(500)
        .default_height(250)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(450);

    let dialog_box = GtkBox::new(Orientation::Vertical, 20);
    dialog_box.set_margin_top(24);
    dialog_box.set_margin_bottom(24);
    dialog_box.set_margin_start(24);
    dialog_box.set_margin_end(24);

    // Get current directory
    let current_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    // Header with icon
    let header_box = GtkBox::new(Orientation::Vertical, 12);
    header_box.set_halign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("dim-label");

    let title_label = Label::new(Some("Choose Base Directory"));
    title_label.add_css_class("title-1");

    let desc_label = Label::new(Some(&format!(
        "This directory will store your project files.\n\nCurrent: {}",
        current_dir
    )));
    desc_label.set_wrap(true);
    desc_label.set_justify(gtk::Justification::Center);
    desc_label.add_css_class("dim-label");

    header_box.append(&icon);
    header_box.append(&title_label);
    header_box.append(&desc_label);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    button_box.set_halign(gtk::Align::Center);
    button_box.set_margin_top(12);

    let yes_btn = Button::with_label("Use Current Directory");
    yes_btn.add_css_class("suggested-action");
    yes_btn.add_css_class("pill");

    let browse_btn = Button::with_label("Browse...");
    browse_btn.add_css_class("pill");

    let callback_rc = Rc::new(callback);

    // Yes button handler
    let dialog_clone = dialog.clone();
    let callback_clone = Rc::clone(&callback_rc);
    let current_dir_clone = current_dir.clone();
    yes_btn.connect_clicked(move |_| {
        callback_clone(Some(PathBuf::from(&current_dir_clone)));
        dialog_clone.close();
    });

    // Browse button handler
    let dialog_clone2 = dialog.clone();
    let callback_clone2 = Rc::clone(&callback_rc);
    browse_btn.connect_clicked(move |_| {
        let file_chooser = gtk::FileChooserDialog::builder()
            .title("Select Base Directory")
            .transient_for(&dialog_clone2)
            .action(gtk::FileChooserAction::SelectFolder)
            .build();
        file_chooser.add_button("Cancel", gtk::ResponseType::Cancel);
        file_chooser.add_button("Select", gtk::ResponseType::Accept);

        let dialog_clone3 = dialog_clone2.clone();
        let callback_clone3 = Rc::clone(&callback_clone2);
        file_chooser.connect_response(move |file_chooser, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = file_chooser.file() {
                    if let Some(path) = file.path() {
                        callback_clone3(Some(path));
                        dialog_clone3.close();
                    }
                }
            }
            file_chooser.close();
        });

        file_chooser.show();
    });

    button_box.append(&yes_btn);
    button_box.append(&browse_btn);

    dialog_box.append(&header_box);
    dialog_box.append(&button_box);

    content.set_child(Some(&dialog_box));
    dialog.set_content(Some(&content));
    dialog.present();
}

/// Shows the settings dialog using Notebook tabs compatible with libadwaita 0.7
/// Creates the about page
fn create_about_page() -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(600);

    let page = GtkBox::new(Orientation::Vertical, 24);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);
    page.set_halign(gtk::Align::Center);

    // App icon/name
    let app_name = Label::new(Some("PenEnv"));
    app_name.add_css_class("title-1");
    app_name.set_margin_bottom(4);
    page.append(&app_name);

    // Version
    let version = Label::new(Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    version.add_css_class("title-4");
    version.set_margin_bottom(24);
    page.append(&version);

    // Description
    let description = Label::new(Some(
        "A modern GTK4 desktop application for managing\n\
        penetration testing environments with integrated\n\
        shells, note-taking, and target management."
    ));
    description.set_justify(gtk::Justification::Center);
    description.set_margin_bottom(24);
    page.append(&description);

    // Separator
    let separator1 = gtk::Separator::new(Orientation::Horizontal);
    separator1.set_margin_bottom(24);
    page.append(&separator1);

    // Built with section
    let built_heading = Label::new(Some("Built With"));
    built_heading.add_css_class("title-4");
    built_heading.set_margin_bottom(12);
    page.append(&built_heading);

    let tech_box = GtkBox::new(Orientation::Vertical, 6);
    tech_box.set_halign(gtk::Align::Center);

    let tech_items = vec![
        "GTK4 0.9 - Modern GNOME toolkit",
        "libadwaita 0.7 - GNOME design library",
        "VTE4 0.8 - Virtual Terminal Emulator",
        "Rust - Systems programming language",
    ];

    for item in tech_items {
        let label = Label::new(Some(item));
        label.add_css_class("dim-label");
        tech_box.append(&label);
    }

    page.append(&tech_box);

    // Separator
    let separator2 = gtk::Separator::new(Orientation::Horizontal);
    separator2.set_margin_top(24);
    separator2.set_margin_bottom(24);
    page.append(&separator2);

    // Author
    let author_heading = Label::new(Some("Author"));
    author_heading.add_css_class("title-4");
    author_heading.set_margin_bottom(8);
    page.append(&author_heading);

    let author = Label::new(Some("undergroundbiscuitclub"));
    author.add_css_class("dim-label");
    author.set_margin_bottom(24);
    page.append(&author);

    // License
    let license_heading = Label::new(Some("License"));
    license_heading.add_css_class("title-4");
    license_heading.set_margin_bottom(8);
    page.append(&license_heading);

    let license = Label::new(Some("MIT License"));
    license.add_css_class("dim-label");
    license.set_margin_bottom(24);
    page.append(&license);

    // Disclaimer
    let disclaimer_heading = Label::new(Some("⚠️  Important"));
    disclaimer_heading.add_css_class("title-4");
    disclaimer_heading.set_margin_bottom(8);
    page.append(&disclaimer_heading);

    let disclaimer = Label::new(Some(
        "This software is provided for educational purposes\n\
        and authorized security testing only.\n\n\
        Only use against systems you own or have\n\
        written permission to test."
    ));
    disclaimer.set_justify(gtk::Justification::Center);
    disclaimer.add_css_class("dim-label");
    page.append(&disclaimer);

    content.set_child(Some(&page));
    scrolled.set_child(Some(&content));

    scrolled
}

/// Settings tab indices for use with show_settings_dialog_at_tab
#[allow(dead_code)]
pub mod settings_tabs {
    pub const GENERAL: u32 = 0;
    pub const SHORTCUTS: u32 = 1;
    pub const COMMANDS: u32 = 2;
    pub const BROWSER: u32 = 3;
    pub const CONTAINERS: u32 = 4;
    pub const ABOUT: u32 = 5;
}

pub fn show_settings_dialog(
    parent: &adw::ApplicationWindow,
    cpu_frame: &Frame,
    ram_frame: &Frame,
    net_frame: &Frame
) {
    show_settings_dialog_at_tab(parent, cpu_frame, ram_frame, net_frame, settings_tabs::GENERAL);
}

/// Shows the settings dialog with a specific tab selected
pub fn show_settings_dialog_at_tab(
    parent: &adw::ApplicationWindow,
    cpu_frame: &Frame,
    ram_frame: &Frame,
    net_frame: &Frame,
    tab_index: u32,
) {
    let dialog = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .default_width(600)
        .default_height(550)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // Header bar
    let header_bar = adw::HeaderBar::new();
    main_box.append(&header_bar);

    // Create notebook for tabs (compatible with libadwaita 0.7)
    let notebook = Notebook::new();
    notebook.set_margin_top(6);
    notebook.set_margin_bottom(6);
    notebook.set_margin_start(6);
    notebook.set_margin_end(6);

    // ===== GENERAL TAB =====
    let general_page = create_general_settings_page(cpu_frame, ram_frame, net_frame);
    let general_label = Label::new(Some("General"));
    notebook.append_page(&general_page, Some(&general_label));

    // ===== SHORTCUTS TAB =====
    let shortcuts_page = create_shortcuts_page(parent);
    let shortcuts_label = Label::new(Some("Shortcuts"));
    notebook.append_page(&shortcuts_page, Some(&shortcuts_label));

    // ===== COMMANDS TAB =====
    let commands_page = create_commands_page(parent, &dialog, cpu_frame, ram_frame, net_frame);
    let commands_label = Label::new(Some("Commands"));
    notebook.append_page(&commands_page, Some(&commands_label));

    // ===== BROWSER TAB =====
    let browser_page = create_browser_settings_page();
    let browser_label = Label::new(Some("Browser"));
    notebook.append_page(&browser_page, Some(&browser_label));

    // ===== CONTAINERS TAB =====
    let containers_page = create_containers_settings_page();
    let containers_label = Label::new(Some("Containers"));
    notebook.append_page(&containers_page, Some(&containers_label));

    // ===== ABOUT TAB =====
    let about_page = create_about_page();
    let about_label = Label::new(Some("About"));
    notebook.append_page(&about_page, Some(&about_label));

    // Set the requested tab
    notebook.set_current_page(Some(tab_index));

    main_box.append(&notebook);
    dialog.set_content(Some(&main_box));
    dialog.present();
}

/// Creates the general settings page
fn create_general_settings_page(cpu_frame: &Frame, ram_frame: &Frame, net_frame: &Frame) -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(500);

    let page = GtkBox::new(Orientation::Vertical, 24);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    // Monitor Settings Group
    let monitor_heading = Label::new(Some("System Monitors"));
    monitor_heading.add_css_class("title-4");
    monitor_heading.set_halign(gtk::Align::Start);
    monitor_heading.set_margin_bottom(12);
    page.append(&monitor_heading);

    let monitor_box = GtkBox::new(Orientation::Vertical, 8);
    monitor_box.set_margin_start(12);
    monitor_box.set_margin_bottom(24);

    // CPU toggle
    let cpu_check = CheckButton::with_label("Show CPU Monitor");
    cpu_check.set_active(cpu_frame.is_visible());
    let cpu_frame_clone = cpu_frame.clone();
    cpu_check.connect_toggled(move |check| {
        cpu_frame_clone.set_visible(check.is_active());
        let mut settings = get_app_settings();
        settings.monitor_visibility.show_cpu = check.is_active();
        let _ = save_app_settings(&settings);
    });
    monitor_box.append(&cpu_check);

    // RAM toggle
    let ram_check = CheckButton::with_label("Show RAM Monitor");
    ram_check.set_active(ram_frame.is_visible());
    let ram_frame_clone = ram_frame.clone();
    ram_check.connect_toggled(move |check| {
        ram_frame_clone.set_visible(check.is_active());
        let mut settings = get_app_settings();
        settings.monitor_visibility.show_ram = check.is_active();
        let _ = save_app_settings(&settings);
    });
    monitor_box.append(&ram_check);

    // Network toggle
    let net_check = CheckButton::with_label("Show Network Monitor");
    net_check.set_active(net_frame.is_visible());
    let net_frame_clone = net_frame.clone();
    net_check.connect_toggled(move |check| {
        net_frame_clone.set_visible(check.is_active());
        let mut settings = get_app_settings();
        settings.monitor_visibility.show_network = check.is_active();
        let _ = save_app_settings(&settings);
    });
    monitor_box.append(&net_check);

    page.append(&monitor_box);

    // Logging Group
    let logging_heading = Label::new(Some("Command Logging"));
    logging_heading.add_css_class("title-4");
    logging_heading.set_halign(gtk::Align::Start);
    logging_heading.set_margin_bottom(12);
    page.append(&logging_heading);

    let logging_box = GtkBox::new(Orientation::Vertical, 8);
    logging_box.set_margin_start(12);
    logging_box.set_margin_bottom(24);

    let logging_check = CheckButton::with_label("Enable Command Logging (requires restart)");
    logging_check.set_active(is_command_logging_enabled());
    logging_check.connect_toggled(move |check| {
        let mut settings = get_app_settings();
        settings.enable_command_logging = check.is_active();
        let _ = save_app_settings(&settings);
    });
    logging_box.append(&logging_check);

    page.append(&logging_box);

    // Features Group
    let features_heading = Label::new(Some("Features"));
    features_heading.add_css_class("title-4");
    features_heading.set_halign(gtk::Align::Start);
    features_heading.set_margin_bottom(12);
    page.append(&features_heading);

    let features_box = GtkBox::new(Orientation::Vertical, 8);
    features_box.set_margin_start(12);
    features_box.set_margin_bottom(24);

    let browser_check = CheckButton::with_label("Enable Browser Tab (requires restart)");
    browser_check.set_active(is_browser_enabled());
    browser_check.connect_toggled(move |check| {
        let mut settings = get_app_settings();
        settings.enable_browser = check.is_active();
        let _ = save_app_settings(&settings);
    });
    features_box.append(&browser_check);

    let containers_check = CheckButton::with_label("Enable Containers Tab (requires restart)");
    containers_check.set_active(is_containers_enabled());
    containers_check.connect_toggled(move |check| {
        let mut settings = get_app_settings();
        settings.enable_containers = check.is_active();
        let _ = save_app_settings(&settings);
    });
    features_box.append(&containers_check);

    page.append(&features_box);

    // Terminal Group
    let terminal_heading = Label::new(Some("Terminal Settings"));
    terminal_heading.add_css_class("title-4");
    terminal_heading.set_halign(gtk::Align::Start);
    terminal_heading.set_margin_bottom(12);
    page.append(&terminal_heading);

    let terminal_box = GtkBox::new(Orientation::Vertical, 12);
    terminal_box.set_margin_start(12);
    terminal_box.set_margin_bottom(24);

    // Terminal scrollback lines
    let scrollback_box = GtkBox::new(Orientation::Horizontal, 12);
    let scrollback_label = Label::new(Some("Terminal History Lines:"));
    scrollback_label.set_xalign(0.0);
    scrollback_label.set_hexpand(true);
    scrollback_box.append(&scrollback_label);

    let scrollback_spin = gtk::SpinButton::with_range(100.0, 100000.0, 100.0);
    scrollback_spin.set_value(get_app_settings().terminal_scrollback_lines as f64);
    scrollback_spin.set_digits(0);
    scrollback_spin.connect_value_changed(move |spin| {
        let mut settings = get_app_settings();
        settings.terminal_scrollback_lines = spin.value() as i64;
        let _ = save_app_settings(&settings);
    });
    scrollback_box.append(&scrollback_spin);

    terminal_box.append(&scrollback_box);
    page.append(&terminal_box);

    // Notes Group
    let notes_heading = Label::new(Some("Notes Settings"));
    notes_heading.add_css_class("title-4");
    notes_heading.set_halign(gtk::Align::Start);
    notes_heading.set_margin_bottom(12);
    page.append(&notes_heading);

    let notes_box = GtkBox::new(Orientation::Vertical, 8);
    notes_box.set_margin_start(12);
    notes_box.set_margin_bottom(24);

    let wrap_check = CheckButton::with_label("Wrap text in notes views");
    wrap_check.set_active(is_notes_wrap_text_enabled());
    wrap_check.connect_toggled(move |check| {
        crate::ui::editor::set_notes_wrap_mode(check.is_active());
    });
    notes_box.append(&wrap_check);

    page.append(&notes_box);

    // Zoom Group
    let zoom_heading = Label::new(Some("Zoom Settings"));
    zoom_heading.add_css_class("title-4");
    zoom_heading.set_halign(gtk::Align::Start);
    zoom_heading.set_margin_bottom(12);
    page.append(&zoom_heading);

    let zoom_box = GtkBox::new(Orientation::Vertical, 12);
    zoom_box.set_margin_start(12);
    zoom_box.set_margin_bottom(24);

    // Text zoom
    let text_zoom_box = GtkBox::new(Orientation::Horizontal, 12);
    let text_zoom_label = Label::new(Some("Text Zoom:"));
    text_zoom_label.set_width_request(120);
    text_zoom_label.set_halign(gtk::Align::Start);

    let text_scale = gtk::Scale::with_range(Orientation::Horizontal, zoom::MIN_SCALE, zoom::MAX_SCALE, 0.1);
    text_scale.set_value(get_text_zoom_scale());
    text_scale.set_hexpand(true);
    text_scale.set_draw_value(true);
    text_scale.connect_value_changed(|scale| {
        crate::ui::editor::set_text_zoom_scale(scale.value());
    });

    let text_reset_btn = Button::with_label("Reset");
    text_reset_btn.add_css_class("flat");
    let text_scale_clone = text_scale.clone();
    text_reset_btn.connect_clicked(move |_| {
        text_scale_clone.set_value(zoom::DEFAULT_SCALE);
    });

    text_zoom_box.append(&text_zoom_label);
    text_zoom_box.append(&text_scale);
    text_zoom_box.append(&text_reset_btn);
    zoom_box.append(&text_zoom_box);

    // Terminal zoom
    let terminal_zoom_box = GtkBox::new(Orientation::Horizontal, 12);
    let terminal_zoom_label = Label::new(Some("Terminal Zoom:"));
    terminal_zoom_label.set_width_request(120);
    terminal_zoom_label.set_halign(gtk::Align::Start);

    let terminal_scale = gtk::Scale::with_range(Orientation::Horizontal, zoom::MIN_SCALE, zoom::MAX_SCALE, 0.1);
    terminal_scale.set_value(get_terminal_zoom_scale());
    terminal_scale.set_hexpand(true);
    terminal_scale.set_draw_value(true);
    terminal_scale.connect_value_changed(|scale| {
        crate::ui::terminal::set_terminal_zoom_scale(scale.value());
    });

    let terminal_reset_btn = Button::with_label("Reset");
    terminal_reset_btn.add_css_class("flat");
    let terminal_scale_clone = terminal_scale.clone();
    terminal_reset_btn.connect_clicked(move |_| {
        terminal_scale_clone.set_value(zoom::DEFAULT_SCALE);
    });

    terminal_zoom_box.append(&terminal_zoom_label);
    terminal_zoom_box.append(&terminal_scale);
    terminal_zoom_box.append(&terminal_reset_btn);
    zoom_box.append(&terminal_zoom_box);

    let zoom_hint = Label::new(Some("Tip: Use Ctrl+Scroll for quick zoom"));
    zoom_hint.add_css_class("dim-label");
    zoom_hint.set_halign(gtk::Align::Start);
    zoom_box.append(&zoom_hint);

    page.append(&zoom_box);

    content.set_child(Some(&page));
    scrolled.set_child(Some(&content));

    scrolled
}

/// Creates the keyboard shortcuts page
fn create_shortcuts_page(parent: &adw::ApplicationWindow) -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(500);

    let page = GtkBox::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    let shortcuts = get_keyboard_shortcuts();

    let shortcuts_heading = Label::new(Some("Keyboard Shortcuts"));
    shortcuts_heading.add_css_class("title-4");
    shortcuts_heading.set_halign(gtk::Align::Start);
    shortcuts_heading.set_margin_bottom(12);
    page.append(&shortcuts_heading);

    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);
    list_box.add_css_class("boxed-list");

    // Toggle drawer shortcut
    let drawer_row = create_shortcut_row(
        "Toggle Command Drawer",
        &format!("Ctrl+{}", key_to_display(&shortcuts.toggle_drawer)),
        parent,
        "toggle_drawer",
        false,
    );
    list_box.append(&drawer_row);

    // Insert target shortcut
    let target_row = create_shortcut_row(
        "Insert Target",
        &format!("Ctrl+{}", key_to_display(&shortcuts.insert_target)),
        parent,
        "insert_target",
        false,
    );
    list_box.append(&target_row);

    // Insert timestamp shortcut
    let timestamp_row = create_shortcut_row(
        "Insert Timestamp",
        &format!("Ctrl+Shift+{}", key_to_display(&shortcuts.insert_timestamp)),
        parent,
        "insert_timestamp",
        true,
    );
    list_box.append(&timestamp_row);

    // New shell shortcut
    let new_shell_text = shortcuts.new_shell
        .as_ref()
        .map(|k| format!("Ctrl+Shift+{}", key_to_display(k)))
        .unwrap_or_else(|| "Not assigned".to_string());
    let new_shell_row = create_shortcut_row(
        "New Shell Tab",
        &new_shell_text,
        parent,
        "new_shell",
        true,
    );
    list_box.append(&new_shell_row);

    // New split shortcut
    let new_split_text = shortcuts.new_split
        .as_ref()
        .map(|k| format!("Ctrl+Shift+{}", key_to_display(k)))
        .unwrap_or_else(|| "Not assigned".to_string());
    let new_split_row = create_shortcut_row(
        "New Split View",
        &new_split_text,
        parent,
        "new_split",
        true,
    );
    list_box.append(&new_split_row);

    page.append(&list_box);

    content.set_child(Some(&page));
    scrolled.set_child(Some(&content));

    scrolled
}

/// Creates a shortcut row with Change and Clear buttons
fn create_shortcut_row(
    title: &str,
    current_value: &str,
    parent: &adw::ApplicationWindow,
    shortcut_name: &str,
    _requires_shift: bool,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();

    let row_box = GtkBox::new(Orientation::Horizontal, 12);
    row_box.set_margin_top(8);
    row_box.set_margin_bottom(8);
    row_box.set_margin_start(12);
    row_box.set_margin_end(12);

    let title_label = Label::new(Some(title));
    title_label.set_hexpand(true);
    title_label.set_halign(gtk::Align::Start);

    let shortcut_label = Label::new(Some(current_value));
    shortcut_label.add_css_class("dim-label");
    shortcut_label.add_css_class("numeric");

    let change_btn = Button::with_label("Change");
    change_btn.add_css_class("flat");
    let parent_clone = parent.clone();
    let shortcut_name_owned = shortcut_name.to_string();
    let shortcut_label_clone = shortcut_label.clone();
    change_btn.connect_clicked(move |_| {
        show_key_capture_dialog(&parent_clone, &shortcut_name_owned, &shortcut_label_clone);
    });

    let clear_btn = Button::builder()
        .icon_name("edit-clear-symbolic")
        .tooltip_text("Clear shortcut")
        .build();
    clear_btn.add_css_class("flat");
    let shortcut_name_owned2 = shortcut_name.to_string();
    let shortcut_label_clone2 = shortcut_label.clone();
    clear_btn.connect_clicked(move |_| {
        let mut settings = get_app_settings();
        match shortcut_name_owned2.as_str() {
            "toggle_drawer" => settings.keyboard_shortcuts.toggle_drawer = String::new(),
            "insert_target" => settings.keyboard_shortcuts.insert_target = String::new(),
            "insert_timestamp" => settings.keyboard_shortcuts.insert_timestamp = String::new(),
            "new_shell" => settings.keyboard_shortcuts.new_shell = None,
            "new_split" => settings.keyboard_shortcuts.new_split = None,
            _ => {}
        }
        let _ = save_app_settings(&settings);
        shortcut_label_clone2.set_text("Not assigned");
    });

    row_box.append(&title_label);
    row_box.append(&shortcut_label);
    row_box.append(&change_btn);
    row_box.append(&clear_btn);

    row.set_child(Some(&row_box));
    row
}

/// Shows a dialog to capture a new keyboard shortcut
fn show_key_capture_dialog(parent: &adw::ApplicationWindow, shortcut_name: &str, display_label: &Label) {
    let dialog = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Set Shortcut")
        .default_width(350)
        .default_height(180)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(300);

    let dialog_box = GtkBox::new(Orientation::Vertical, 16);
    dialog_box.set_margin_top(24);
    dialog_box.set_margin_bottom(24);
    dialog_box.set_margin_start(24);
    dialog_box.set_margin_end(24);
    dialog_box.set_halign(gtk::Align::Center);

    let info = Label::new(Some("Press Ctrl + any key"));
    info.set_wrap(true);
    info.add_css_class("dim-label");

    let current_key = Label::new(Some("Waiting for key..."));
    current_key.add_css_class("title-2");

    let cancel_btn = Button::with_label("Cancel");
    cancel_btn.set_halign(gtk::Align::Center);
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });

    dialog_box.append(&info);
    dialog_box.append(&current_key);
    dialog_box.append(&cancel_btn);

    // Keyboard handler
    let key_controller = gtk::EventControllerKey::new();
    let shortcut_name_owned = shortcut_name.to_string();
    let display_label_clone = display_label.clone();
    let dialog_clone2 = dialog.clone();
    let current_key_clone = current_key.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let key_name = keyval.name().unwrap_or_default().to_string();
            let has_shift = modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK);

            let display_text = if has_shift {
                format!("Ctrl+Shift+{}", key_to_display(&key_name))
            } else {
                format!("Ctrl+{}", key_to_display(&key_name))
            };
            current_key_clone.set_text(&display_text);

            // Save the shortcut
            let mut settings = get_app_settings();
            match shortcut_name_owned.as_str() {
                "toggle_drawer" => settings.keyboard_shortcuts.toggle_drawer = key_name.clone(),
                "insert_target" => settings.keyboard_shortcuts.insert_target = key_name.clone(),
                "insert_timestamp" => settings.keyboard_shortcuts.insert_timestamp = key_name.clone(),
                "new_shell" => settings.keyboard_shortcuts.new_shell = Some(key_name.clone()),
                "new_split" => settings.keyboard_shortcuts.new_split = Some(key_name.clone()),
                _ => {}
            }

            if save_app_settings(&settings).is_ok() {
                display_label_clone.set_text(&display_text);

                // Close after delay
                let dialog = dialog_clone2.clone();
                gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
                    dialog.close();
                });
            }

            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });

    content.set_child(Some(&dialog_box));
    dialog.set_content(Some(&content));
    dialog.add_controller(key_controller);
    dialog.present();
}

/// Creates the custom commands page
/// Creates the browser settings page with proxy configuration
fn create_browser_settings_page() -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(500);

    let page = GtkBox::new(Orientation::Vertical, 24);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    // Load current settings
    let current_settings = get_browser_settings();

    // ===== PROXY SETTINGS =====
    let proxy_group = adw::PreferencesGroup::new();
    proxy_group.set_title("Proxy Settings");
    proxy_group.set_description(Some("Configure HTTP or SOCKS5 proxy for browser tabs"));

    // Proxy type selector
    let proxy_type_row = adw::ComboRow::new();
    proxy_type_row.set_title("Proxy Type");
    let proxy_types = gtk::StringList::new(&["None", "HTTP", "SOCKS5"]);
    proxy_type_row.set_model(Some(&proxy_types));
    proxy_type_row.set_selected(match current_settings.proxy_type {
        ProxyType::None => 0,
        ProxyType::Http => 1,
        ProxyType::Socks5 => 2,
    });
    proxy_group.add(&proxy_type_row);

    page.append(&proxy_group);

    // Host and port fields
    let connection_group = adw::PreferencesGroup::new();
    connection_group.set_title("Connection");

    let host_box = GtkBox::new(Orientation::Horizontal, 8);
    host_box.set_margin_top(8);
    host_box.set_margin_bottom(8);
    let host_label = Label::new(Some("Host:"));
    host_label.set_width_chars(10);
    host_label.set_xalign(0.0);
    let host_entry = Entry::new();
    host_entry.set_hexpand(true);
    host_entry.set_text(&current_settings.proxy_host);
    host_entry.set_placeholder_text(Some("e.g., 127.0.0.1"));
    host_box.append(&host_label);
    host_box.append(&host_entry);

    let port_box = GtkBox::new(Orientation::Horizontal, 8);
    port_box.set_margin_bottom(8);
    let port_label = Label::new(Some("Port:"));
    port_label.set_width_chars(10);
    port_label.set_xalign(0.0);
    let port_entry = Entry::new();
    port_entry.set_hexpand(true);
    port_entry.set_text(&current_settings.proxy_port.to_string());
    port_entry.set_placeholder_text(Some("e.g., 8080"));
    port_box.append(&port_label);
    port_box.append(&port_entry);

    connection_group.add(&host_box);
    connection_group.add(&port_box);
    page.append(&connection_group);

    // Authentication fields
    let auth_group = adw::PreferencesGroup::new();
    auth_group.set_title("Authentication (Optional)");

    let username_box = GtkBox::new(Orientation::Horizontal, 8);
    username_box.set_margin_top(8);
    username_box.set_margin_bottom(8);
    let username_label = Label::new(Some("Username:"));
    username_label.set_width_chars(10);
    username_label.set_xalign(0.0);
    let username_entry = Entry::new();
    username_entry.set_hexpand(true);
    if let Some(ref username) = current_settings.proxy_username {
        username_entry.set_text(username);
    }
    username_box.append(&username_label);
    username_box.append(&username_entry);

    let password_box = GtkBox::new(Orientation::Horizontal, 8);
    password_box.set_margin_bottom(8);
    let password_label = Label::new(Some("Password:"));
    password_label.set_width_chars(10);
    password_label.set_xalign(0.0);
    let password_entry = gtk::PasswordEntry::new();
    password_entry.set_hexpand(true);
    password_entry.set_show_peek_icon(true);
    if let Some(ref password) = current_settings.proxy_password {
        password_entry.set_text(password);
    }
    password_box.append(&password_label);
    password_box.append(&password_entry);

    auth_group.add(&username_box);
    auth_group.add(&password_box);
    page.append(&auth_group);

    // CA Certificate settings
    let ca_group = adw::PreferencesGroup::new();
    ca_group.set_title("CA Certificate (for Burp Suite, etc.)");
    ca_group.set_description(Some("Add a custom CA certificate to trust proxy-intercepted connections"));

    let ca_box = GtkBox::new(Orientation::Horizontal, 8);
    ca_box.set_margin_top(8);
    ca_box.set_margin_bottom(8);
    let ca_label = Label::new(Some("Certificate:"));
    ca_label.set_width_chars(10);
    ca_label.set_xalign(0.0);
    let ca_entry = Entry::new();
    ca_entry.set_hexpand(true);
    ca_entry.set_placeholder_text(Some("Path to CA certificate (.pem, .crt, .der)"));
    if let Some(ref ca_path) = current_settings.ca_certificate_path {
        ca_entry.set_text(ca_path);
    }

    let ca_browse_btn = Button::with_label("Browse...");
    let ca_entry_clone = ca_entry.clone();
    ca_browse_btn.connect_clicked(move |btn| {
        let parent_window = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());

        let dialog = gtk::FileChooserNative::new(
            Some("Select CA Certificate"),
            parent_window.as_ref(),
            gtk::FileChooserAction::Open,
            Some("Select"),
            Some("Cancel"),
        );

        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Certificate files"));
        filter.add_pattern("*.pem");
        filter.add_pattern("*.crt");
        filter.add_pattern("*.der");
        filter.add_pattern("*.cer");
        dialog.add_filter(&filter);

        let all_filter = gtk::FileFilter::new();
        all_filter.set_name(Some("All files"));
        all_filter.add_pattern("*");
        dialog.add_filter(&all_filter);

        let ca_entry_for_dialog = ca_entry_clone.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = dialog.file() {
                    if let Some(path) = file.path() {
                        ca_entry_for_dialog.set_text(&path.to_string_lossy());
                    }
                }
            }
        });

        dialog.show();
    });

    ca_box.append(&ca_label);
    ca_box.append(&ca_entry);
    ca_box.append(&ca_browse_btn);
    ca_group.add(&ca_box);

    let ca_info = Label::new(Some("Export Burp's CA: Proxy → Options → Import/Export CA Certificate → Export Certificate in DER format"));
    ca_info.add_css_class("dim-label");
    ca_info.set_wrap(true);
    ca_info.set_xalign(0.0);
    ca_info.set_margin_start(8);
    ca_group.add(&ca_info);

    page.append(&ca_group);

    // Save button
    let button_box = GtkBox::new(Orientation::Horizontal, 8);
    button_box.set_halign(gtk::Align::End);
    button_box.set_margin_top(16);

    let save_btn = Button::with_label("Save Browser Settings");
    save_btn.add_css_class("suggested-action");

    save_btn.connect_clicked(move |btn| {
        let proxy_type = match proxy_type_row.selected() {
            0 => ProxyType::None,
            1 => ProxyType::Http,
            2 => ProxyType::Socks5,
            _ => ProxyType::None,
        };

        let port: u16 = port_entry.text().parse().unwrap_or(8080);

        let username = {
            let text = username_entry.text().to_string();
            if text.is_empty() { None } else { Some(text) }
        };

        let password = {
            let text = password_entry.text().to_string();
            if text.is_empty() { None } else { Some(text) }
        };

        let ca_certificate_path = {
            let text = ca_entry.text().to_string();
            if text.is_empty() { None } else { Some(text) }
        };

        let new_settings = BrowserSettings {
            proxy_type,
            proxy_host: host_entry.text().to_string(),
            proxy_port: port,
            proxy_username: username,
            proxy_password: password,
            ca_certificate_path,
        };

        // Save to app settings
        let mut app_settings = get_app_settings();
        app_settings.browser_settings = new_settings;
        if save_app_settings(&app_settings).is_ok() {
            btn.set_label("Saved!");
            let btn_clone = btn.clone();
            gtk::glib::timeout_add_seconds_local_once(2, move || {
                btn_clone.set_label("Save Browser Settings");
            });
        }
    });

    button_box.append(&save_btn);
    page.append(&button_box);

    // Info label
    let info_label = Label::new(Some("Note: Proxy settings apply to new browser tabs.\nExisting tabs need to be reloaded after changing settings."));
    info_label.add_css_class("dim-label");
    info_label.set_wrap(true);
    info_label.set_margin_top(16);
    page.append(&info_label);

    content.set_child(Some(&page));
    scrolled.set_child(Some(&content));
    scrolled
}

fn create_commands_page(
    parent: &adw::ApplicationWindow,
    settings_dialog: &adw::Window,
    cpu_frame: &Frame,
    ram_frame: &Frame,
    net_frame: &Frame,
) -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(500);

    let page = GtkBox::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    let commands_heading = Label::new(Some("Custom Commands"));
    commands_heading.add_css_class("title-4");
    commands_heading.set_halign(gtk::Align::Start);
    commands_heading.set_margin_bottom(12);
    page.append(&commands_heading);

    let inner_box = GtkBox::new(Orientation::Vertical, 8);
    inner_box.set_margin_start(12);

    let hint_label = Label::new(Some("Add your own command templates. Use {target} as placeholder."));
    hint_label.add_css_class("dim-label");
    hint_label.set_halign(gtk::Align::Start);
    hint_label.set_wrap(true);
    inner_box.append(&hint_label);

    // Commands list
    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);
    list_box.add_css_class("boxed-list");
    list_box.set_margin_top(12);

    let commands = load_custom_commands();

    if commands.is_empty() {
        let empty_row = gtk::ListBoxRow::new();
        let empty_label = Label::new(Some("No custom commands yet"));
        empty_label.add_css_class("dim-label");
        empty_label.set_margin_top(12);
        empty_label.set_margin_bottom(12);
        empty_row.set_child(Some(&empty_label));
        list_box.append(&empty_row);
    } else {
        for (idx, cmd) in commands.iter().enumerate() {
            let row = gtk::ListBoxRow::new();
            let row_box = GtkBox::new(Orientation::Horizontal, 12);
            row_box.set_margin_top(8);
            row_box.set_margin_bottom(8);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);

            let info_box = GtkBox::new(Orientation::Vertical, 2);
            info_box.set_hexpand(true);

            let name_label = Label::new(Some(&cmd.name));
            name_label.set_halign(gtk::Align::Start);
            name_label.add_css_class("heading");

            let cmd_label = Label::new(Some(&cmd.command));
            cmd_label.set_halign(gtk::Align::Start);
            cmd_label.add_css_class("dim-label");
            cmd_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

            info_box.append(&name_label);
            info_box.append(&cmd_label);

            let edit_btn = Button::builder()
                .icon_name("document-edit-symbolic")
                .tooltip_text("Edit")
                .build();
            edit_btn.add_css_class("flat");

            let parent_clone = parent.clone();
            let dialog_clone = settings_dialog.clone();
            let cpu_clone = cpu_frame.clone();
            let ram_clone = ram_frame.clone();
            let net_clone = net_frame.clone();
            let cmd_clone = cmd.clone();
            edit_btn.connect_clicked(move |_| {
                let parent_ref = parent_clone.clone();
                let dialog_ref = dialog_clone.clone();
                let cpu_ref = cpu_clone.clone();
                let ram_ref = ram_clone.clone();
                let net_ref = net_clone.clone();
                show_edit_command_dialog(&parent_clone, idx, cmd_clone.clone(), move || {
                    dialog_ref.close();
                    show_settings_dialog(&parent_ref, &cpu_ref, &ram_ref, &net_ref);
                });
            });

            let delete_btn = Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text("Delete")
                .build();
            delete_btn.add_css_class("flat");
            delete_btn.add_css_class("error");

            let parent_clone2 = parent.clone();
            let dialog_clone2 = settings_dialog.clone();
            let cpu_clone2 = cpu_frame.clone();
            let ram_clone2 = ram_frame.clone();
            let net_clone2 = net_frame.clone();
            delete_btn.connect_clicked(move |_| {
                if delete_custom_command(idx).is_ok() {
                    dialog_clone2.close();
                    show_settings_dialog(&parent_clone2, &cpu_clone2, &ram_clone2, &net_clone2);
                }
            });

            row_box.append(&info_box);
            row_box.append(&edit_btn);
            row_box.append(&delete_btn);

            row.set_child(Some(&row_box));
            list_box.append(&row);
        }
    }

    inner_box.append(&list_box);

    // Add button
    let add_btn = Button::with_label("Add Command");
    add_btn.add_css_class("suggested-action");
    add_btn.add_css_class("pill");
    add_btn.set_halign(gtk::Align::Center);
    add_btn.set_margin_top(12);

    let parent_clone = parent.clone();
    let dialog_clone = settings_dialog.clone();
    let cpu_clone = cpu_frame.clone();
    let ram_clone = ram_frame.clone();
    let net_clone = net_frame.clone();
    add_btn.connect_clicked(move |_| {
        let parent_ref = parent_clone.clone();
        let dialog_ref = dialog_clone.clone();
        let cpu_ref = cpu_clone.clone();
        let ram_ref = ram_clone.clone();
        let net_ref = net_clone.clone();
        show_add_command_dialog(&parent_clone, move || {
            dialog_ref.close();
            show_settings_dialog(&parent_ref, &cpu_ref, &ram_ref, &net_ref);
        });
    });

    inner_box.append(&add_btn);
    page.append(&inner_box);

    content.set_child(Some(&page));
    scrolled.set_child(Some(&content));

    scrolled
}

/// Shows dialog to add a new custom command
fn show_add_command_dialog<F>(parent: &adw::ApplicationWindow, on_save: F)
where
    F: Fn() + 'static,
{
    let dialog = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Add Custom Command")
        .default_width(450)
        .default_height(400)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    let header = adw::HeaderBar::new();
    main_box.append(&header);

    let content = adw::Clamp::new();
    content.set_maximum_size(400);

    let page = GtkBox::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    // Name entry
    let name_box = GtkBox::new(Orientation::Vertical, 4);
    let name_label = Label::new(Some("Name"));
    name_label.set_halign(gtk::Align::Start);
    let name_entry = Entry::new();
    name_entry.set_placeholder_text(Some("Command name"));
    name_box.append(&name_label);
    name_box.append(&name_entry);
    page.append(&name_box);

    // Command entry
    let command_box = GtkBox::new(Orientation::Vertical, 4);
    let command_label = Label::new(Some("Command"));
    command_label.set_halign(gtk::Align::Start);
    let command_entry = Entry::new();
    command_entry.set_placeholder_text(Some("nmap -sV {target}"));
    command_box.append(&command_label);
    command_box.append(&command_entry);
    page.append(&command_box);

    // Description entry
    let desc_box = GtkBox::new(Orientation::Vertical, 4);
    let desc_label = Label::new(Some("Description"));
    desc_label.set_halign(gtk::Align::Start);
    let desc_entry = Entry::new();
    desc_entry.set_placeholder_text(Some("What this command does"));
    desc_box.append(&desc_label);
    desc_box.append(&desc_entry);
    page.append(&desc_box);

    // Category entry
    let cat_box = GtkBox::new(Orientation::Vertical, 4);
    let cat_label = Label::new(Some("Category"));
    cat_label.set_halign(gtk::Align::Start);
    let cat_entry = Entry::new();
    cat_entry.set_text("Custom");
    cat_box.append(&cat_label);
    cat_box.append(&cat_entry);
    page.append(&cat_box);

    // Tip
    let tip_label = Label::new(Some("💡 Use {target} as a placeholder for target selection"));
    tip_label.add_css_class("dim-label");
    tip_label.set_wrap(true);
    tip_label.set_margin_top(12);
    page.append(&tip_label);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    button_box.set_halign(gtk::Align::End);
    button_box.set_margin_top(24);

    let cancel_btn = Button::with_label("Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });

    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    let dialog_clone2 = dialog.clone();
    let name_entry_clone = name_entry.clone();
    let command_entry_clone = command_entry.clone();
    let desc_entry_clone = desc_entry.clone();
    let cat_entry_clone = cat_entry.clone();
    save_btn.connect_clicked(move |_| {
        let name = name_entry_clone.text().to_string();
        let command = command_entry_clone.text().to_string();
        let description = desc_entry_clone.text().to_string();
        let category = cat_entry_clone.text().to_string();

        if name.is_empty() || command.is_empty() {
            log::warn!("Name and command are required");
            return;
        }

        let cmd_template = CommandTemplate {
            name,
            command,
            description: if description.is_empty() { "Custom command".to_string() } else { description },
            category: if category.is_empty() { "Custom".to_string() } else { category },
        };

        if save_custom_command(cmd_template).is_ok() {
            on_save();
            dialog_clone2.close();
        }
    });

    button_box.append(&cancel_btn);
    button_box.append(&save_btn);
    page.append(&button_box);

    content.set_child(Some(&page));
    main_box.append(&content);
    dialog.set_content(Some(&main_box));
    dialog.present();
}

/// Shows dialog to edit an existing custom command
fn show_edit_command_dialog<F>(parent: &adw::ApplicationWindow, index: usize, cmd: CommandTemplate, on_save: F)
where
    F: Fn() + 'static,
{
    let dialog = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Edit Command")
        .default_width(450)
        .default_height(400)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    let header = adw::HeaderBar::new();
    main_box.append(&header);

    let content = adw::Clamp::new();
    content.set_maximum_size(400);

    let page = GtkBox::new(Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    // Name entry
    let name_box = GtkBox::new(Orientation::Vertical, 4);
    let name_label = Label::new(Some("Name"));
    name_label.set_halign(gtk::Align::Start);
    let name_entry = Entry::new();
    name_entry.set_text(&cmd.name);
    name_box.append(&name_label);
    name_box.append(&name_entry);
    page.append(&name_box);

    // Command entry
    let command_box = GtkBox::new(Orientation::Vertical, 4);
    let command_label = Label::new(Some("Command"));
    command_label.set_halign(gtk::Align::Start);
    let command_entry = Entry::new();
    command_entry.set_text(&cmd.command);
    command_box.append(&command_label);
    command_box.append(&command_entry);
    page.append(&command_box);

    // Description entry
    let desc_box = GtkBox::new(Orientation::Vertical, 4);
    let desc_label = Label::new(Some("Description"));
    desc_label.set_halign(gtk::Align::Start);
    let desc_entry = Entry::new();
    desc_entry.set_text(&cmd.description);
    desc_box.append(&desc_label);
    desc_box.append(&desc_entry);
    page.append(&desc_box);

    // Category entry
    let cat_box = GtkBox::new(Orientation::Vertical, 4);
    let cat_label = Label::new(Some("Category"));
    cat_label.set_halign(gtk::Align::Start);
    let cat_entry = Entry::new();
    cat_entry.set_text(&cmd.category);
    cat_box.append(&cat_label);
    cat_box.append(&cat_entry);
    page.append(&cat_box);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    button_box.set_halign(gtk::Align::End);
    button_box.set_margin_top(24);

    let cancel_btn = Button::with_label("Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });

    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    let dialog_clone2 = dialog.clone();
    let name_entry_clone = name_entry.clone();
    let command_entry_clone = command_entry.clone();
    let desc_entry_clone = desc_entry.clone();
    let cat_entry_clone = cat_entry.clone();
    save_btn.connect_clicked(move |_| {
        let name = name_entry_clone.text().to_string();
        let command = command_entry_clone.text().to_string();
        let description = desc_entry_clone.text().to_string();
        let category = cat_entry_clone.text().to_string();

        if name.is_empty() || command.is_empty() {
            log::warn!("Name and command are required");
            return;
        }

        let cmd_template = CommandTemplate {
            name,
            command,
            description: if description.is_empty() { "Custom command".to_string() } else { description },
            category: if category.is_empty() { "Custom".to_string() } else { category },
        };

        if update_custom_command(index, cmd_template).is_ok() {
            on_save();
            dialog_clone2.close();
        }
    });

    button_box.append(&cancel_btn);
    button_box.append(&save_btn);
    page.append(&button_box);

    content.set_child(Some(&page));
    main_box.append(&content);
    dialog.set_content(Some(&main_box));
    dialog.present();
}

/// Creates the containers settings page for the settings notebook
fn create_containers_settings_page() -> ScrolledWindow {
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = adw::Clamp::new();
    content.set_maximum_size(500);

    let page = GtkBox::new(Orientation::Vertical, 24);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);

    let config = load_container_config();

    // === Runtime Section ===
    let runtime_heading = Label::new(Some("Container Runtime"));
    runtime_heading.add_css_class("title-4");
    runtime_heading.set_halign(gtk::Align::Start);
    runtime_heading.set_margin_bottom(12);
    page.append(&runtime_heading);

    let runtime_box = GtkBox::new(Orientation::Vertical, 8);
    runtime_box.set_margin_start(12);
    runtime_box.set_margin_bottom(24);

    let runtime_row = GtkBox::new(Orientation::Horizontal, 12);
    let runtime_label = Label::new(Some("Runtime:"));
    runtime_label.set_width_request(120);
    runtime_label.set_halign(gtk::Align::Start);

    let runtime_combo = ComboBoxText::new();
    runtime_combo.append_text("Podman");
    runtime_combo.append_text("Docker");
    runtime_combo.set_active(Some(match config.runtime {
        ContainerRuntime::Podman => 0,
        ContainerRuntime::Docker => 1,
    }));
    runtime_combo.set_hexpand(true);

    runtime_row.append(&runtime_label);
    runtime_row.append(&runtime_combo);
    runtime_box.append(&runtime_row);

    let runtime_hint = Label::new(Some("Podman may require sudo for full networking support"));
    runtime_hint.add_css_class("dim-label");
    runtime_hint.set_halign(gtk::Align::Start);
    runtime_box.append(&runtime_hint);

    page.append(&runtime_box);

    // === Image Names Section ===
    let images_heading = Label::new(Some("Image Names"));
    images_heading.add_css_class("title-4");
    images_heading.set_halign(gtk::Align::Start);
    images_heading.set_margin_bottom(12);
    page.append(&images_heading);

    let images_box = GtkBox::new(Orientation::Vertical, 8);
    images_box.set_margin_start(12);
    images_box.set_margin_bottom(24);

    let base_image_row = GtkBox::new(Orientation::Horizontal, 12);
    let base_image_label = Label::new(Some("Base Image:"));
    base_image_label.set_width_request(120);
    base_image_label.set_halign(gtk::Align::Start);
    let base_image_entry = Entry::builder()
        .text(&config.image_name)
        .placeholder_text("Base image name (e.g., kali-build)")
        .hexpand(true)
        .build();
    base_image_row.append(&base_image_label);
    base_image_row.append(&base_image_entry);
    images_box.append(&base_image_row);

    let master_image_row = GtkBox::new(Orientation::Horizontal, 12);
    let master_image_label = Label::new(Some("Master Image:"));
    master_image_label.set_width_request(120);
    master_image_label.set_halign(gtk::Align::Start);
    let master_image_entry = Entry::builder()
        .text(&config.master_image)
        .placeholder_text("Master image name (e.g., kali-master)")
        .hexpand(true)
        .build();
    master_image_row.append(&master_image_label);
    master_image_row.append(&master_image_entry);
    images_box.append(&master_image_row);

    let dockerfile_row = GtkBox::new(Orientation::Horizontal, 12);
    let dockerfile_label = Label::new(Some("Dockerfile:"));
    dockerfile_label.set_width_request(120);
    dockerfile_label.set_halign(gtk::Align::Start);
    let dockerfile_entry = Entry::builder()
        .text(&config.dockerfile_path)
        .placeholder_text("Path to Dockerfile")
        .hexpand(true)
        .build();
    dockerfile_row.append(&dockerfile_label);
    dockerfile_row.append(&dockerfile_entry);
    images_box.append(&dockerfile_row);

    page.append(&images_box);

    // === SSH Section ===
    let ssh_heading = Label::new(Some("SSH Configuration"));
    ssh_heading.add_css_class("title-4");
    ssh_heading.set_halign(gtk::Align::Start);
    ssh_heading.set_margin_bottom(12);
    page.append(&ssh_heading);

    let ssh_box = GtkBox::new(Orientation::Vertical, 8);
    ssh_box.set_margin_start(12);
    ssh_box.set_margin_bottom(24);

    let ssh_key_row = GtkBox::new(Orientation::Horizontal, 12);
    let ssh_key_label = Label::new(Some("SSH Public Key:"));
    ssh_key_label.set_width_request(120);
    ssh_key_label.set_halign(gtk::Align::Start);
    let ssh_key_entry = Entry::builder()
        .text(&config.ssh_pubkey_path)
        .placeholder_text("Path to SSH public key")
        .hexpand(true)
        .build();
    ssh_key_row.append(&ssh_key_label);
    ssh_key_row.append(&ssh_key_entry);
    ssh_box.append(&ssh_key_row);

    page.append(&ssh_box);

    // === Resource Limits Section ===
    let resources_heading = Label::new(Some("Resource Limits"));
    resources_heading.add_css_class("title-4");
    resources_heading.set_halign(gtk::Align::Start);
    resources_heading.set_margin_bottom(12);
    page.append(&resources_heading);

    let resources_box = GtkBox::new(Orientation::Vertical, 8);
    resources_box.set_margin_start(12);
    resources_box.set_margin_bottom(24);

    let memory_row = GtkBox::new(Orientation::Horizontal, 12);
    let memory_label = Label::new(Some("Memory Limit:"));
    memory_label.set_width_request(120);
    memory_label.set_halign(gtk::Align::Start);
    let memory_entry = Entry::builder()
        .text(&config.memory_limit)
        .placeholder_text("Memory limit (e.g., 8g)")
        .hexpand(true)
        .build();
    memory_row.append(&memory_label);
    memory_row.append(&memory_entry);
    resources_box.append(&memory_row);

    let cpu_row = GtkBox::new(Orientation::Horizontal, 12);
    let cpu_label = Label::new(Some("CPU Limit:"));
    cpu_label.set_width_request(120);
    cpu_label.set_halign(gtk::Align::Start);
    let cpu_entry = Entry::builder()
        .text(&config.cpu_limit.to_string())
        .placeholder_text("CPU limit (number of CPUs)")
        .hexpand(true)
        .build();
    cpu_row.append(&cpu_label);
    cpu_row.append(&cpu_entry);
    resources_box.append(&cpu_row);

    let data_row = GtkBox::new(Orientation::Horizontal, 12);
    let data_label = Label::new(Some("Data Directory:"));
    data_label.set_width_request(120);
    data_label.set_halign(gtk::Align::Start);
    let data_mapping_entry = Entry::builder()
        .text(&config.data_mapping)
        .placeholder_text("Directory to map to /data in container")
        .hexpand(true)
        .build();
    data_row.append(&data_label);
    data_row.append(&data_mapping_entry);
    resources_box.append(&data_row);

    page.append(&resources_box);

    // === VNC Section ===
    let vnc_heading = Label::new(Some("VNC Configuration"));
    vnc_heading.add_css_class("title-4");
    vnc_heading.set_halign(gtk::Align::Start);
    vnc_heading.set_margin_bottom(12);
    page.append(&vnc_heading);

    let vnc_box = GtkBox::new(Orientation::Vertical, 8);
    vnc_box.set_margin_start(12);
    vnc_box.set_margin_bottom(24);

    let vnc_expose_check = CheckButton::with_label("Expose VNC port outside container");
    vnc_expose_check.set_active(config.vnc_expose);
    vnc_box.append(&vnc_expose_check);

    let vnc_port_row = GtkBox::new(Orientation::Horizontal, 12);
    let vnc_port_label = Label::new(Some("VNC Port:"));
    vnc_port_label.set_width_request(120);
    vnc_port_label.set_halign(gtk::Align::Start);
    let vnc_port_entry = Entry::builder()
        .text(&config.vnc_port.to_string())
        .placeholder_text("VNC Port (e.g., 5900)")
        .hexpand(true)
        .build();
    vnc_port_row.append(&vnc_port_label);
    vnc_port_row.append(&vnc_port_entry);
    vnc_box.append(&vnc_port_row);

    let vnc_password_row = GtkBox::new(Orientation::Horizontal, 12);
    let vnc_password_label = Label::new(Some("VNC Password:"));
    vnc_password_label.set_width_request(120);
    vnc_password_label.set_halign(gtk::Align::Start);
    let vnc_password_entry = PasswordEntry::builder()
        .text(&config.vnc_password)
        .show_peek_icon(true)
        .hexpand(true)
        .build();
    vnc_password_row.append(&vnc_password_label);
    vnc_password_row.append(&vnc_password_entry);
    vnc_box.append(&vnc_password_row);

    let vnc_display_row = GtkBox::new(Orientation::Horizontal, 12);
    let vnc_display_label = Label::new(Some("Display:"));
    vnc_display_label.set_width_request(120);
    vnc_display_label.set_halign(gtk::Align::Start);
    let vnc_display_entry = Entry::builder()
        .text(&config.vnc_display)
        .placeholder_text("Display resolution (e.g., 1920x1080)")
        .hexpand(true)
        .build();
    vnc_display_row.append(&vnc_display_label);
    vnc_display_row.append(&vnc_display_entry);
    vnc_box.append(&vnc_display_row);

    page.append(&vnc_box);

    // === Save Button ===
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    button_box.set_halign(gtk::Align::End);
    button_box.set_margin_top(12);

    let save_btn = Button::with_label("Save Container Settings");
    save_btn.add_css_class("suggested-action");

    // Clone entries for save handler
    let runtime_combo_clone = runtime_combo.clone();
    let base_image_entry_clone = base_image_entry.clone();
    let master_image_entry_clone = master_image_entry.clone();
    let dockerfile_entry_clone = dockerfile_entry.clone();
    let ssh_key_entry_clone = ssh_key_entry.clone();
    let memory_entry_clone = memory_entry.clone();
    let cpu_entry_clone = cpu_entry.clone();
    let data_mapping_entry_clone = data_mapping_entry.clone();
    let vnc_expose_check_clone = vnc_expose_check.clone();
    let vnc_port_entry_clone = vnc_port_entry.clone();
    let vnc_password_entry_clone = vnc_password_entry.clone();
    let vnc_display_entry_clone = vnc_display_entry.clone();

    save_btn.connect_clicked(move |btn| {
        let new_config = ContainerConfig {
            runtime: match runtime_combo_clone.active() {
                Some(0) => ContainerRuntime::Podman,
                _ => ContainerRuntime::Docker,
            },
            image_name: base_image_entry_clone.text().to_string(),
            master_image: master_image_entry_clone.text().to_string(),
            dockerfile_path: dockerfile_entry_clone.text().to_string(),
            ssh_pubkey_path: ssh_key_entry_clone.text().to_string(),
            data_mapping: data_mapping_entry_clone.text().to_string(),
            vnc_expose: vnc_expose_check_clone.is_active(),
            vnc_port: vnc_port_entry_clone.text().parse().unwrap_or(5900),
            vnc_password: vnc_password_entry_clone.text().to_string(),
            vnc_display: vnc_display_entry_clone.text().to_string(),
            vnc_depth: 16,
            novnc_port: 1337,
            memory_limit: memory_entry_clone.text().to_string(),
            cpu_limit: cpu_entry_clone.text().parse().unwrap_or(10),
        };

        match save_container_config(&new_config) {
            Ok(_) => {
                log::info!("Container settings saved");
                // Show a brief visual feedback
                btn.set_label("Saved!");
                btn.set_sensitive(false);
                let btn_clone = btn.clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    btn_clone.set_label("Save Container Settings");
                    btn_clone.set_sensitive(true);
                });
            }
            Err(e) => {
                log::error!("Failed to save container settings: {}", e);
                btn.set_label("Save Failed!");
                let btn_clone = btn.clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    btn_clone.set_label("Save Container Settings");
                });
            }
        }
    });

    button_box.append(&save_btn);
    page.append(&button_box);

    content.set_child(Some(&page));
    scrolled.set_child(Some(&content));

    scrolled
}
