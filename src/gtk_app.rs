//! GTK4 UI implementation for PenEnv
//!
//! This module contains all the UI components and logic for the PenEnv application,
//! including:
//! - Base directory selection dialog
//! - Main window with tabbed interface
//! - Text editors for targets and notes with markdown highlighting
//! - Shell tabs with VTE terminal integration
//! - Command templates drawer with search functionality
//! - Split view mode combining notes and shell
//! - Target management and insertion features

use gtk4::prelude::*;
use gtk4::{self as gtk, Application, ApplicationWindow, Box as GtkBox, Button, Label, Notebook, 
          Orientation, ScrolledWindow, TextView, Paned, ProgressBar, Frame, CheckButton, Entry, Separator, ListBox};
use gtk4::glib;
use libadwaita::{self as adw};
use vte4::{Terminal, TerminalExt, TerminalExtManual};
use std::fs;
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use chrono;
use serde::{Deserialize, Serialize};
use sysinfo::{System, Networks};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct CommandTemplate {
    name: String,
    command: String,
    description: String,
    category: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandsConfig {
    commands: Vec<CommandTemplate>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct KeyboardShortcuts {
    toggle_drawer: String,
    insert_target: String,
    insert_timestamp: String,
    new_shell: Option<String>,
    new_split: Option<String>,
}

impl Default for KeyboardShortcuts {
    fn default() -> Self {
        Self {
            toggle_drawer: "grave".to_string(),  // ` key
            insert_target: "t".to_string(),
            insert_timestamp: "T".to_string(),  // Shift+T
            new_shell: Some("N".to_string()),  // Shift+N
            new_split: Some("S".to_string()),  // Shift+S
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct AppSettings {
    monitor_visibility: MonitorVisibility,
    keyboard_shortcuts: KeyboardShortcuts,
    enable_command_logging: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct MonitorVisibility {
    show_cpu: bool,
    show_ram: bool,
    show_network: bool,
}

impl Default for MonitorVisibility {
    fn default() -> Self {
        Self {
            show_cpu: true,
            show_ram: true,
            show_network: true,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            monitor_visibility: MonitorVisibility::default(),
            keyboard_shortcuts: KeyboardShortcuts::default(),
            enable_command_logging: true,
        }
    }
}

// Embed the commands.yaml file at compile time
const COMMANDS_YAML: &str = include_str!("../commands.yaml");

// Global base directory
thread_local! {
    static BASE_DIR: RefCell<PathBuf> = RefCell::new(PathBuf::from("."));
    static APP_SETTINGS: RefCell<AppSettings> = RefCell::new(AppSettings {
        monitor_visibility: MonitorVisibility {
            show_cpu: true,
            show_ram: true,
            show_network: true,
        },
        keyboard_shortcuts: KeyboardShortcuts {
            toggle_drawer: "grave".to_string(),
            insert_target: "t".to_string(),
            insert_timestamp: "T".to_string(),
            new_shell: Some("N".to_string()),
            new_split: Some("S".to_string()),
        },
        enable_command_logging: true,
    });
}

/// Sets the base directory for storing project files
fn set_base_dir(path: PathBuf) {
    BASE_DIR.with(|dir| {
        *dir.borrow_mut() = path;
    });
}

/// Gets the current base directory
fn get_base_dir() -> PathBuf {
    BASE_DIR.with(|dir| dir.borrow().clone())
}

/// Constructs a full file path from the base directory and filename
fn get_file_path(filename: &str) -> PathBuf {
    let mut path = get_base_dir();
    path.push(filename);
    path
}

/// Gets the penenv config directory, creating it if it doesn't exist
fn get_config_dir() -> PathBuf {
    let mut path = if let Some(config_dir) = glib::user_config_dir().to_str() {
        PathBuf::from(config_dir)
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string())).join(".config")
    };
    path.push("penenv");
    fs::create_dir_all(&path).ok();
    path
}

/// Gets the custom commands config file path in user's config directory
fn get_custom_commands_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("custom_commands.yaml");
    path
}

/// Gets the settings config file path
fn get_settings_config_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("settings.yaml");
    path
}

/// Loads app settings from config file
fn load_app_settings() -> AppSettings {
    let path = get_settings_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_yaml::from_str::<AppSettings>(&content) {
                APP_SETTINGS.with(|s| {
                    *s.borrow_mut() = settings.clone();
                });
                return settings;
            }
        }
    }
    AppSettings::default()
}

/// Saves app settings to config file
fn save_app_settings(settings: &AppSettings) -> Result<(), String> {
    let path = get_settings_config_path();
    let yaml = serde_yaml::to_string(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    fs::write(&path, yaml)
        .map_err(|e| format!("Failed to write settings config: {}", e))?;
    APP_SETTINGS.with(|s| {
        *s.borrow_mut() = settings.clone();
    });
    Ok(())
}

/// Gets the current app settings
fn get_app_settings() -> AppSettings {
    APP_SETTINGS.with(|s| s.borrow().clone())
}

/// Gets the current keyboard shortcuts
fn get_keyboard_shortcuts() -> KeyboardShortcuts {
    APP_SETTINGS.with(|s| s.borrow().keyboard_shortcuts.clone())
}

/// Checks if command logging is enabled
fn is_command_logging_enabled() -> bool {
    APP_SETTINGS.with(|s| s.borrow().enable_command_logging)
}

/// Loads targets from targets.txt file
/// 
/// Returns a vector of non-empty, non-comment lines from the targets file.
/// Comments are lines starting with '#'. Returns empty vector if file doesn't exist.
fn load_targets() -> Vec<String> {
    if let Ok(content) = fs::read_to_string(get_file_path("targets.txt")) {
        content
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .map(String::from)
            .collect()
    } else {
        Vec::new()
    }
}

/// Loads command templates from the embedded YAML file and custom commands
/// 
/// Returns an empty vector if parsing fails, with error logged to stderr
fn load_command_templates() -> Vec<CommandTemplate> {
    let mut commands = Vec::new();
    
    // Load built-in commands
    match serde_yaml::from_str::<CommandsConfig>(COMMANDS_YAML) {
        Ok(config) => commands.extend(config.commands),
        Err(e) => {
            eprintln!("Warning: Failed to parse commands.yaml: {}. Command drawer will be empty.", e);
        }
    }
    
    // Load custom commands
    let custom_path = get_custom_commands_path();
    if custom_path.exists() {
        if let Ok(content) = fs::read_to_string(&custom_path) {
            match serde_yaml::from_str::<CommandsConfig>(&content) {
                Ok(config) => commands.extend(config.commands),
                Err(e) => {
                    eprintln!("Warning: Failed to parse custom_commands.yaml: {}", e);
                }
            }
        }
    }
    
    commands
}

/// Saves a new custom command to the custom_commands.yaml file
fn save_custom_command(command: CommandTemplate) -> Result<(), String> {
    let custom_path = get_custom_commands_path();
    
    // Load existing custom commands
    let mut commands = Vec::new();
    if custom_path.exists() {
        if let Ok(content) = fs::read_to_string(&custom_path) {
            if let Ok(config) = serde_yaml::from_str::<CommandsConfig>(&content) {
                commands = config.commands;
            }
        }
    }
    
    // Add new command
    commands.push(command);
    
    // Save back to file
    let config = CommandsConfig { commands };
    let yaml = serde_yaml::to_string(&config).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&custom_path, yaml).map_err(|e| format!("Failed to write file: {}", e))?;
    
    Ok(())
}

/// Loads only custom commands from the config file
fn load_custom_commands() -> Vec<CommandTemplate> {
    let custom_path = get_custom_commands_path();
    if custom_path.exists() {
        if let Ok(content) = fs::read_to_string(&custom_path) {
            if let Ok(config) = serde_yaml::from_str::<CommandsConfig>(&content) {
                return config.commands;
            }
        }
    }
    Vec::new()
}

/// Saves the entire list of custom commands
fn save_custom_commands_list(commands: Vec<CommandTemplate>) -> Result<(), String> {
    let custom_path = get_custom_commands_path();
    let config = CommandsConfig { commands };
    let yaml = serde_yaml::to_string(&config).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&custom_path, yaml).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

/// Deletes a custom command by index
fn delete_custom_command(index: usize) -> Result<(), String> {
    let mut commands = load_custom_commands();
    if index < commands.len() {
        commands.remove(index);
        save_custom_commands_list(commands)?;
        Ok(())
    } else {
        Err("Invalid command index".to_string())
    }
}

/// Updates a custom command by index
fn update_custom_command(index: usize, command: CommandTemplate) -> Result<(), String> {
    let mut commands = load_custom_commands();
    if index < commands.len() {
        commands[index] = command;
        save_custom_commands_list(commands)?;
        Ok(())
    } else {
        Err("Invalid command index".to_string())
    }
}

/// Builds and initializes the main application UI
/// 
/// Shows a base directory selection dialog on startup, then creates the main window
/// with all tabs, toolbars, and features.
pub fn build_ui(app: &Application) {
    // Load libadwaita stylesheet
    adw::init().expect("Failed to initialize libadwaita");

    // Show base directory selection dialog first
    let app_clone = app.clone();
    show_base_dir_dialog(app, move |selected_dir| {
        if let Some(dir) = selected_dir {
            set_base_dir(dir);
            create_main_window(&app_clone);
        }
    });
}

fn show_base_dir_dialog<F>(app: &Application, callback: F)
where
    F: Fn(Option<PathBuf>) + 'static,
{
    let dialog = gtk::Window::builder()
        .application(app)
        .title("Select Base Directory")
        .modal(true)
        .default_width(500)
        .default_height(200)
        .build();
    
    let dialog_box = GtkBox::new(Orientation::Vertical, 15);
    dialog_box.set_margin_top(20);
    dialog_box.set_margin_bottom(20);
    dialog_box.set_margin_start(20);
    dialog_box.set_margin_end(20);
    
    // Get current directory
    let current_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string();
    
    // Question label
    let question_label = Label::new(Some(&format!(
        "Do you want to use the current directory as the base location?\n\n{}",
        current_dir
    )));
    question_label.set_wrap(true);
    question_label.set_justify(gtk::Justification::Center);
    
    // Info label
    let info_text = if is_command_logging_enabled() {
        "This directory will store targets.txt, notes.md, and commands.log"
    } else {
        "This directory will store targets.txt and notes.md"
    };
    let info_label = Label::new(Some(info_text));
    info_label.set_opacity(0.7);
    info_label.set_wrap(true);
    info_label.set_justify(gtk::Justification::Center);
    
    // Button box
    let button_box = GtkBox::new(Orientation::Horizontal, 10);
    button_box.set_halign(gtk::Align::Center);
    
    let yes_btn = Button::with_label("Yes, use current directory");
    yes_btn.add_css_class("suggested-action");
    
    let browse_btn = Button::with_label("No, browse for directory");
    
    // Yes button handler
    let dialog_clone = dialog.clone();
    let current_dir_clone = current_dir.clone();
    let callback_clone = Rc::new(callback);
    let callback_clone2 = Rc::clone(&callback_clone);
    yes_btn.connect_clicked(move |_| {
        callback_clone(Some(PathBuf::from(&current_dir_clone)));
        dialog_clone.close();
    });
    
    // Browse button handler
    let dialog_clone2 = dialog.clone();
    browse_btn.connect_clicked(move |_| {
        let file_chooser = gtk::FileChooserDialog::new(
            Some("Select Base Directory"),
            Some(&dialog_clone2),
            gtk::FileChooserAction::SelectFolder,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Select", gtk::ResponseType::Accept),
            ],
        );
        
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
    
    dialog_box.append(&question_label);
    dialog_box.append(&info_label);
    dialog_box.append(&button_box);
    
    dialog.set_child(Some(&dialog_box));
    dialog.present();
}

fn show_settings_dialog(parent: &ApplicationWindow, cpu_frame: &Frame, ram_frame: &Frame, net_frame: &Frame) {
    let dialog = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .default_width(600)
        .default_height(500)
        .build();
    
    let main_box = GtkBox::new(Orientation::Vertical, 0);
    
    // Create notebook for tabs
    let notebook = Notebook::builder()
        .scrollable(false)
        .build();
    
    // TAB 1: General Settings
    let general_box = GtkBox::new(Orientation::Vertical, 15);
    general_box.set_margin_top(20);
    general_box.set_margin_bottom(20);
    general_box.set_margin_start(20);
    general_box.set_margin_end(20);
    
    let monitor_title = Label::new(Some("Monitor Settings"));
    monitor_title.add_css_class("title-3");
    general_box.append(&monitor_title);
    
    // CPU Monitor checkbox
    let cpu_check = CheckButton::with_label("Show CPU Monitor");
    cpu_check.set_active(cpu_frame.is_visible());
    let cpu_frame_clone = cpu_frame.clone();
    cpu_check.connect_toggled(move |check| {
        cpu_frame_clone.set_visible(check.is_active());
        // Save to settings
        let mut settings = get_app_settings();
        settings.monitor_visibility.show_cpu = check.is_active();
        let _ = save_app_settings(&settings);
    });
    general_box.append(&cpu_check);
    
    // RAM Monitor checkbox
    let ram_check = CheckButton::with_label("Show RAM Monitor");
    ram_check.set_active(ram_frame.is_visible());
    let ram_frame_clone = ram_frame.clone();
    ram_check.connect_toggled(move |check| {
        ram_frame_clone.set_visible(check.is_active());
        // Save to settings
        let mut settings = get_app_settings();
        settings.monitor_visibility.show_ram = check.is_active();
        let _ = save_app_settings(&settings);
    });
    general_box.append(&ram_check);
    
    // Network Monitor checkbox
    let net_check = CheckButton::with_label("Show Network Monitor");
    net_check.set_active(net_frame.is_visible());
    let net_frame_clone = net_frame.clone();
    net_check.connect_toggled(move |check| {
        net_frame_clone.set_visible(check.is_active());
        // Save to settings
        let mut settings = get_app_settings();
        settings.monitor_visibility.show_network = check.is_active();
        let _ = save_app_settings(&settings);
    });
    general_box.append(&net_check);
    
    // Separator
    let separator = Separator::new(Orientation::Horizontal);
    general_box.append(&separator);
    
    // Command Logging section
    let logging_title = Label::new(Some("Command Logging"));
    logging_title.add_css_class("title-3");
    general_box.append(&logging_title);
    
    let logging_check = CheckButton::with_label("Enable Command Logging");
    logging_check.set_active(is_command_logging_enabled());
    let parent_for_msg = parent.clone();
    logging_check.connect_toggled(move |check| {
        // Save to settings
        let mut settings = get_app_settings();
        settings.enable_command_logging = check.is_active();
        let _ = save_app_settings(&settings);
        
        // Show restart message using a simple dialog
        let msg_dialog = gtk::Window::builder()
            .transient_for(&parent_for_msg)
            .modal(true)
            .title("Settings Updated")
            .default_width(350)
            .default_height(150)
            .build();
        
        let msg_box = GtkBox::new(Orientation::Vertical, 15);
        msg_box.set_margin_top(20);
        msg_box.set_margin_bottom(20);
        msg_box.set_margin_start(20);
        msg_box.set_margin_end(20);
        
        let msg_label = Label::new(Some("Please restart PenEnv for the command logging changes to take effect."));
        msg_label.set_wrap(true);
        msg_box.append(&msg_label);
        
        let ok_btn = Button::with_label("OK");
        ok_btn.set_halign(gtk::Align::Center);
        let msg_dialog_clone = msg_dialog.clone();
        ok_btn.connect_clicked(move |_| {
            msg_dialog_clone.close();
        });
        msg_box.append(&ok_btn);
        
        msg_dialog.set_child(Some(&msg_box));
        msg_dialog.present();
    });
    general_box.append(&logging_check);
    
    let logging_info = Label::new(Some("When disabled, the Log tab will be hidden and commands will not be logged."));
    logging_info.set_wrap(true);
    logging_info.add_css_class("dim-label");
    general_box.append(&logging_info);
    
    notebook.append_page(&general_box, Some(&Label::new(Some("⚙️ General"))));
    
    // TAB 2: Keyboard Shortcuts
    let shortcuts_box = GtkBox::new(Orientation::Vertical, 15);
    shortcuts_box.set_margin_top(20);
    shortcuts_box.set_margin_bottom(20);
    shortcuts_box.set_margin_start(20);
    shortcuts_box.set_margin_end(20);
    
    let shortcuts_title = Label::new(Some("Keyboard Shortcuts"));
    shortcuts_title.add_css_class("title-3");
    shortcuts_box.append(&shortcuts_title);
    
    let shortcuts = get_keyboard_shortcuts();
    
    // Toggle drawer shortcut
    let drawer_box = GtkBox::new(Orientation::Horizontal, 10);
    drawer_box.set_spacing(10);
    let drawer_label = Label::new(Some("Toggle Command Drawer:"));
    drawer_label.set_halign(gtk::Align::Start);
    drawer_label.set_hexpand(true);
    let drawer_entry = Entry::new();
    drawer_entry.set_text(&format!("Ctrl+{}", key_to_display(&shortcuts.toggle_drawer)));
    drawer_entry.set_width_chars(15);
    drawer_entry.set_editable(false);
    drawer_box.append(&drawer_label);
    drawer_box.append(&drawer_entry);
    shortcuts_box.append(&drawer_box);
    
    let change_drawer_btn = Button::with_label("Change");
    change_drawer_btn.add_css_class("flat");
    let parent_clone2 = parent.clone();
    let drawer_entry_clone = drawer_entry.clone();
    change_drawer_btn.connect_clicked(move |_| {
        show_key_capture_dialog(&parent_clone2, "Toggle Command Drawer", "toggle_drawer", &drawer_entry_clone);
    });
    drawer_box.append(&change_drawer_btn);
    
    let clear_drawer_btn = Button::with_label("Clear");
    clear_drawer_btn.add_css_class("flat");
    let drawer_entry_clone2 = drawer_entry.clone();
    clear_drawer_btn.connect_clicked(move |_| {
        let mut settings = get_app_settings();
        settings.keyboard_shortcuts.toggle_drawer = String::new();
        let _ = save_app_settings(&settings);
        drawer_entry_clone2.set_text("Not assigned");
    });
    drawer_box.append(&clear_drawer_btn);
    
    // Insert target shortcut
    let target_box = GtkBox::new(Orientation::Horizontal, 10);
    target_box.set_spacing(10);
    let target_label = Label::new(Some("Insert Target:"));
    target_label.set_halign(gtk::Align::Start);
    target_label.set_hexpand(true);
    let target_entry = Entry::new();
    target_entry.set_text(&format!("Ctrl+{}", key_to_display(&shortcuts.insert_target)));
    target_entry.set_width_chars(15);
    target_entry.set_editable(false);
    target_box.append(&target_label);
    target_box.append(&target_entry);
    shortcuts_box.append(&target_box);
    
    let change_target_btn = Button::with_label("Change");
    change_target_btn.add_css_class("flat");
    let parent_clone3 = parent.clone();
    let target_entry_clone = target_entry.clone();
    change_target_btn.connect_clicked(move |_| {
        show_key_capture_dialog(&parent_clone3, "Insert Target", "insert_target", &target_entry_clone);
    });
    target_box.append(&change_target_btn);
    
    let clear_target_btn = Button::with_label("Clear");
    clear_target_btn.add_css_class("flat");
    let target_entry_clone2 = target_entry.clone();
    clear_target_btn.connect_clicked(move |_| {
        let mut settings = get_app_settings();
        settings.keyboard_shortcuts.insert_target = String::new();
        let _ = save_app_settings(&settings);
        target_entry_clone2.set_text("Not assigned");
    });
    target_box.append(&clear_target_btn);
    
    // Insert timestamp shortcut
    let timestamp_box = GtkBox::new(Orientation::Horizontal, 10);
    timestamp_box.set_spacing(10);
    let timestamp_label = Label::new(Some("Insert Timestamp:"));
    timestamp_label.set_halign(gtk::Align::Start);
    timestamp_label.set_hexpand(true);
    let timestamp_entry = Entry::new();
    timestamp_entry.set_text(&format!("Ctrl+Shift+{}", key_to_display(&shortcuts.insert_timestamp)));
    timestamp_entry.set_width_chars(15);
    timestamp_entry.set_editable(false);
    timestamp_box.append(&timestamp_label);
    timestamp_box.append(&timestamp_entry);
    shortcuts_box.append(&timestamp_box);
    
    let change_timestamp_btn = Button::with_label("Change");
    change_timestamp_btn.add_css_class("flat");
    let parent_clone4 = parent.clone();
    let timestamp_entry_clone = timestamp_entry.clone();
    change_timestamp_btn.connect_clicked(move |_| {
        show_key_capture_dialog(&parent_clone4, "Insert Timestamp", "insert_timestamp", &timestamp_entry_clone);
    });
    timestamp_box.append(&change_timestamp_btn);
    
    let clear_timestamp_btn = Button::with_label("Clear");
    clear_timestamp_btn.add_css_class("flat");
    let timestamp_entry_clone2 = timestamp_entry.clone();
    clear_timestamp_btn.connect_clicked(move |_| {
        let mut settings = get_app_settings();
        settings.keyboard_shortcuts.insert_timestamp = String::new();
        let _ = save_app_settings(&settings);
        timestamp_entry_clone2.set_text("Not assigned");
    });
    timestamp_box.append(&clear_timestamp_btn);
    
    // New shell shortcut
    let new_shell_box = GtkBox::new(Orientation::Horizontal, 10);
    new_shell_box.set_spacing(10);
    let new_shell_label = Label::new(Some("New Shell Tab:"));
    new_shell_label.set_halign(gtk::Align::Start);
    new_shell_label.set_hexpand(true);
    let new_shell_entry = Entry::new();
    if let Some(ref key) = shortcuts.new_shell {
        new_shell_entry.set_text(&format!("Ctrl+Shift+{}", key_to_display(key)));
    } else {
        new_shell_entry.set_text("Not assigned");
    }
    new_shell_entry.set_width_chars(15);
    new_shell_entry.set_editable(false);
    new_shell_box.append(&new_shell_label);
    new_shell_box.append(&new_shell_entry);
    shortcuts_box.append(&new_shell_box);
    
    let change_new_shell_btn = Button::with_label("Change");
    change_new_shell_btn.add_css_class("flat");
    let parent_clone5 = parent.clone();
    let new_shell_entry_clone = new_shell_entry.clone();
    change_new_shell_btn.connect_clicked(move |_| {
        show_key_capture_dialog(&parent_clone5, "New Shell Tab", "new_shell", &new_shell_entry_clone);
    });
    new_shell_box.append(&change_new_shell_btn);
    
    let clear_new_shell_btn = Button::with_label("Clear");
    clear_new_shell_btn.add_css_class("flat");
    let new_shell_entry_clone2 = new_shell_entry.clone();
    clear_new_shell_btn.connect_clicked(move |_| {
        let mut settings = get_app_settings();
        settings.keyboard_shortcuts.new_shell = None;
        let _ = save_app_settings(&settings);
        new_shell_entry_clone2.set_text("Not assigned");
    });
    new_shell_box.append(&clear_new_shell_btn);
    
    // New split view shortcut
    let new_split_box = GtkBox::new(Orientation::Horizontal, 10);
    new_split_box.set_spacing(10);
    let new_split_label = Label::new(Some("New Split View:"));
    new_split_label.set_halign(gtk::Align::Start);
    new_split_label.set_hexpand(true);
    let new_split_entry = Entry::new();
    if let Some(ref key) = shortcuts.new_split {
        new_split_entry.set_text(&format!("Ctrl+Shift+{}", key_to_display(key)));
    } else {
        new_split_entry.set_text("Not assigned");
    }
    new_split_entry.set_width_chars(15);
    new_split_entry.set_editable(false);
    new_split_box.append(&new_split_label);
    new_split_box.append(&new_split_entry);
    shortcuts_box.append(&new_split_box);
    
    let change_new_split_btn = Button::with_label("Change");
    change_new_split_btn.add_css_class("flat");
    let parent_clone6 = parent.clone();
    let new_split_entry_clone = new_split_entry.clone();
    change_new_split_btn.connect_clicked(move |_| {
        show_key_capture_dialog(&parent_clone6, "New Split View", "new_split", &new_split_entry_clone);
    });
    new_split_box.append(&change_new_split_btn);
    
    let clear_new_split_btn = Button::with_label("Clear");
    clear_new_split_btn.add_css_class("flat");
    let new_split_entry_clone2 = new_split_entry.clone();
    clear_new_split_btn.connect_clicked(move |_| {
        let mut settings = get_app_settings();
        settings.keyboard_shortcuts.new_split = None;
        let _ = save_app_settings(&settings);
        new_split_entry_clone2.set_text("Not assigned");
    });
    new_split_box.append(&clear_new_split_btn);
    
    notebook.append_page(&shortcuts_box, Some(&Label::new(Some("⌨️ Shortcuts"))));
    
    // TAB 3: Custom Commands
    let commands_box = GtkBox::new(Orientation::Vertical, 10);
    commands_box.set_margin_top(20);
    commands_box.set_margin_bottom(20);
    commands_box.set_margin_start(20);
    commands_box.set_margin_end(20);
    
    let commands_title = Label::new(Some("Custom Commands"));
    commands_title.add_css_class("title-3");
    commands_box.append(&commands_title);
    
    // Commands list
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_min_content_height(250);
    
    let list_box = ListBox::new();
    list_box.add_css_class("boxed-list");
    
    // Function to populate the command list
    let populate_commands = {
        let list_box = list_box.clone();
        let parent = parent.clone();
        let dialog = dialog.clone();
        
        move || {
            // Clear existing rows
            while let Some(row) = list_box.first_child() {
                list_box.remove(&row);
            }
            
            // Load and populate with current commands
            let commands = load_custom_commands();
            
            if commands.is_empty() {
                let empty_label = Label::new(Some("No custom commands yet. Click \"Add Command\" to create one."));
                empty_label.add_css_class("dim-label");
                empty_label.set_margin_top(20);
                empty_label.set_margin_bottom(20);
                list_box.append(&empty_label);
            } else {
                for (idx, cmd) in commands.iter().enumerate() {
                    let row_box = GtkBox::new(Orientation::Horizontal, 10);
                    row_box.set_margin_top(8);
                    row_box.set_margin_bottom(8);
                    row_box.set_margin_start(10);
                    row_box.set_margin_end(10);
                    
                    // Command info
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
                    
                    // Action buttons
                    let btn_box = GtkBox::new(Orientation::Horizontal, 5);
                    
                    let edit_btn = Button::with_label("✏️ Edit");
                    let delete_btn = Button::with_label("🗑️");
                    delete_btn.add_css_class("destructive-action");
                    
                    // Edit handler - reopen settings to refresh
                    let parent_clone = parent.clone();
                    let dialog_clone = dialog.clone();
                    let cmd_clone = cmd.clone();
                    let cpu_frame_clone = cpu_frame.clone();
                    let ram_frame_clone = ram_frame.clone();
                    let net_frame_clone = net_frame.clone();
                    edit_btn.connect_clicked(move |_| {
                        let parent_ref = parent_clone.clone();
                        let dialog_ref = dialog_clone.clone();
                        let cpu_ref = cpu_frame_clone.clone();
                        let ram_ref = ram_frame_clone.clone();
                        let net_ref = net_frame_clone.clone();
                        show_edit_command_dialog(&parent_clone, idx, cmd_clone.clone(), move || {
                            // Close current dialog and reopen settings on commands tab
                            dialog_ref.close();
                            show_settings_dialog(&parent_ref, &cpu_ref, &ram_ref, &net_ref);
                        });
                    });
                    
                    // Delete handler - refresh the dialog
                    let parent_clone2 = parent.clone();
                    let dialog_clone2 = dialog.clone();
                    let cpu_frame_clone2 = cpu_frame.clone();
                    let ram_frame_clone2 = ram_frame.clone();
                    let net_frame_clone2 = net_frame.clone();
                    delete_btn.connect_clicked(move |_| {
                        if let Err(e) = delete_custom_command(idx) {
                            eprintln!("Failed to delete command: {}", e);
                        } else {
                            // Close and reopen settings on commands tab
                            dialog_clone2.close();
                            show_settings_dialog(&parent_clone2, &cpu_frame_clone2, &ram_frame_clone2, &net_frame_clone2);
                        }
                    });
                    
                    btn_box.append(&edit_btn);
                    btn_box.append(&delete_btn);
                    
                    row_box.append(&info_box);
                    row_box.append(&btn_box);
                    
                    list_box.append(&row_box);
                }
            }
        }
    };
    
    // Initial population
    populate_commands();
    
    scrolled.set_child(Some(&list_box));
    commands_box.append(&scrolled);
    
    // Add button for new command
    let add_cmd_btn = Button::with_label("➕ Add Command");
    add_cmd_btn.add_css_class("suggested-action");
    let parent_clone_cmd = parent.clone();
    let dialog_clone_cmd = dialog.clone();
    let cpu_frame_clone_cmd = cpu_frame.clone();
    let ram_frame_clone_cmd = ram_frame.clone();
    let net_frame_clone_cmd = net_frame.clone();
    add_cmd_btn.connect_clicked(move |_| {
        let parent_ref = parent_clone_cmd.clone();
        let dialog_ref = dialog_clone_cmd.clone();
        let cpu_ref = cpu_frame_clone_cmd.clone();
        let ram_ref = ram_frame_clone_cmd.clone();
        let net_ref = net_frame_clone_cmd.clone();
        show_add_command_dialog(&parent_clone_cmd, move || {
            // Close and reopen settings on commands tab
            dialog_ref.close();
            show_settings_dialog(&parent_ref, &cpu_ref, &ram_ref, &net_ref);
        });
    });
    commands_box.append(&add_cmd_btn);
    
    notebook.append_page(&commands_box, Some(&Label::new(Some("📝 Commands"))));
    
    // Add notebook to main box
    main_box.append(&notebook);
    
    // Close button at bottom
    let bottom_box = GtkBox::new(Orientation::Horizontal, 10);
    bottom_box.set_margin_top(10);
    bottom_box.set_margin_bottom(10);
    bottom_box.set_margin_start(20);
    bottom_box.set_margin_end(20);
    bottom_box.set_halign(gtk::Align::End);
    bottom_box.set_halign(gtk::Align::End);
    let close_btn = Button::with_label("Close");
    let dialog_clone = dialog.clone();
    close_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });
    bottom_box.append(&close_btn);
    main_box.append(&bottom_box);
    
    dialog.set_child(Some(&main_box));
    dialog.present();
}

/// Converts a key name to display format
fn key_to_display(key: &str) -> String {
    match key {
        "grave" => "`".to_string(),
        "t" => "T".to_string(),
        "Return" => "Enter".to_string(),
        "space" => "Space".to_string(),
        _ => key.to_uppercase(),
    }
}

/// Shows a dialog to capture a new keyboard shortcut
fn show_key_capture_dialog(parent: &ApplicationWindow, label: &str, shortcut_name: &str, display_entry: &Entry) {
    let dialog = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(&format!("Change {}", label))
        .default_width(400)
        .default_height(150)
        .build();
    
    let dialog_box = GtkBox::new(Orientation::Vertical, 15);
    dialog_box.set_margin_top(20);
    dialog_box.set_margin_bottom(20);
    dialog_box.set_margin_start(20);
    dialog_box.set_margin_end(20);
    
    let info = Label::new(Some(&format!("Press Ctrl{} + any key for '{}'", 
        if label.contains("Timestamp") || label.contains("Shell") || label.contains("Split") { "+Shift" } else { "" }, label)));
    info.set_wrap(true);
    dialog_box.append(&info);
    
    let current_key = Label::new(Some("Waiting for key..."));
    current_key.add_css_class("title-3");
    dialog_box.append(&current_key);
    
    let button_box = GtkBox::new(Orientation::Horizontal, 10);
    button_box.set_halign(gtk::Align::End);
    
    let cancel_btn = Button::with_label("Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });
    button_box.append(&cancel_btn);
    dialog_box.append(&button_box);
    
    let key_controller = gtk::EventControllerKey::new();
    let shortcut_name = shortcut_name.to_string();
    let display_entry = display_entry.clone();
    let dialog_clone2 = dialog.clone();
    let current_key_clone = current_key.clone();
    
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let key_name = keyval.name().unwrap_or_default().to_string();
            let has_shift = modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            
            // Update the display
            let display_text = if has_shift {
                format!("Ctrl+Shift+{}", key_to_display(&key_name))
            } else {
                format!("Ctrl+{}", key_to_display(&key_name))
            };
            current_key_clone.set_text(&display_text);
            
            // Save the shortcut to settings
            let mut settings = get_app_settings();
            match shortcut_name.as_str() {
                "toggle_drawer" => settings.keyboard_shortcuts.toggle_drawer = key_name.clone(),
                "insert_target" => settings.keyboard_shortcuts.insert_target = key_name.clone(),
                "insert_timestamp" => settings.keyboard_shortcuts.insert_timestamp = key_name.clone(),
                "new_shell" => settings.keyboard_shortcuts.new_shell = Some(key_name.clone()),
                "new_split" => settings.keyboard_shortcuts.new_split = Some(key_name.clone()),
                _ => {}
            }
            
            if save_app_settings(&settings).is_ok() {
                display_entry.set_text(&display_text);
                
                // Close dialog after a short delay
                glib::timeout_add_local_once(std::time::Duration::from_millis(500), {
                    let dialog = dialog_clone2.clone();
                    move || {
                        dialog.close();
                    }
                });
            }
            
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    
    dialog.add_controller(key_controller);
    dialog.set_child(Some(&dialog_box));
    dialog.present();
}

fn show_add_command_dialog<F>(parent: &ApplicationWindow, on_save: F)
where
    F: Fn() + 'static,
{
    let dialog = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Add Custom Command")
        .default_width(500)
        .default_height(400)
        .build();
    
    let dialog_box = GtkBox::new(Orientation::Vertical, 15);
    dialog_box.set_margin_top(20);
    dialog_box.set_margin_bottom(20);
    dialog_box.set_margin_start(20);
    dialog_box.set_margin_end(20);
    
    let title = Label::new(Some("Create Custom Command"));
    title.add_css_class("title-2");
    dialog_box.append(&title);
    
    // Name field
    let name_label = Label::new(Some("Command Name:"));
    name_label.set_halign(gtk::Align::Start);
    dialog_box.append(&name_label);
    let name_entry = Entry::new();
    name_entry.set_placeholder_text(Some("e.g., Quick Scan"));
    dialog_box.append(&name_entry);
    
    // Command field
    let command_label = Label::new(Some("Command:"));
    command_label.set_halign(gtk::Align::Start);
    dialog_box.append(&command_label);
    let command_entry = Entry::new();
    command_entry.set_placeholder_text(Some("e.g., nmap -sV {target}"));
    dialog_box.append(&command_entry);
    
    // Description field
    let desc_label = Label::new(Some("Description:"));
    desc_label.set_halign(gtk::Align::Start);
    dialog_box.append(&desc_label);
    let desc_entry = Entry::new();
    desc_entry.set_placeholder_text(Some("e.g., Fast service version scan"));
    dialog_box.append(&desc_entry);
    
    // Category field
    let cat_label = Label::new(Some("Category:"));
    cat_label.set_halign(gtk::Align::Start);
    dialog_box.append(&cat_label);
    let cat_entry = Entry::new();
    cat_entry.set_placeholder_text(Some("e.g., Custom"));
    dialog_box.append(&cat_entry);
    
    // Info message
    let info = Label::new(Some("Tip: Use {target} as a placeholder for target selection"));
    info.add_css_class("dim-label");
    info.set_wrap(true);
    dialog_box.append(&info);
    
    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 10);
    button_box.set_halign(gtk::Align::End);
    
    let cancel_btn = Button::with_label("Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });
    
    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    let dialog_clone2 = dialog.clone();
    save_btn.connect_clicked(move |_| {
        let name = name_entry.text().to_string();
        let command = command_entry.text().to_string();
        let description = desc_entry.text().to_string();
        let category = cat_entry.text().to_string();
        
        if name.is_empty() || command.is_empty() {
            eprintln!("Name and command are required");
            return;
        }
        
        let cmd_template = CommandTemplate {
            name: name.clone(),
            command: command.clone(),
            description: if description.is_empty() { "Custom command".to_string() } else { description },
            category: if category.is_empty() { "Custom".to_string() } else { category },
        };
        
        match save_custom_command(cmd_template) {
            Ok(_) => {
                on_save();
                dialog_clone2.close();
            }
            Err(e) => {
                eprintln!("Failed to save custom command: {}", e);
            }
        }
    });
    
    button_box.append(&cancel_btn);
    button_box.append(&save_btn);
    dialog_box.append(&button_box);
    
    dialog.set_child(Some(&dialog_box));
    dialog.present();
}

fn show_edit_command_dialog<F>(parent: &ApplicationWindow, index: usize, cmd: CommandTemplate, on_save: F)
where
    F: Fn() + 'static,
{
    let dialog = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Edit Custom Command")
        .default_width(500)
        .default_height(400)
        .build();
    
    let dialog_box = GtkBox::new(Orientation::Vertical, 15);
    dialog_box.set_margin_top(20);
    dialog_box.set_margin_bottom(20);
    dialog_box.set_margin_start(20);
    dialog_box.set_margin_end(20);
    
    let title = Label::new(Some("Edit Command"));
    title.add_css_class("title-2");
    dialog_box.append(&title);
    
    // Name field
    let name_label = Label::new(Some("Command Name:"));
    name_label.set_halign(gtk::Align::Start);
    dialog_box.append(&name_label);
    let name_entry = Entry::new();
    name_entry.set_text(&cmd.name);
    dialog_box.append(&name_entry);
    
    // Command field
    let command_label = Label::new(Some("Command:"));
    command_label.set_halign(gtk::Align::Start);
    dialog_box.append(&command_label);
    let command_entry = Entry::new();
    command_entry.set_text(&cmd.command);
    dialog_box.append(&command_entry);
    
    // Description field
    let desc_label = Label::new(Some("Description:"));
    desc_label.set_halign(gtk::Align::Start);
    dialog_box.append(&desc_label);
    let desc_entry = Entry::new();
    desc_entry.set_text(&cmd.description);
    dialog_box.append(&desc_entry);
    
    // Category field
    let cat_label = Label::new(Some("Category:"));
    cat_label.set_halign(gtk::Align::Start);
    dialog_box.append(&cat_label);
    let cat_entry = Entry::new();
    cat_entry.set_text(&cmd.category);
    dialog_box.append(&cat_entry);
    
    // Info message
    let info = Label::new(Some("Tip: Use {target} as a placeholder for target selection"));
    info.add_css_class("dim-label");
    info.set_wrap(true);
    dialog_box.append(&info);
    
    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 10);
    button_box.set_halign(gtk::Align::End);
    
    let cancel_btn = Button::with_label("Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });
    
    let save_btn = Button::with_label("💾");
    save_btn.add_css_class("suggested-action");
    let dialog_clone2 = dialog.clone();
    let name_entry = name_entry.clone();
    let command_entry = command_entry.clone();
    let desc_entry = desc_entry.clone();
    let cat_entry = cat_entry.clone();
    save_btn.connect_clicked(move |_| {
        let name = name_entry.text().to_string();
        let command = command_entry.text().to_string();
        let description = desc_entry.text().to_string();
        let category = cat_entry.text().to_string();
        
        if name.is_empty() || command.is_empty() {
            eprintln!("Name and command are required");
            return;
        }
        
        let cmd_template = CommandTemplate {
            name: name.clone(),
            command: command.clone(),
            description: if description.is_empty() { "Custom command".to_string() } else { description },
            category: if category.is_empty() { "Custom".to_string() } else { category },
        };
        
        match update_custom_command(index, cmd_template) {
            Ok(_) => {
                on_save();
                dialog_clone2.close();
            }
            Err(e) => {
                eprintln!("Failed to update custom command: {}", e);
            }
        }
    });
    
    button_box.append(&cancel_btn);
    button_box.append(&save_btn);
    dialog_box.append(&button_box);
    
    dialog.set_child(Some(&dialog_box));
    dialog.present();
}

fn create_main_window(app: &Application) {
    // Load app settings at startup
    let settings = load_app_settings();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("PenEnv - Pentesting Environment")
        .default_width(1200)
        .default_height(800)
        .build();
    
    // Set application icon name (GTK4 uses icon themes)
    window.set_icon_name(Some("penenv"));

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // Create notebook for tabs
    let notebook = Notebook::builder()
        .scrollable(true)
        .build();

    // Shell counter for tracking shell tab numbers
    let shell_counter: Rc<RefCell<usize>> = Rc::new(RefCell::new(5));

    // Tab 1: Targets
    let targets_page = create_text_editor(&get_file_path("targets.txt").to_string_lossy().to_string(), Some(notebook.clone()));
    notebook.append_page(&targets_page, Some(&Label::new(Some("📋 Targets"))));

    // Tab 2: Notes
    let notes_page = create_text_editor(&get_file_path("notes.md").to_string_lossy().to_string(), None);
    notebook.append_page(&notes_page, Some(&Label::new(Some("📝 Notes"))));

    // Tab 3: Command Log (only if logging is enabled)
    if is_command_logging_enabled() {
        let log_page = create_readonly_viewer(&get_file_path("commands.log").to_string_lossy().to_string());
        notebook.append_page(&log_page, Some(&Label::new(Some("📜 Log"))));
    }

    // Tab 4 (or 3 if no log): First Shell
    let shell_page = create_shell_tab(4, notebook.clone(), Some(shell_counter.clone()));
    let shell_label = create_editable_tab_label("💻 Shell 4", &notebook);
    notebook.append_page(&shell_page, Some(&shell_label));

    // Toolbar with buttons
    let toolbar = GtkBox::new(Orientation::Horizontal, 5);
    toolbar.set_margin_top(5);
    toolbar.set_margin_bottom(5);
    toolbar.set_margin_start(5);
    toolbar.set_margin_end(5);

    let new_shell_btn = Button::with_label("➕ New Shell");
    let split_mode_btn = Button::with_label("⚡ Split Mode");
    let close_tab_btn = Button::with_label("❌ Close Tab");
    let settings_btn = Button::with_label("⚙️");
    
    // System monitors on the right
    let monitors_box = GtkBox::new(Orientation::Horizontal, 5);
    monitors_box.set_halign(gtk::Align::End);
    monitors_box.set_hexpand(true);
    
    // CPU Monitor
    let cpu_frame = Frame::new(None);
    cpu_frame.set_visible(settings.monitor_visibility.show_cpu);
    let cpu_box = GtkBox::new(Orientation::Vertical, 1);
    cpu_box.set_margin_top(2);
    cpu_box.set_margin_bottom(2);
    cpu_box.set_margin_start(6);
    cpu_box.set_margin_end(6);
    let cpu_label = Label::new(Some("CPU"));
    cpu_label.add_css_class("caption");
    let cpu_bar = ProgressBar::new();
    cpu_bar.set_width_request(60);
    cpu_bar.set_show_text(true);
    cpu_box.append(&cpu_label);
    cpu_box.append(&cpu_bar);
    cpu_frame.set_child(Some(&cpu_box));
    
    // RAM Monitor
    let ram_frame = Frame::new(None);
    ram_frame.set_visible(settings.monitor_visibility.show_ram);
    let ram_box = GtkBox::new(Orientation::Vertical, 1);
    ram_box.set_margin_top(2);
    ram_box.set_margin_bottom(2);
    ram_box.set_margin_start(6);
    ram_box.set_margin_end(6);
    let ram_label = Label::new(Some("RAM"));
    ram_label.add_css_class("caption");
    let ram_bar = ProgressBar::new();
    ram_bar.set_width_request(60);
    ram_bar.set_show_text(true);
    ram_box.append(&ram_label);
    ram_box.append(&ram_bar);
    ram_frame.set_child(Some(&ram_box));
    
    // Network Monitor
    let net_frame = Frame::new(None);
    net_frame.set_visible(settings.monitor_visibility.show_network);
    let net_box = GtkBox::new(Orientation::Vertical, 1);
    net_box.set_margin_top(2);
    net_box.set_margin_bottom(2);
    net_box.set_margin_start(6);
    net_box.set_margin_end(6);
    net_box.set_size_request(130, -1);
    let net_label = Label::new(Some("NET"));
    net_label.add_css_class("caption");
    let net_text = Label::new(Some("↓ 0 KB/s ↑ 0 KB/s"));
    net_text.set_size_request(118, -1);
    net_text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    net_text.set_xalign(0.5);
    net_text.add_css_class("caption");
    net_box.append(&net_label);
    net_box.append(&net_text);
    net_frame.set_child(Some(&net_box));
    
    monitors_box.append(&cpu_frame);
    monitors_box.append(&ram_frame);
    monitors_box.append(&net_frame);
    
    let notebook_clone = notebook.clone();
    let shell_counter_clone = Rc::clone(&shell_counter);
    new_shell_btn.connect_clicked(move |_| {
        create_new_shell_tab(&notebook_clone, &shell_counter_clone);
    });

    let notebook_clone2 = notebook.clone();
    let shell_counter_clone2 = Rc::clone(&shell_counter);
    split_mode_btn.connect_clicked(move |_| {
        create_new_split_view_tab(&notebook_clone2, &shell_counter_clone2);
    });

    let notebook_clone3 = notebook.clone();
    close_tab_btn.connect_clicked(move |_| {
        if let Some(page_num) = notebook_clone3.current_page() {
            // Don't close first 3 tabs (targets, notes, log)
            if page_num >= 3 {
                notebook_clone3.remove_page(Some(page_num));
            }
        }
    });

    // Settings button handler
    let window_clone = window.clone();
    let cpu_frame_clone = cpu_frame.clone();
    let ram_frame_clone = ram_frame.clone();
    let net_frame_clone = net_frame.clone();
    settings_btn.connect_clicked(move |_| {
        show_settings_dialog(&window_clone, &cpu_frame_clone, &ram_frame_clone, &net_frame_clone);
    });

    toolbar.append(&new_shell_btn);
    toolbar.append(&split_mode_btn);
    toolbar.append(&close_tab_btn);
    toolbar.append(&settings_btn);
    toolbar.append(&monitors_box);

    // Initialize system monitoring
    let sys = Rc::new(RefCell::new(System::new_all()));
    let networks = Rc::new(RefCell::new(Networks::new_with_refreshed_list()));
    
    // Store previous network stats for calculating rates
    let prev_rx = Rc::new(RefCell::new(0u64));
    let prev_tx = Rc::new(RefCell::new(0u64));
    
    // Update monitors every second
    let sys_clone = Rc::clone(&sys);
    let networks_clone = Rc::clone(&networks);
    let prev_rx_clone = Rc::clone(&prev_rx);
    let prev_tx_clone = Rc::clone(&prev_tx);
    let cpu_bar_clone = cpu_bar.clone();
    let ram_bar_clone = ram_bar.clone();
    let net_text_clone = net_text.clone();
    
    glib::timeout_add_seconds_local(1, move || {
        // Update system info
        sys_clone.borrow_mut().refresh_all();
        networks_clone.borrow_mut().refresh();
        
        let sys_ref = sys_clone.borrow();
        
        // CPU usage (global)
        let cpu_usage = sys_ref.global_cpu_usage();
        cpu_bar_clone.set_fraction((cpu_usage / 100.0) as f64);
        cpu_bar_clone.set_text(Some(&format!("{:.0}%", cpu_usage)));
        
        // RAM usage
        let total_mem = sys_ref.total_memory() as f64;
        let used_mem = sys_ref.used_memory() as f64;
        let mem_percent = if total_mem > 0.0 { used_mem / total_mem } else { 0.0 };
        ram_bar_clone.set_fraction(mem_percent);
        ram_bar_clone.set_text(Some(&format!("{:.0}%", mem_percent * 100.0)));
        
        // Network usage (calculate rates)
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_name, data) in networks_clone.borrow().iter() {
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }
        
        let prev_rx_val = *prev_rx_clone.borrow();
        let prev_tx_val = *prev_tx_clone.borrow();
        
        let rx_rate = if total_rx >= prev_rx_val { total_rx - prev_rx_val } else { 0 };
        let tx_rate = if total_tx >= prev_tx_val { total_tx - prev_tx_val } else { 0 };
        
        *prev_rx_clone.borrow_mut() = total_rx;
        *prev_tx_clone.borrow_mut() = total_tx;
        
        // Format network rates
        let rx_str = format_bytes(rx_rate);
        let tx_str = format_bytes(tx_rate);
        net_text_clone.set_text(&format!("↓ {} ↑ {}", rx_str, tx_str));
        
        glib::ControlFlow::Continue
    });

    // Add handler to refresh notes tab when switched to
    notebook.connect_switch_page(move |notebook, _, page_num| {
        // If switching to notes tab (index 1), refresh it
        if page_num == 1 {
            if let Some(notes_page) = notebook.nth_page(Some(1)) {
                if let Some(notes_box) = notes_page.downcast_ref::<GtkBox>() {
                    // Find the TextView in the notes page (first child is ScrolledWindow)
                    if let Some(child) = notes_box.first_child() {
                        if let Some(scrolled) = child.downcast_ref::<ScrolledWindow>() {
                            if let Some(text_view) = scrolled.child() {
                                if let Some(text_view) = text_view.downcast_ref::<TextView>() {
                                    // Reload notes content
                                    let notes_path = get_file_path("notes.md");
                                    if let Ok(content) = fs::read_to_string(notes_path) {
                                        text_view.buffer().set_text(&content);
                                        apply_markdown_highlighting(text_view);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    main_box.append(&toolbar);
    main_box.append(&notebook);

    // Status bar with creator and version
    let status_box = GtkBox::new(Orientation::Horizontal, 10);
    status_box.set_margin_top(5);
    status_box.set_margin_bottom(5);
    status_box.set_margin_start(10);
    status_box.set_margin_end(10);
    
    let creator_label = Label::new(Some("Created by undergroundbiscuitclub"));
    creator_label.set_halign(gtk::Align::Start);
    creator_label.set_hexpand(true);
    
    let version_label = Label::new(Some(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    version_label.set_halign(gtk::Align::End);
    version_label.set_opacity(0.7);
    
    status_box.append(&creator_label);
    status_box.append(&version_label);
    
    main_box.append(&status_box);

    // Add global keyboard shortcuts for switching tabs (Ctrl+1 through Ctrl+9)
    // and creating new tabs (Ctrl+Shift+N for shell, Ctrl+Shift+S for split)
    let key_controller = gtk::EventControllerKey::new();
    let notebook_clone = notebook.clone();
    let new_shell_btn_clone = new_shell_btn.clone();
    let split_mode_btn_clone = split_mode_btn.clone();
    
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            
            // Check for Ctrl+Shift combinations for new tab creation
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                // New shell shortcut
                if let Some(ref new_shell_key) = shortcuts.new_shell {
                    if &key_name == new_shell_key {
                        new_shell_btn_clone.emit_clicked();
                        return gtk::glib::Propagation::Stop;
                    }
                }
                
                // New split view shortcut
                if let Some(ref new_split_key) = shortcuts.new_split {
                    if &key_name == new_split_key {
                        split_mode_btn_clone.emit_clicked();
                        return gtk::glib::Propagation::Stop;
                    }
                }
            }
            
            // Map Ctrl+1 through Ctrl+9 to tabs 0-8
            let page_num = match keyval {
                gtk::gdk::Key::_1 => Some(0),
                gtk::gdk::Key::_2 => Some(1),
                gtk::gdk::Key::_3 => Some(2),
                gtk::gdk::Key::_4 => Some(3),
                gtk::gdk::Key::_5 => Some(4),
                gtk::gdk::Key::_6 => Some(5),
                gtk::gdk::Key::_7 => Some(6),
                gtk::gdk::Key::_8 => Some(7),
                gtk::gdk::Key::_9 => Some(8),
                _ => None,
            };
            
            if let Some(page) = page_num {
                if page < notebook_clone.n_pages() {
                    notebook_clone.set_current_page(Some(page));
                    return gtk::glib::Propagation::Stop;
                }
            }
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.set_child(Some(&main_box));
    window.present();
}

fn create_editable_tab_label(initial_text: &str, _notebook: &Notebook) -> GtkBox {
    let tab_box = GtkBox::new(Orientation::Horizontal, 5);
    let label = Label::new(Some(initial_text));
    
    // Add double-click gesture to edit label
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1); // Left mouse button
    
    let label_clone = label.clone();
    gesture.connect_released(move |_gesture, n_press, _, _| {
        if n_press == 2 { // Double-click
            // Create dialog for renaming
            let dialog = gtk::Window::builder()
                .title("Rename Tab")
                .modal(true)
                .default_width(300)
                .default_height(100)
                .build();
            
            let dialog_box = GtkBox::new(Orientation::Vertical, 10);
            dialog_box.set_margin_top(10);
            dialog_box.set_margin_bottom(10);
            dialog_box.set_margin_start(10);
            dialog_box.set_margin_end(10);
            
            let entry = gtk::Entry::new();
            entry.set_text(&label_clone.text());
            entry.set_activates_default(true);
            
            let button_box = GtkBox::new(Orientation::Horizontal, 5);
            button_box.set_halign(gtk::Align::End);
            
            let ok_btn = Button::with_label("OK");
            ok_btn.set_receives_default(true);
            let cancel_btn = Button::with_label("Cancel");
            
            let dialog_clone = dialog.clone();
            let label_clone2 = label_clone.clone();
            let entry_clone = entry.clone();
            ok_btn.connect_clicked(move |_| {
                let new_name = entry_clone.text();
                if !new_name.is_empty() {
                    label_clone2.set_text(&new_name);
                }
                dialog_clone.close();
            });
            
            let dialog_clone2 = dialog.clone();
            cancel_btn.connect_clicked(move |_| {
                dialog_clone2.close();
            });
            
            button_box.append(&cancel_btn);
            button_box.append(&ok_btn);
            
            dialog_box.append(&entry);
            dialog_box.append(&button_box);
            
            dialog.set_child(Some(&dialog_box));
            dialog.present();
        }
    });
    
    label.add_controller(gesture);
    tab_box.append(&label);
    
    tab_box
}

fn create_text_editor(file_path: &str, notebook: Option<Notebook>) -> GtkBox {
    let container = GtkBox::new(Orientation::Vertical, 5);
    container.set_margin_top(5);
    container.set_margin_bottom(5);
    container.set_margin_start(5);
    container.set_margin_end(5);

    // Apply markdown syntax highlighting for notes.md
    let is_notes = file_path == get_file_path("notes.md").to_string_lossy().to_string();
    
    // Add target selector for notes tab
    let target_combo_opt = if is_notes {
        let target_box = GtkBox::new(Orientation::Horizontal, 5);
        let target_combo = gtk::ComboBoxText::new();
        target_combo.set_hexpand(true);
        
        // Load targets using helper function
        let targets = load_targets();
        for target in &targets {
            target_combo.append_text(target);
        }
        if !targets.is_empty() {
            target_combo.set_active(Some(0));
        }
        
        target_box.append(&target_combo);
        container.append(&target_box);
        Some((target_box, target_combo))
    } else {
        None
    };

    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let text_view = TextView::builder()
        .monospace(true)
        .build();

    // Load file content
    if let Ok(content) = fs::read_to_string(file_path) {
        text_view.buffer().set_text(&content);
    }
    
    if is_notes {
        apply_markdown_highlighting(&text_view);
    }

    scrolled.set_child(Some(&text_view));

    // Auto-save for notes.md with debounce
    if is_notes {
        let file_path_owned = file_path.to_string();
        let text_view_clone = text_view.clone();
        let save_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let save_timeout_clone = Rc::clone(&save_timeout_id);
        
        text_view.buffer().connect_changed(move |buffer| {
            let file_path = file_path_owned.clone();
            let text_view_ref = text_view_clone.clone();
            
            // Cancel any pending save
            if let Some(id) = save_timeout_clone.borrow_mut().take() {
                id.remove();
            }
            
            // Get text and apply highlighting immediately (fast operation)
            apply_markdown_highlighting(&text_view_ref);
            
            // Debounce file save - wait 500ms after last change before saving
            let save_timeout_inner = Rc::clone(&save_timeout_clone);
            let buffer_clone = buffer.clone();
            let source_id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                let start = buffer_clone.start_iter();
                let end = buffer_clone.end_iter();
                let text = buffer_clone.text(&start, &end, false);
                let _ = fs::write(&file_path, text.as_str());
                *save_timeout_inner.borrow_mut() = None;
                glib::ControlFlow::Break
            });
            *save_timeout_clone.borrow_mut() = Some(source_id);
        });
        
        // Add insert target button for notes
        if let Some((target_box, target_combo)) = target_combo_opt {
            let insert_target_btn = Button::with_label("🎯 Insert Target");
            let text_view_clone2 = text_view.clone();
            insert_target_btn.connect_clicked(move |_| {
                if let Some(target) = target_combo.active_text() {
                    let buffer = text_view_clone2.buffer();
                    buffer.insert_at_cursor(&target.to_string());
                    text_view_clone2.grab_focus();
                }
            });
            target_box.append(&insert_target_btn);
        }
    }

    // Save button
    let button_box = GtkBox::new(Orientation::Horizontal, 5);
    let save_btn = Button::with_label("💾 Save");
    
    let file_path_owned = file_path.to_string();
    let text_view_clone = text_view.clone();
    let notebook_clone = notebook.clone();
    save_btn.connect_clicked(move |_| {
        let buffer = text_view_clone.buffer();
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, false);
        let _ = fs::write(&file_path_owned, text.as_str());
        
        // If this is targets.txt, reload all shell tabs
        if file_path_owned == get_file_path("targets.txt").to_string_lossy().to_string() {
            if let Some(ref nb) = notebook_clone {
                reload_targets_in_shells(nb);
            }
        }
    });

    button_box.append(&save_btn);
    button_box.append(&Label::new(Some(file_path)));

    // Add Ctrl+S keyboard shortcut
    let key_controller = gtk::EventControllerKey::new();
    let file_path_owned2 = file_path.to_string();
    let text_view_clone2 = text_view.clone();
    let notebook_clone2 = notebook.clone();
    let text_view_clone3 = text_view.clone();
    let text_view_clone4 = text_view.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            if keyval == gtk::gdk::Key::s {
                let buffer = text_view_clone2.buffer();
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false);
                let _ = fs::write(&file_path_owned2, text.as_str());
                
                // If this is targets.txt, reload all shell tabs
                if file_path_owned2 == get_file_path("targets.txt").to_string_lossy().to_string() {
                    if let Some(ref nb) = notebook_clone2 {
                        reload_targets_in_shells(nb);
                    }
                }
                
                return gtk::glib::Propagation::Stop;
            }
            
            // Add target insertion shortcut for notes
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            if key_name == shortcuts.insert_target {
                show_target_selector_for_textview(&text_view_clone3);
                return gtk::glib::Propagation::Stop;
            }
            
            // Check for Ctrl+Shift+T (timestamp insertion)
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) && key_name == shortcuts.insert_timestamp {
                let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S] ").to_string();
                let buffer = text_view_clone4.buffer();
                buffer.insert_at_cursor(&timestamp);
                return gtk::glib::Propagation::Stop;
            }
        }
        gtk::glib::Propagation::Proceed
    });
    text_view.add_controller(key_controller);

    container.append(&scrolled);
    container.append(&button_box);

    container
}

fn create_readonly_viewer(file_path: &str) -> GtkBox {
    let container = GtkBox::new(Orientation::Vertical, 5);
    container.set_margin_top(5);
    container.set_margin_bottom(5);
    container.set_margin_start(5);
    container.set_margin_end(5);

    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let text_view = TextView::builder()
        .monospace(true)
        .editable(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();

    // Load file content and scroll to end
    if let Ok(content) = fs::read_to_string(file_path) {
        text_view.buffer().set_text(&content);
        // Scroll to end
        let buffer = text_view.buffer();
        let mut end_iter = buffer.end_iter();
        text_view.scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
    }

    scrolled.set_child(Some(&text_view));

    let button_box = GtkBox::new(Orientation::Horizontal, 5);
    let refresh_btn = Button::with_label("↻");
    
    let file_path_owned = file_path.to_string();
    let text_view_clone = text_view.clone();
    refresh_btn.connect_clicked(move |_| {
        if let Ok(content) = fs::read_to_string(&file_path_owned) {
            text_view_clone.buffer().set_text(&content);
            // Scroll to end
            let buffer = text_view_clone.buffer();
            let mut end_iter = buffer.end_iter();
            text_view_clone.scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
        }
    });

    button_box.append(&refresh_btn);
    button_box.append(&Label::new(Some(file_path)));

    container.append(&scrolled);
    container.append(&button_box);

    container
}

fn create_split_view_tab(_shell_id: usize, notebook: Notebook, shell_counter: Option<Rc<RefCell<usize>>>) -> Paned {
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_margin_top(5);
    paned.set_margin_bottom(5);
    paned.set_margin_start(5);
    paned.set_margin_end(5);
    
    // Left side: Notes editor with save button
    let notes_container = GtkBox::new(Orientation::Vertical, 5);
    
    let notes_scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let notes_view = TextView::builder()
        .monospace(true)
        .build();

    // Load notes content
    let notes_path = get_file_path("notes.md");
    if let Ok(content) = fs::read_to_string(&notes_path) {
        notes_view.buffer().set_text(&content);
    }
    
    // Apply markdown highlighting
    apply_markdown_highlighting(&notes_view);

    // Auto-save notes with debounce
    let notes_path_clone = notes_path.clone();
    let notes_view_clone = notes_view.clone();
    let save_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let save_timeout_clone = Rc::clone(&save_timeout_id);
    
    notes_view.buffer().connect_changed(move |buffer| {
        let file_path = notes_path_clone.clone();
        let notes_view_ref = notes_view_clone.clone();
        
        // Cancel any pending save
        if let Some(id) = save_timeout_clone.borrow_mut().take() {
            id.remove();
        }
        
        // Apply highlighting immediately (fast operation)
        apply_markdown_highlighting(&notes_view_ref);
        
        // Debounce file save - wait 500ms after last change before saving
        let save_timeout_inner = Rc::clone(&save_timeout_clone);
        let buffer_clone = buffer.clone();
        let source_id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let start = buffer_clone.start_iter();
            let end = buffer_clone.end_iter();
            let text = buffer_clone.text(&start, &end, false);
            let _ = fs::write(&file_path, text.as_str());
            *save_timeout_inner.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *save_timeout_clone.borrow_mut() = Some(source_id);
    });

    notes_scrolled.set_child(Some(&notes_view));
    
    // Save button and label
    let button_box = GtkBox::new(Orientation::Horizontal, 5);
    let save_btn = Button::with_label("💾 Save");
    
    let notes_path_clone2 = notes_path.clone();
    let notes_view_clone2 = notes_view.clone();
    save_btn.connect_clicked(move |_| {
        let buffer = notes_view_clone2.buffer();
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, false);
        let _ = fs::write(&notes_path_clone2, text.as_str());
    });

    button_box.append(&save_btn);
    button_box.append(&Label::new(Some("notes.md")));

    // Add Ctrl+S keyboard shortcut
    let key_controller = gtk::EventControllerKey::new();
    let notes_path_clone3 = notes_path.clone();
    let notes_view_clone3 = notes_view.clone();
    let notes_view_clone4 = notes_view.clone();
    let notes_view_clone5 = notes_view.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            if keyval == gtk::gdk::Key::s {
                let buffer = notes_view_clone3.buffer();
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false);
                let _ = fs::write(&notes_path_clone3, text.as_str());
                
                return gtk::glib::Propagation::Stop;
            }
            
            // Add target insertion shortcut
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            if key_name == shortcuts.insert_target {
                show_target_selector_for_textview(&notes_view_clone4);
                return gtk::glib::Propagation::Stop;
            }
            
            // Check for Ctrl+Shift+T (timestamp insertion)
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) && key_name == shortcuts.insert_timestamp {
                let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S] ").to_string();
                let buffer = notes_view_clone5.buffer();
                buffer.insert_at_cursor(&timestamp);
                return gtk::glib::Propagation::Stop;
            }
        }
        gtk::glib::Propagation::Proceed
    });
    notes_view.add_controller(key_controller);
    
    notes_container.append(&notes_scrolled);
    notes_container.append(&button_box);
    
    // Right side: Shell tab
    let shell_container = create_shell_tab(_shell_id, notebook, shell_counter);
    
    paned.set_start_child(Some(&notes_container));
    paned.set_end_child(Some(&shell_container));
    paned.set_position(600); // Initial split position
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);

    paned
}

// Helper function to create a new shell tab (callable from anywhere including terminal keyboard handler)
fn create_new_shell_tab(notebook: &Notebook, shell_counter: &Rc<RefCell<usize>>) {
    let mut counter = shell_counter.borrow_mut();
    let shell_page = create_shell_tab(*counter, notebook.clone(), Some(Rc::clone(shell_counter)));
    let shell_label = create_editable_tab_label(&format!("💻 Shell {}", *counter), notebook);
    let page_num = notebook.append_page(&shell_page, Some(&shell_label));
    notebook.set_current_page(Some(page_num));
    focus_terminal_in_page(&shell_page.upcast_ref::<gtk::Widget>());
    *counter += 1;
}

// Helper function to create a new split view tab (callable from anywhere including terminal keyboard handler)
fn create_new_split_view_tab(notebook: &Notebook, shell_counter: &Rc<RefCell<usize>>) {
    let counter = shell_counter.borrow();
    let split_page = create_split_view_tab(*counter, notebook.clone(), Some(Rc::clone(shell_counter)));
    let split_label = create_editable_tab_label("📝💻 Split View", notebook);
    let page_num = notebook.append_page(&split_page, Some(&split_label));
    notebook.set_current_page(Some(page_num));
    focus_terminal_in_split_view(&split_page.upcast_ref::<gtk::Widget>());
}

fn create_shell_tab(_shell_id: usize, notebook: Notebook, shell_counter: Option<Rc<RefCell<usize>>>) -> GtkBox {
    let outer_container = GtkBox::new(Orientation::Vertical, 5);
    outer_container.set_margin_top(5);
    outer_container.set_margin_bottom(5);
    outer_container.set_margin_start(5);
    outer_container.set_margin_end(5);

    // Target selector
    let target_box = GtkBox::new(Orientation::Horizontal, 5);
    let target_combo = gtk::ComboBoxText::new();
    target_combo.set_hexpand(true);
    
    // Load targets using helper function
    let targets = load_targets();
    for target in &targets {
        target_combo.append_text(target);
    }
    if !targets.is_empty() {
        target_combo.set_active(Some(0));
    }

    let insert_target_btn = Button::with_label("🎯 Insert Target");
    
    // Add toggle button for command drawer
    let drawer_toggle = gtk::ToggleButton::with_label("📚 Commands");
    
    // Create horizontal paned layout for terminal and drawer
    let paned = Paned::new(Orientation::Horizontal);
    
    // Terminal container
    let terminal_container = GtkBox::new(Orientation::Vertical, 0);
    
    // Create VTE terminal widget
    let terminal = Terminal::new();
    terminal.set_vexpand(true);
    
    // Build base environment variables for shell
    let mut env_vars = vec![
        format!("HOME={}", std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())),
        format!("USER={}", std::env::var("USER").unwrap_or_else(|_| "user".to_string())),
        format!("PATH={}", std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string())),
        format!("TERM={}", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string())),
        format!("SHELL={}", std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())),
    ];
    
    // Add command logging via PROMPT_COMMAND if enabled
    if is_command_logging_enabled() {
        let log_file = get_file_path("commands.log").to_string_lossy().to_string();
        
        // Set up bash to log commands after execution using PROMPT_COMMAND
        // This captures only completed commands from history, not keystrokes or passwords
        // Initialize __penenv_prev_cmd only if it's not set (first run) to prevent logging old commands
        let prompt_cmd = format!(
            r#"history -a; __penenv_last_cmd=$(HISTTIMEFORMAT= history 1 | sed 's/^[ ]*[0-9]*[ ]*//'); if [ -z "$__penenv_prev_cmd" ]; then __penenv_prev_cmd="$__penenv_last_cmd"; fi; if [ -n "$__penenv_last_cmd" ] && [ "$__penenv_last_cmd" != "$__penenv_prev_cmd" ]; then echo "[$(date '+%Y-%m-%d %H:%M:%S')] $__penenv_last_cmd" >> '{}'; __penenv_prev_cmd="$__penenv_last_cmd"; fi"#,
            log_file
        );
        env_vars.insert(0, format!("PROMPT_COMMAND={}", prompt_cmd));
    }
    
    let env_refs: Vec<&str> = env_vars.iter().map(|s| s.as_str()).collect();
    
    let _ = terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        None, // working directory
        &["/bin/bash"],
        &env_refs, // environment with PROMPT_COMMAND for logging
        gtk::glib::SpawnFlags::DEFAULT,
        || {}, // child setup
        -1, // timeout
        None::<&gtk::gio::Cancellable>,
        |result| {
            if let Err(e) = result {
                eprintln!("Failed to spawn shell: {:?}", e);
            }
        },
    );
    
    terminal_container.append(&terminal);
    
    // Create command drawer
    let (drawer, search_entry) = create_command_drawer(terminal.clone(), drawer_toggle.clone(), paned.clone());
    drawer.set_visible(false);
    
    paned.set_start_child(Some(&terminal_container));
    paned.set_end_child(Some(&drawer));
    paned.set_position(800); // Initial position
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    // Toggle drawer visibility
    let drawer_clone = drawer.clone();
    let paned_clone = paned.clone();
    drawer_toggle.connect_toggled(move |btn| {
        drawer_clone.set_visible(btn.is_active());
        if btn.is_active() {
            paned_clone.set_position(600); // Show drawer
        } else {
            paned_clone.set_position(10000); // Hide drawer
        }
    });

    // Insert target button functionality
    let terminal_clone = terminal.clone();
    let target_combo_clone = target_combo.clone();
    insert_target_btn.connect_clicked(move |_| {
        if let Some(target) = target_combo_clone.active_text() {
            terminal_clone.feed_child(target.as_bytes());
            terminal_clone.grab_focus();
        }
    });

    // Command logging is now handled by bash PROMPT_COMMAND environment variable
    // This only logs completed commands from history, not keystrokes or passwords
    
    // Set up periodic log refresh to show new commands (only if logging is enabled)
    if is_command_logging_enabled() {
        let notebook_clone = notebook.clone();
        glib::timeout_add_seconds_local(2, move || {
            refresh_log_viewer(&notebook_clone);
            glib::ControlFlow::Continue
        });
    }

    target_box.append(&target_combo);
    target_box.append(&insert_target_btn);
    target_box.append(&drawer_toggle);
    
    let key_controller = gtk::EventControllerKey::new();
    let terminal_clone2 = terminal.clone();
    let notebook_clone2 = notebook.clone();
    let drawer_toggle_clone2 = drawer_toggle.clone();
    let search_entry_clone = search_entry.clone();
    let shell_counter_clone3 = shell_counter.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let shortcuts = get_keyboard_shortcuts();
            let key_name = keyval.name().unwrap_or_default().to_string();
            
            // Check for Ctrl+Shift combinations first (new shell/split shortcuts)
            if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                // Handle new shell shortcut
                if let Some(ref new_shell_key) = shortcuts.new_shell {
                    if &key_name == new_shell_key {
                        if let Some(ref counter) = shell_counter_clone3 {
                            create_new_shell_tab(&notebook_clone2, counter);
                        }
                        return gtk::glib::Propagation::Stop;
                    }
                }
                // Handle new split view shortcut
                if let Some(ref new_split_key) = shortcuts.new_split {
                    if &key_name == new_split_key {
                        if let Some(ref counter) = shell_counter_clone3 {
                            create_new_split_view_tab(&notebook_clone2, counter);
                        }
                        return gtk::glib::Propagation::Stop;
                    }
                }
            }
            
            // Toggle command drawer shortcut
            if key_name == shortcuts.toggle_drawer {
                drawer_toggle_clone2.set_active(!drawer_toggle_clone2.is_active());
                if drawer_toggle_clone2.is_active() {
                    search_entry_clone.grab_focus();
                }
                return gtk::glib::Propagation::Stop;
            }
            
            // Insert target shortcut
            if key_name == shortcuts.insert_target {
                show_target_selector_popup(&terminal_clone2);
                return gtk::glib::Propagation::Stop;
            }
            
            // Ctrl+1 through Ctrl+9 to switch tabs
            let page_num = match keyval {
                gtk::gdk::Key::_1 => Some(0),
                gtk::gdk::Key::_2 => Some(1),
                gtk::gdk::Key::_3 => Some(2),
                gtk::gdk::Key::_4 => Some(3),
                gtk::gdk::Key::_5 => Some(4),
                gtk::gdk::Key::_6 => Some(5),
                gtk::gdk::Key::_7 => Some(6),
                gtk::gdk::Key::_8 => Some(7),
                gtk::gdk::Key::_9 => Some(8),
                _ => None,
            };
            
            if let Some(page) = page_num {
                if page < notebook_clone2.n_pages() {
                    notebook_clone2.set_current_page(Some(page));
                    return gtk::glib::Propagation::Stop;
                }
            }
        }
        gtk::glib::Propagation::Proceed
    });
    terminal.add_controller(key_controller);

    // Add Shift+Ctrl+C/V for copy/paste
    let copy_paste_controller = gtk::EventControllerKey::new();
    let terminal_clone3 = terminal.clone();
    copy_paste_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        if modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) &&
           modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            match keyval {
                gtk::gdk::Key::C | gtk::gdk::Key::c => {
                    // Copy selected text to clipboard
                    terminal_clone3.copy_clipboard_format(vte4::Format::Text);
                    return gtk::glib::Propagation::Stop;
                }
                gtk::gdk::Key::V | gtk::gdk::Key::v => {
                    // Paste from clipboard
                    terminal_clone3.paste_clipboard();
                    return gtk::glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        gtk::glib::Propagation::Proceed
    });
    terminal.add_controller(copy_paste_controller);

    // Add right-click context menu
    let right_click_gesture = gtk::GestureClick::new();
    right_click_gesture.set_button(3); // Right mouse button
    
    let terminal_clone4 = terminal.clone();
    right_click_gesture.connect_pressed(move |_, _, x, y| {
        // Create popup menu
        let menu_model = gtk::gio::Menu::new();
        
        menu_model.append(Some("Copy"), Some("terminal.copy"));
        menu_model.append(Some("Paste"), Some("terminal.paste"));
        
        let menu = gtk::PopoverMenu::from_model(Some(&menu_model));
        menu.set_parent(&terminal_clone4);
        menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        
        // Create action group for menu actions
        let actions = gtk::gio::SimpleActionGroup::new();
        
        // Copy action
        let copy_action = gtk::gio::SimpleAction::new("copy", None);
        let terminal_copy = terminal_clone4.clone();
        copy_action.connect_activate(move |_, _| {
            terminal_copy.copy_clipboard_format(vte4::Format::Text);
        });
        actions.add_action(&copy_action);
        
        // Paste action
        let paste_action = gtk::gio::SimpleAction::new("paste", None);
        let terminal_paste = terminal_clone4.clone();
        paste_action.connect_activate(move |_, _| {
            terminal_paste.paste_clipboard();
        });
        actions.add_action(&paste_action);
        
        terminal_clone4.insert_action_group("terminal", Some(&actions));
        
        menu.popup();
    });
    terminal.add_controller(right_click_gesture);

    outer_container.append(&target_box);
    outer_container.append(&paned);

    outer_container
}

fn create_command_drawer(terminal: Terminal, drawer_toggle: gtk::ToggleButton, paned: Paned) -> (GtkBox, gtk::SearchEntry) {
    let drawer = GtkBox::new(Orientation::Vertical, 5);
    drawer.set_width_request(350);
    
    // Search box
    let search_box = GtkBox::new(Orientation::Horizontal, 5);
    search_box.set_margin_top(5);
    search_box.set_margin_bottom(5);
    search_box.set_margin_start(5);
    search_box.set_margin_end(5);
    
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search commands..."));
    search_entry.set_hexpand(true);
    
    search_box.append(&search_entry);
    
    // Scrolled window for commands list
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    
    // Load commands
    let commands = Rc::new(load_command_templates());
    let commands_clone = Rc::clone(&commands);
    
    // Populate list with all commands initially
    let mut category_widgets: std::collections::HashMap<String, gtk::ListBoxRow> = std::collections::HashMap::new();
    
    for (idx, cmd) in commands.iter().enumerate() {
        // Add category header if new
        if !category_widgets.contains_key(&cmd.category) {
            let category_row = gtk::ListBoxRow::new();
            category_row.set_selectable(false);
            category_row.set_activatable(false);
            
            let category_label = Label::new(Some(&cmd.category));
            category_label.set_halign(gtk::Align::Start);
            category_label.set_margin_start(5);
            category_label.set_margin_top(10);
            category_label.set_margin_bottom(5);
            category_label.add_css_class("heading");
            
            category_row.set_child(Some(&category_label));
            list_box.append(&category_row);
            category_widgets.insert(cmd.category.clone(), category_row);
        }
        
        // Add command row
        let row = gtk::ListBoxRow::new();
        row.set_activatable(true);
        
        let row_box = GtkBox::new(Orientation::Vertical, 2);
        row_box.set_margin_start(10);
        row_box.set_margin_end(10);
        row_box.set_margin_top(5);
        row_box.set_margin_bottom(5);
        
        let name_label = Label::new(Some(&cmd.name));
        name_label.set_halign(gtk::Align::Start);
        name_label.add_css_class("caption");
        
        let desc_label = Label::new(Some(&cmd.description));
        desc_label.set_halign(gtk::Align::Start);
        desc_label.set_wrap(true);
        desc_label.set_opacity(0.7);
        desc_label.set_xalign(0.0);
        desc_label.add_css_class("caption");
        
        row_box.append(&name_label);
        row_box.append(&desc_label);
        
        row.set_child(Some(&row_box));
        
        // Set tooltip with full command
        row.set_tooltip_text(Some(&format!("{}\n\nCommand: {}", cmd.description, cmd.command)));
        
        // Store command as string in row name (using idx as identifier)
        row.set_widget_name(&format!("cmd_{}", idx));
        
        list_box.append(&row);
    }
    
    scrolled.set_child(Some(&list_box));
    
    // Handle command selection
    let terminal_clone = terminal.clone();
    let commands_clone2 = Rc::clone(&commands_clone);
    let drawer_toggle_clone = drawer_toggle.clone();
    let paned_clone = paned.clone();
    list_box.connect_row_activated(move |_, row| {
        let name = row.widget_name();
        if let Some(idx_str) = name.strip_prefix("cmd_") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some(cmd) = commands_clone2.get(idx) {
                    // Check if command contains {target} placeholder
                    if cmd.command.contains("{target}") {
                        // Show target selector popup to fill in the target
                        show_target_selector_for_command(&terminal_clone, cmd.command.clone());
                    } else {
                        // Insert command directly into terminal
                        terminal_clone.feed_child(cmd.command.as_bytes());
                        terminal_clone.feed_child(b" ");
                        terminal_clone.grab_focus();
                    }
                    
                    // Close the drawer
                    drawer_toggle_clone.set_active(false);
                    paned_clone.set_position(10000);
                }
            }
        }
    });
    
    // Search functionality
    let list_box_clone = list_box.clone();
    let commands_clone3 = Rc::clone(&commands_clone);
    search_entry.connect_search_changed(move |entry| {
        let search_text = entry.text().to_lowercase();
        let is_searching = !search_text.is_empty();
        
        // Track which categories have visible commands
        let mut visible_categories: std::collections::HashSet<String> = std::collections::HashSet::new();
        
        // First pass: determine which commands match and collect their categories
        if is_searching {
            for cmd in commands_clone3.iter() {
                let matches = cmd.name.to_lowercase().contains(&search_text)
                    || cmd.description.to_lowercase().contains(&search_text)
                    || cmd.command.to_lowercase().contains(&search_text)
                    || cmd.category.to_lowercase().contains(&search_text);
                if matches {
                    visible_categories.insert(cmd.category.clone());
                }
            }
        }
        
        // Second pass: update visibility of all rows
        let mut child = list_box_clone.first_child();
        while let Some(row) = child {
            if let Some(list_row) = row.downcast_ref::<gtk::ListBoxRow>() {
                let name = list_row.widget_name();
                
                // Handle command rows
                if let Some(idx_str) = name.strip_prefix("cmd_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if let Some(cmd) = commands_clone3.get(idx) {
                            if is_searching {
                                let matches = cmd.name.to_lowercase().contains(&search_text)
                                    || cmd.description.to_lowercase().contains(&search_text)
                                    || cmd.command.to_lowercase().contains(&search_text)
                                    || cmd.category.to_lowercase().contains(&search_text);
                                list_row.set_visible(matches);
                            } else {
                                list_row.set_visible(true);
                            }
                        }
                    }
                } else {
                    // Handle category header rows
                    // Category rows don't have "cmd_" prefix and are not selectable
                    if !list_row.is_selectable() {
                        if is_searching {
                            // Check if this category has any visible commands
                            if let Some(child_widget) = list_row.child() {
                                if let Some(label) = child_widget.downcast_ref::<Label>() {
                                    let category_text = label.text();
                                    list_row.set_visible(visible_categories.contains(category_text.as_str()));
                                }
                            }
                        } else {
                            list_row.set_visible(true);
                        }
                    }
                }
            }
            child = row.next_sibling();
        }
    });
    
    // Add keyboard navigation to search entry
    let search_key_controller = gtk::EventControllerKey::new();
    let list_box_clone2 = list_box.clone();
    let drawer_toggle_clone = drawer_toggle.clone();
    search_key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
        match keyval {
            gtk::gdk::Key::Down => {
                // Move focus to list and select first visible item
                list_box_clone2.grab_focus();
                if let Some(first_row) = list_box_clone2.first_child() {
                    // Find first visible selectable row
                    let mut current = Some(first_row);
                    while let Some(row) = current {
                        if let Some(list_row) = row.downcast_ref::<gtk::ListBoxRow>() {
                            if list_row.is_visible() && list_row.is_selectable() {
                                list_box_clone2.select_row(Some(list_row));
                                break;
                            }
                        }
                        current = row.next_sibling();
                    }
                }
                return gtk::glib::Propagation::Stop;
            }
            _ if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) => {
                let shortcuts = get_keyboard_shortcuts();
                let key_name = keyval.name().unwrap_or_default().to_string();
                
                // Check if it's the toggle drawer shortcut
                if key_name == shortcuts.toggle_drawer {
                    drawer_toggle_clone.set_active(false);
                    return gtk::glib::Propagation::Stop;
                }
            }
            gtk::gdk::Key::Escape => {
                // Escape to close drawer
                drawer_toggle_clone.set_active(false);
                return gtk::glib::Propagation::Stop;
            }
            _ => {}
        }
        gtk::glib::Propagation::Proceed
    });
    search_entry.add_controller(search_key_controller);
    
    drawer.append(&search_box);
    drawer.append(&scrolled);
    
    (drawer, search_entry)
}

fn show_target_selector_popup(terminal: &Terminal) {
    let targets = load_targets();
    if targets.is_empty() {
        return;
    }
    
    // Create popup window
    let popup = gtk::Window::builder()
        .title("Select Target")
        .modal(true)
        .default_width(400)
        .default_height(300)
        .build();
    
    let popup_box = GtkBox::new(Orientation::Vertical, 5);
    popup_box.set_margin_top(10);
    popup_box.set_margin_bottom(10);
    popup_box.set_margin_start(10);
    popup_box.set_margin_end(10);
    
    // Create scrolled window with list
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .build();
    
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    
    // Add targets to list
    for target in targets.iter() {
        let row = gtk::ListBoxRow::new();
        let label = Label::new(Some(target));
        label.set_halign(gtk::Align::Start);
        label.set_margin_start(10);
        label.set_margin_end(10);
        label.set_margin_top(8);
        label.set_margin_bottom(8);
        row.set_child(Some(&label));
        list_box.append(&row);
    }
    
    // Select first item by default
    list_box.select_row(list_box.row_at_index(0).as_ref());
    
    scrolled.set_child(Some(&list_box));
    
    let button_box = GtkBox::new(Orientation::Horizontal, 5);
    button_box.set_halign(gtk::Align::End);
    
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let cancel_btn = Button::with_label("Cancel");
    
    // Handle Insert button
    let popup_clone = popup.clone();
    let terminal_clone = terminal.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                terminal_clone.feed_child(targets_clone[index].as_bytes());
                terminal_clone.grab_focus();
            }
        }
        popup_clone.close();
    });
    
    // Handle Cancel button
    let popup_clone2 = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone2.close();
    });
    
    // Handle Enter key
    let popup_clone3 = popup.clone();
    let terminal_clone2 = terminal.clone();
    let targets_clone2 = targets.clone();
    list_box.connect_row_activated(move |_list_box, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            terminal_clone2.feed_child(targets_clone2[index].as_bytes());
            terminal_clone2.grab_focus();
        }
        popup_clone3.close();
    });
    
    // Handle keyboard navigation in popup
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let terminal_clone3 = terminal.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        } else if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    terminal_clone3.feed_child(targets_clone3[index].as_bytes());
                    terminal_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    popup.add_controller(key_controller);
    
    button_box.append(&cancel_btn);
    button_box.append(&insert_btn);
    
    popup_box.append(&scrolled);
    popup_box.append(&button_box);
    
    popup.set_child(Some(&popup_box));
    popup.present();
}

fn show_target_selector_for_command(terminal: &Terminal, command_template: String) {
    let targets = load_targets();
    if targets.is_empty() {
        // No targets available, insert command with placeholder
        terminal.feed_child(command_template.as_bytes());
        terminal.feed_child(b" ");
        return;
    }
    
    // Create popup window
    let popup = gtk::Window::builder()
        .title("Select Target for Command")
        .modal(true)
        .default_width(400)
        .default_height(300)
        .build();
    
    let popup_box = GtkBox::new(Orientation::Vertical, 5);
    popup_box.set_margin_top(10);
    popup_box.set_margin_bottom(10);
    popup_box.set_margin_start(10);
    popup_box.set_margin_end(10);
    
    // Create scrolled window with list
    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .build();
    
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    
    // Add targets to list
    for target in targets.iter() {
        let row = gtk::ListBoxRow::new();
        let label = Label::new(Some(target));
        label.set_halign(gtk::Align::Start);
        label.set_margin_start(10);
        label.set_margin_end(10);
        label.set_margin_top(8);
        label.set_margin_bottom(8);
        row.set_child(Some(&label));
        list_box.append(&row);
    }
    
    // Select first item by default
    list_box.select_row(list_box.row_at_index(0).as_ref());
    
    scrolled.set_child(Some(&list_box));
    
    let button_box = GtkBox::new(Orientation::Horizontal, 5);
    button_box.set_halign(gtk::Align::End);
    
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let cancel_btn = Button::with_label("Cancel");
    
    // Handle Insert button
    let popup_clone = popup.clone();
    let terminal_clone = terminal.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    let command_clone = command_template.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                // Replace {target} and {port} placeholders
                let filled_command = command_clone
                    .replace("{target}", &targets_clone[index])
                    .replace("{port}", ""); // Leave port empty for user to fill
                terminal_clone.feed_child(filled_command.as_bytes());
                terminal_clone.feed_child(b" ");
                terminal_clone.grab_focus();
            }
        }
        popup_clone.close();
    });
    
    // Handle Cancel button
    let popup_clone2 = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone2.close();
    });
    
    // Handle Enter key
    let popup_clone3 = popup.clone();
    let terminal_clone2 = terminal.clone();
    let targets_clone2 = targets.clone();
    let command_clone2 = command_template.clone();
    list_box.connect_row_activated(move |_list_box, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            let filled_command = command_clone2
                .replace("{target}", &targets_clone2[index])
                .replace("{port}", "");
            terminal_clone2.feed_child(filled_command.as_bytes());
            terminal_clone2.feed_child(b" ");
            terminal_clone2.grab_focus();
        }
        popup_clone3.close();
    });
    
    // Handle keyboard navigation
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let terminal_clone3 = terminal.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    let command_clone3 = command_template.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        } else if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    let filled_command = command_clone3
                        .replace("{target}", &targets_clone3[index])
                        .replace("{port}", "");
                    terminal_clone3.feed_child(filled_command.as_bytes());
                    terminal_clone3.feed_child(b" ");
                    terminal_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    popup.add_controller(key_controller);
    
    button_box.append(&cancel_btn);
    button_box.append(&insert_btn);
    
    popup_box.append(&scrolled);
    popup_box.append(&button_box);
    
    popup.set_child(Some(&popup_box));
    popup.present();
}

/// Shows a target selector popup for TextView (notes)
fn show_target_selector_for_textview(text_view: &TextView) {
    // Load targets using helper function
    let targets = load_targets();
    
    if targets.is_empty() {
        return;
    }
    
    // Create popup window
    let popup = gtk::Window::builder()
        .title("Select Target")
        .modal(true)
        .default_width(400)
        .default_height(300)
        .build();
    
    let popup_box = GtkBox::new(Orientation::Vertical, 10);
    popup_box.set_margin_top(10);
    popup_box.set_margin_bottom(10);
    popup_box.set_margin_start(10);
    popup_box.set_margin_end(10);
    
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    
    let list_box = ListBox::new();
    list_box.add_css_class("boxed-list");
    
    for target in &targets {
        let row = gtk::ListBoxRow::new();
        let label = Label::new(Some(target));
        label.set_halign(gtk::Align::Start);
        label.set_margin_top(8);
        label.set_margin_bottom(8);
        label.set_margin_start(12);
        label.set_margin_end(12);
        row.set_child(Some(&label));
        list_box.append(&row);
    }
    
    // Select first item by default
    if let Some(first_row) = list_box.row_at_index(0) {
        list_box.select_row(Some(&first_row));
    }
    
    scrolled.set_child(Some(&list_box));
    
    let button_box = GtkBox::new(Orientation::Horizontal, 10);
    button_box.set_halign(gtk::Align::End);
    
    let cancel_btn = Button::with_label("Cancel");
    let popup_clone = popup.clone();
    cancel_btn.connect_clicked(move |_| {
        popup_clone.close();
    });
    
    let insert_btn = Button::with_label("Insert");
    insert_btn.add_css_class("suggested-action");
    let popup_clone2 = popup.clone();
    let text_view_clone = text_view.clone();
    let list_box_clone = list_box.clone();
    let targets_clone = targets.clone();
    insert_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_clone.selected_row() {
            let index = row.index() as usize;
            if index < targets_clone.len() {
                let buffer = text_view_clone.buffer();
                buffer.insert_at_cursor(&targets_clone[index]);
                text_view_clone.grab_focus();
            }
        }
        popup_clone2.close();
    });
    
    // Handle double-click
    let popup_clone3 = popup.clone();
    let text_view_clone2 = text_view.clone();
    let targets_clone2 = targets.clone();
    list_box.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if index < targets_clone2.len() {
            let buffer = text_view_clone2.buffer();
            buffer.insert_at_cursor(&targets_clone2[index]);
            text_view_clone2.grab_focus();
        }
        popup_clone3.close();
    });
    
    // Handle keyboard navigation in popup
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone4 = popup.clone();
    let text_view_clone3 = text_view.clone();
    let list_box_clone2 = list_box.clone();
    let targets_clone3 = targets.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        } else if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            if let Some(row) = list_box_clone2.selected_row() {
                let index = row.index() as usize;
                if index < targets_clone3.len() {
                    let buffer = text_view_clone3.buffer();
                    buffer.insert_at_cursor(&targets_clone3[index]);
                    text_view_clone3.grab_focus();
                }
            }
            popup_clone4.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    popup.add_controller(key_controller);
    
    button_box.append(&cancel_btn);
    button_box.append(&insert_btn);
    
    popup_box.append(&scrolled);
    popup_box.append(&button_box);
    
    popup.set_child(Some(&popup_box));
    popup.present();
}

/// Focus the terminal in a shell tab page
fn focus_terminal_in_page(page: &gtk::Widget) {
    // Shell page structure: GtkBox -> ... -> Paned -> GtkBox (terminal_container) -> Terminal
    if let Some(outer_box) = page.downcast_ref::<GtkBox>() {
        // Skip target_box, go to paned
        if let Some(mut child) = outer_box.first_child() {
            child = child.next_sibling().unwrap_or(child); // Skip target_box
            if let Some(paned) = child.downcast_ref::<Paned>() {
                if let Some(start_child) = paned.start_child() {
                    if let Some(terminal_container) = start_child.downcast_ref::<GtkBox>() {
                        if let Some(terminal_widget) = terminal_container.first_child() {
                            if let Some(terminal) = terminal_widget.downcast_ref::<Terminal>() {
                                terminal.grab_focus();
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Focus the terminal in a split view page
fn focus_terminal_in_split_view(page: &gtk::Widget) {
    // Split view structure: Paned -> end_child (shell container) -> ... -> Terminal
    if let Some(paned) = page.downcast_ref::<Paned>() {
        if let Some(end_child) = paned.end_child() {
            // The end_child is the shell container, use the same logic as focus_terminal_in_page
            focus_terminal_in_page(&end_child);
        }
    }
}

fn reload_targets_in_shells(notebook: &Notebook) {
    // Load targets using helper function
    let targets = load_targets();
    
    // Update notes tab (page 1)
    if let Some(notes_page) = notebook.nth_page(Some(1)) {
        if let Some(notes_box) = notes_page.downcast_ref::<GtkBox>() {
            // Get the target_box (first child if it exists)
            if let Some(target_box) = notes_box.first_child() {
                if let Some(target_box) = target_box.downcast_ref::<GtkBox>() {
                    // Get the ComboBoxText (first child of target_box)
                    if let Some(combo) = target_box.first_child() {
                        if let Some(combo) = combo.downcast_ref::<gtk::ComboBoxText>() {
                            // Remember current selection
                            let current = combo.active_text();
                            
                            // Clear and reload
                            combo.remove_all();
                            for target in &targets {
                                combo.append_text(target);
                            }
                            
                            // Restore selection if still exists
                            if let Some(current_text) = current {
                                for (idx, target) in targets.iter().enumerate() {
                                    if target == current_text.as_str() {
                                        combo.set_active(Some(idx as u32));
                                        break;
                                    }
                                }
                            }
                            
                            // If nothing selected, select first
                            if combo.active().is_none() && !targets.is_empty() {
                                combo.set_active(Some(0));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Update all shell tabs (starting from page 3)
    for i in 3..notebook.n_pages() {
        if let Some(page) = notebook.nth_page(Some(i)) {
            // Get the shell container (GtkBox)
            if let Some(shell_box) = page.downcast_ref::<GtkBox>() {
                // Get the target_box (first child)
                if let Some(target_box) = shell_box.first_child() {
                    if let Some(target_box) = target_box.downcast_ref::<GtkBox>() {
                        // Get the ComboBoxText (first child of target_box)
                        if let Some(combo) = target_box.first_child() {
                            if let Some(combo) = combo.downcast_ref::<gtk::ComboBoxText>() {
                                // Remember current selection
                                let current = combo.active_text();
                                
                                // Clear and reload
                                combo.remove_all();
                                for target in &targets {
                                    combo.append_text(target);
                                }
                                
                                // Restore selection if still exists
                                if let Some(current_text) = current {
                                    for (idx, target) in targets.iter().enumerate() {
                                        if target == current_text.as_str() {
                                            combo.set_active(Some(idx as u32));
                                            break;
                                        }
                                    }
                                }
                                
                                // If nothing selected, select first
                                if combo.active().is_none() && !targets.is_empty() {
                                    combo.set_active(Some(0));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn refresh_log_viewer(notebook: &Notebook) {
    // Find the log page (page 2) and refresh it
    if let Some(log_page) = notebook.nth_page(Some(2)) {
        if let Some(log_box) = log_page.downcast_ref::<GtkBox>() {
            // Get scrolled window (first child)
            if let Some(scrolled) = log_box.first_child() {
                if let Some(scrolled) = scrolled.downcast_ref::<ScrolledWindow>() {
                    // Get text view
                    if let Some(text_view) = scrolled.child() {
                        if let Some(text_view) = text_view.downcast_ref::<TextView>() {
                            // Reload content
                            if let Ok(content) = fs::read_to_string(get_file_path("commands.log")) {
                                text_view.buffer().set_text(&content);
                                // Scroll to end
                                let buffer = text_view.buffer();
                                let mut end_iter = buffer.end_iter();
                                text_view.scroll_to_iter(&mut end_iter, 0.0, false, 0.0, 0.0);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    
    if bytes >= MB {
        format!("{:.1} MB/s", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB/s", bytes as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes)
    }
}

fn apply_markdown_highlighting(text_view: &TextView) {
    let buffer = text_view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    let text = buffer.text(&start, &end, false);
    
    // Remove existing tags
    buffer.remove_all_tags(&start, &end);
    
    // Create text tags for different markdown elements
    let tag_table = buffer.tag_table();
    
    // Header tags (H1-H6)
    for level in 1..=6 {
        let tag_name = format!("h{}", level);
        if tag_table.lookup(&tag_name).is_none() {
            let _tag = buffer.create_tag(
                Some(&tag_name),
                &[
                    ("foreground", &"#4EC9B0"),  // Teal color for headers
                    ("weight", &700),             // Bold
                    ("scale", &(1.5 - (level as f64 * 0.1))),  // Larger for higher headers
                ],
            );
        }
    }
    
    // Bold tag
    if tag_table.lookup("bold").is_none() {
        buffer.create_tag(
            Some("bold"),
            &[("weight", &700)],
        );
    }
    
    // Italic tag
    if tag_table.lookup("italic").is_none() {
        buffer.create_tag(
            Some("italic"),
            &[("style", &gtk::pango::Style::Italic)],
        );
    }
    
    // Code tag (inline)
    if tag_table.lookup("code").is_none() {
        buffer.create_tag(
            Some("code"),
            &[
                ("foreground", &"#CE9178"),    // Orange for code
                ("family", &"monospace"),
                ("background", &"#2D2D2D"),
            ],
        );
    }
    
    // Code block tag
    if tag_table.lookup("code_block").is_none() {
        buffer.create_tag(
            Some("code_block"),
            &[
                ("foreground", &"#D4D4D4"),
                ("family", &"monospace"),
                ("background", &"#1E1E1E"),
                ("paragraph-background", &"#1E1E1E"),
            ],
        );
    }
    
    // Link tag
    if tag_table.lookup("link").is_none() {
        buffer.create_tag(
            Some("link"),
            &[
                ("foreground", &"#569CD6"),    // Blue for links
                ("underline", &gtk::pango::Underline::Single),
            ],
        );
    }
    
    // List item tag
    if tag_table.lookup("list").is_none() {
        buffer.create_tag(
            Some("list"),
            &[("foreground", &"#DCDCAA")],    // Yellow for list markers
        );
    }
    
    // Blockquote tag
    if tag_table.lookup("blockquote").is_none() {
        buffer.create_tag(
            Some("blockquote"),
            &[
                ("foreground", &"#6A9955"),    // Green for blockquotes
                ("style", &gtk::pango::Style::Italic),
            ],
        );
    }
    
    // Apply tags by parsing markdown
    let lines: Vec<&str> = text.split('\n').collect();
    let mut current_pos = 0i32;
    let mut in_code_block = false;
    
    for line in lines {
        let line_start = current_pos;
        let line_end = current_pos + line.len() as i32;
        
        // Code blocks (```)
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            let mut start_iter = buffer.iter_at_offset(line_start);
            let mut end_iter = buffer.iter_at_offset(line_end);
            buffer.apply_tag_by_name("code_block", &mut start_iter, &mut end_iter);
        } else if in_code_block {
            let mut start_iter = buffer.iter_at_offset(line_start);
            let mut end_iter = buffer.iter_at_offset(line_end);
            buffer.apply_tag_by_name("code_block", &mut start_iter, &mut end_iter);
        } else {
            // Headers
            if line.starts_with('#') {
                let level = line.chars().take_while(|&c| c == '#').count();
                if level <= 6 && line.len() > level && line.chars().nth(level) == Some(' ') {
                    let mut start_iter = buffer.iter_at_offset(line_start);
                    let mut end_iter = buffer.iter_at_offset(line_end);
                    buffer.apply_tag_by_name(&format!("h{}", level), &mut start_iter, &mut end_iter);
                }
            }
            // Blockquotes
            else if line.trim_start().starts_with('>') {
                let mut start_iter = buffer.iter_at_offset(line_start);
                let mut end_iter = buffer.iter_at_offset(line_end);
                buffer.apply_tag_by_name("blockquote", &mut start_iter, &mut end_iter);
            }
            // Lists
            else if line.trim_start().starts_with('-') || line.trim_start().starts_with('*') || line.trim_start().starts_with('+') {
                if let Some(marker_pos) = line.find(|c| c == '-' || c == '*' || c == '+') {
                    let mut start_iter = buffer.iter_at_offset(line_start + marker_pos as i32);
                    let mut end_iter = buffer.iter_at_offset(line_start + marker_pos as i32 + 1);
                    buffer.apply_tag_by_name("list", &mut start_iter, &mut end_iter);
                }
            }
            
            // Inline formatting
            let mut i = 0;
            let chars: Vec<char> = line.chars().collect();
            while i < chars.len() {
                // Bold (**text** or __text__)
                if i + 4 < chars.len() && ((chars[i] == '*' && chars[i+1] == '*') || (chars[i] == '_' && chars[i+1] == '_')) {
                    if let Some(end_pos) = line[i+2..].find(if chars[i] == '*' { "**" } else { "__" }) {
                        let mut start_iter = buffer.iter_at_offset(line_start + (i + 2) as i32);
                        let mut end_iter = buffer.iter_at_offset(line_start + (i + 2 + end_pos) as i32);
                        buffer.apply_tag_by_name("bold", &mut start_iter, &mut end_iter);
                        i += end_pos + 4;
                        continue;
                    }
                }
                // Italic (*text* or _text_)
                else if i + 2 < chars.len() && (chars[i] == '*' || chars[i] == '_') && chars[i+1] != chars[i] {
                    if let Some(end_pos) = line[i+1..].find(chars[i]) {
                        let mut start_iter = buffer.iter_at_offset(line_start + (i + 1) as i32);
                        let mut end_iter = buffer.iter_at_offset(line_start + (i + 1 + end_pos) as i32);
                        buffer.apply_tag_by_name("italic", &mut start_iter, &mut end_iter);
                        i += end_pos + 2;
                        continue;
                    }
                }
                // Inline code (`code`)
                else if chars[i] == '`' {
                    if let Some(end_pos) = line[i+1..].find('`') {
                        let mut start_iter = buffer.iter_at_offset(line_start + (i + 1) as i32);
                        let mut end_iter = buffer.iter_at_offset(line_start + (i + 1 + end_pos) as i32);
                        buffer.apply_tag_by_name("code", &mut start_iter, &mut end_iter);
                        i += end_pos + 2;
                        continue;
                    }
                }
                // Links [text](url)
                else if chars[i] == '[' {
                    if let Some(bracket_end) = line[i..].find("](") {
                        if let Some(paren_end) = line[i+bracket_end..].find(')') {
                            let mut start_iter = buffer.iter_at_offset(line_start + i as i32);
                            let mut end_iter = buffer.iter_at_offset(line_start + (i + bracket_end + paren_end + 1) as i32);
                            buffer.apply_tag_by_name("link", &mut start_iter, &mut end_iter);
                            i += bracket_end + paren_end + 1;
                            continue;
                        }
                    }
                }
                i += 1;
            }
        }
        
        current_pos = line_end + 1; // +1 for newline
    }
}
