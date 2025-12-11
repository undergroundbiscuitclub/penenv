//! Configuration and settings management for PenEnv
//!
//! This module handles loading, saving, and accessing application settings
//! including monitor visibility, keyboard shortcuts, zoom levels, and command logging.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::cell::RefCell;
use gtk4::glib;

/// Configuration for keyboard shortcuts
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KeyboardShortcuts {
    pub toggle_drawer: String,
    pub insert_target: String,
    pub insert_timestamp: String,
    pub new_shell: Option<String>,
    pub new_split: Option<String>,
}

impl Default for KeyboardShortcuts {
    fn default() -> Self {
        Self {
            toggle_drawer: "grave".to_string(),  // ` key
            insert_target: "t".to_string(),
            insert_timestamp: "T".to_string(),  // Shift+T
            new_shell: Some("N".to_string()),   // Shift+N
            new_split: Some("S".to_string()),   // Shift+S
        }
    }
}

/// Configuration for system monitor visibility
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MonitorVisibility {
    pub show_cpu: bool,
    pub show_ram: bool,
    pub show_network: bool,
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

/// Main application settings
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppSettings {
    pub monitor_visibility: MonitorVisibility,
    pub keyboard_shortcuts: KeyboardShortcuts,
    pub enable_command_logging: bool,
    pub text_zoom_scale: Option<f64>,
    pub terminal_zoom_scale: Option<f64>,
    pub terminal_scrollback_lines: i64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            monitor_visibility: MonitorVisibility::default(),
            keyboard_shortcuts: KeyboardShortcuts::default(),
            enable_command_logging: true,
            text_zoom_scale: Some(1.0),
            terminal_zoom_scale: Some(1.0),
            terminal_scrollback_lines: 10000,
        }
    }
}

// Thread-local storage for application state
thread_local! {
    static BASE_DIR: RefCell<PathBuf> = RefCell::new(PathBuf::from("."));
    static APP_SETTINGS: RefCell<AppSettings> = RefCell::new(AppSettings::default());
    pub static TEXT_ZOOM_SCALE: RefCell<f64> = RefCell::new(1.0);
    pub static TERMINAL_ZOOM_SCALE: RefCell<f64> = RefCell::new(1.0);
}

/// Tab indices for the main notebook
#[allow(dead_code)]
pub mod tabs {
    pub const TARGETS: u32 = 0;
    pub const NOTES: u32 = 1;
    pub const LOG: u32 = 2;
    pub const FIRST_SHELL: u32 = 3;
}

/// Zoom configuration
pub mod zoom {
    pub const MIN_SCALE: f64 = 0.5;
    pub const MAX_SCALE: f64 = 3.0;
    pub const DEFAULT_SCALE: f64 = 1.0;
    pub const ZOOM_STEP: f64 = 1.1;
}

/// Sets the base directory for storing project files
pub fn set_base_dir(path: PathBuf) {
    BASE_DIR.with(|dir| {
        *dir.borrow_mut() = path;
    });
}

/// Gets the current base directory
pub fn get_base_dir() -> PathBuf {
    BASE_DIR.with(|dir| dir.borrow().clone())
}

/// Constructs a full file path from the base directory and filename
pub fn get_file_path(filename: &str) -> PathBuf {
    let mut path = get_base_dir();
    path.push(filename);
    path
}

/// Gets the penenv config directory, creating it if it doesn't exist
pub fn get_config_dir() -> PathBuf {
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
pub fn get_custom_commands_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("custom_commands.yaml");
    path
}

/// Gets the settings config file path
pub fn get_settings_config_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("settings.yaml");
    path
}

/// Loads app settings from config file
pub fn load_app_settings() -> AppSettings {
    let path = get_settings_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_yaml::from_str::<AppSettings>(&content) {
                APP_SETTINGS.with(|s| {
                    *s.borrow_mut() = settings.clone();
                });
                // Load zoom scales into global state
                if let Some(text_scale) = settings.text_zoom_scale {
                    TEXT_ZOOM_SCALE.with(|s| *s.borrow_mut() = text_scale.clamp(zoom::MIN_SCALE, zoom::MAX_SCALE));
                }
                if let Some(terminal_scale) = settings.terminal_zoom_scale {
                    TERMINAL_ZOOM_SCALE.with(|s| *s.borrow_mut() = terminal_scale.clamp(zoom::MIN_SCALE, zoom::MAX_SCALE));
                }
                return settings;
            }
        }
    }
    AppSettings::default()
}

/// Saves app settings to config file
pub fn save_app_settings(settings: &AppSettings) -> Result<(), String> {
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
pub fn get_app_settings() -> AppSettings {
    APP_SETTINGS.with(|s| s.borrow().clone())
}

/// Gets the current keyboard shortcuts
pub fn get_keyboard_shortcuts() -> KeyboardShortcuts {
    APP_SETTINGS.with(|s| s.borrow().keyboard_shortcuts.clone())
}

/// Checks if command logging is enabled
pub fn is_command_logging_enabled() -> bool {
    APP_SETTINGS.with(|s| s.borrow().enable_command_logging)
}

/// Gets the current text zoom scale
pub fn get_text_zoom_scale() -> f64 {
    TEXT_ZOOM_SCALE.with(|s| *s.borrow())
}

/// Sets the text zoom scale (without updating widgets - use set_text_zoom_scale_with_update for that)
pub fn set_text_zoom_scale_raw(scale: f64) {
    let clamped = scale.clamp(zoom::MIN_SCALE, zoom::MAX_SCALE);
    TEXT_ZOOM_SCALE.with(|s| *s.borrow_mut() = clamped);
}

/// Gets the current terminal zoom scale
pub fn get_terminal_zoom_scale() -> f64 {
    TERMINAL_ZOOM_SCALE.with(|s| *s.borrow())
}

/// Sets the terminal zoom scale (without updating widgets)
pub fn set_terminal_zoom_scale_raw(scale: f64) {
    let clamped = scale.clamp(zoom::MIN_SCALE, zoom::MAX_SCALE);
    TERMINAL_ZOOM_SCALE.with(|s| *s.borrow_mut() = clamped);
}

/// Converts a key name to display format
pub fn key_to_display(key: &str) -> String {
    match key {
        "grave" => "`".to_string(),
        "t" => "T".to_string(),
        "Return" => "Enter".to_string(),
        "space" => "Space".to_string(),
        _ => key.to_uppercase(),
    }
}

/// Loads targets from targets.txt file
///
/// Returns a vector of non-empty, non-comment lines from the targets file.
/// Comments are lines starting with '#'. Returns empty vector if file doesn't exist.
pub fn load_targets() -> Vec<String> {
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
