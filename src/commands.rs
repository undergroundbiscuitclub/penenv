//! Command template management for PenEnv
//!
//! This module handles loading, saving, and managing command templates
//! including both built-in and custom user-defined commands.

use serde::{Deserialize, Serialize};
use std::fs;
use crate::config::{get_custom_commands_path};

/// A command template with name, command string, description, and category
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommandTemplate {
    pub name: String,
    pub command: String,
    pub description: String,
    pub category: String,
}

/// Container for a list of command templates (for YAML serialization)
#[derive(Debug, Deserialize, Serialize)]
pub struct CommandsConfig {
    pub commands: Vec<CommandTemplate>,
}

// Embed the commands.yaml file at compile time
const COMMANDS_YAML: &str = include_str!("../commands.yaml");

/// Loads command templates from the embedded YAML file and custom commands
///
/// Returns an empty vector if parsing fails, with error logged to stderr
pub fn load_command_templates() -> Vec<CommandTemplate> {
    let mut commands = Vec::new();
    
    // Load built-in commands
    match serde_yaml::from_str::<CommandsConfig>(COMMANDS_YAML) {
        Ok(config) => commands.extend(config.commands),
        Err(e) => {
            log::warn!("Failed to parse commands.yaml: {}. Command drawer will be empty.", e);
        }
    }
    
    // Load custom commands
    let custom_path = get_custom_commands_path();
    if custom_path.exists() {
        if let Ok(content) = fs::read_to_string(&custom_path) {
            match serde_yaml::from_str::<CommandsConfig>(&content) {
                Ok(config) => commands.extend(config.commands),
                Err(e) => {
                    log::warn!("Failed to parse custom_commands.yaml: {}", e);
                }
            }
        }
    }
    
    commands
}

/// Saves a new custom command to the custom_commands.yaml file
pub fn save_custom_command(command: CommandTemplate) -> Result<(), String> {
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
pub fn load_custom_commands() -> Vec<CommandTemplate> {
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
pub fn save_custom_commands_list(commands: Vec<CommandTemplate>) -> Result<(), String> {
    let custom_path = get_custom_commands_path();
    let config = CommandsConfig { commands };
    let yaml = serde_yaml::to_string(&config).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&custom_path, yaml).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

/// Deletes a custom command by index
pub fn delete_custom_command(index: usize) -> Result<(), String> {
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
pub fn update_custom_command(index: usize, command: CommandTemplate) -> Result<(), String> {
    let mut commands = load_custom_commands();
    if index < commands.len() {
        commands[index] = command;
        save_custom_commands_list(commands)?;
        Ok(())
    } else {
        Err("Invalid command index".to_string())
    }
}
