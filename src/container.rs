//! Container management for PenEnv
//!
//! Manages Kali/pentest containers using podman or docker.
//! Provides functionality to build, create, start, stop, and connect to containers.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::process::{Command, Output};
use std::path::PathBuf;
use std::fs;
use crate::config::{get_config_dir, get_base_dir, is_flatpak};

/// Container runtime choice - podman or docker
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
pub enum ContainerRuntime {
    Podman,
    #[default]
    Docker,
}

impl ContainerRuntime {
    /// Get the command name for this runtime
    pub fn command(&self) -> &str {
        match self {
            ContainerRuntime::Podman => "podman",
            ContainerRuntime::Docker => "docker",
        }
    }

    /// Check if this runtime typically needs elevated privileges for networking access
    /// Note: This is just the default; actual behavior depends on ConnectionMode in ContainerConfig
    pub fn needs_sudo_by_default(&self) -> bool {
        // Podman typically needs elevated privileges for full networking access
        // Docker uses a daemon with socket permissions, so doesn't need sudo if user is in docker group
        matches!(self, ContainerRuntime::Podman)
    }

    /// Check if this runtime supports pkexec for privilege escalation
    /// Docker doesn't use pkexec - it uses socket permissions via docker group
    pub fn supports_pkexec(&self) -> bool {
        matches!(self, ContainerRuntime::Podman)
    }

    /// Get the pkexec command for privilege escalation with PolicyKit dialog
    pub fn pkexec_command(&self) -> &str {
        "pkexec"
    }

    /// Get display name
    pub fn display_name(&self) -> &str {
        match self {
            ContainerRuntime::Podman => "Podman",
            ContainerRuntime::Docker => "Docker",
        }
    }
}

/// Connection mode for containers
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
pub enum ConnectionMode {
    /// Rootful mode: uses sudo/pkexec, containers get real IPs, full VPN/tun support
    Rootful,
    /// Rootless mode: no sudo required, uses port forwarding or exec for connection
    #[default]
    Rootless,
}

impl ConnectionMode {
    /// Display name for UI
    pub fn display_name(&self) -> &str {
        match self {
            ConnectionMode::Rootful => "Rootful (sudo, full networking)",
            ConnectionMode::Rootless => "Rootless (no sudo, port forwarding)",
        }
    }

    /// Short description
    pub fn description(&self) -> &str {
        match self {
            ConnectionMode::Rootful => "Full networking with VPN/tun support. Requires authentication.",
            ConnectionMode::Rootless => "No root required. Uses port forwarding for SSH. Limited VPN support.",
        }
    }
}

/// Container configuration settings
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContainerConfig {
    /// Container runtime (podman or docker)
    pub runtime: ContainerRuntime,
    /// Connection mode (rootful or rootless)
    #[serde(default)]
    pub connection_mode: ConnectionMode,
    /// Base image name for initial builds
    pub image_name: String,
    /// Master image name for committed containers
    pub master_image: String,
    /// Path to Dockerfile
    pub dockerfile_path: String,
    /// Path to SSH public key file
    pub ssh_pubkey_path: String,
    /// Directory to map to /data in container
    pub data_mapping: String,
    /// Whether to expose VNC port outside container
    pub vnc_expose: bool,
    /// VNC port number
    pub vnc_port: u16,
    /// VNC password
    pub vnc_password: String,
    /// VNC display resolution
    pub vnc_display: String,
    /// VNC color depth
    pub vnc_depth: u8,
    /// NoVNC web port
    pub novnc_port: u16,
    /// Memory limit for container (e.g., "8g")
    pub memory_limit: String,
    /// CPU limit for container
    pub cpu_limit: u8,
    /// Base SSH port for rootless mode port forwarding (containers use base_ssh_port + offset)
    #[serde(default = "default_ssh_port")]
    pub base_ssh_port: u16,
    /// Whether to prefer podman exec over SSH in rootless mode
    #[serde(default)]
    pub prefer_exec: bool,
    /// Whether to enable direct X11 socket access for GUI apps (less secure, use noVNC instead)
    #[serde(default)]
    pub enable_x11_direct: bool,
}

fn default_ssh_port() -> u16 {
    2222
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            runtime: ContainerRuntime::default(),
            connection_mode: ConnectionMode::default(),
            image_name: "kali-build".to_string(),
            master_image: "kali-master".to_string(),
            dockerfile_path: "./Dockerfile-podman".to_string(),
            ssh_pubkey_path: "rootkey.pub".to_string(),
            data_mapping: "./data".to_string(),
            vnc_expose: false,
            vnc_port: 5900,
            vnc_password: "changeme".to_string(),
            vnc_display: "1920x1080".to_string(),
            vnc_depth: 16,
            novnc_port: 1337,
            memory_limit: "8g".to_string(),
            cpu_limit: 10,
            base_ssh_port: 2222,
            prefer_exec: true,
            enable_x11_direct: true,
        }
    }
}

/// Validates a container name for security and compatibility
/// Container names must:
/// - Be between 1 and 128 characters
/// - Start with an alphanumeric character
/// - Contain only alphanumeric characters, hyphens (-), and underscores (_)
/// - Not start or end with a hyphen
pub fn validate_container_name(name: &str) -> Result<(), String> {
    let name = name.trim();

    if name.is_empty() {
        return Err("Container name cannot be empty".to_string());
    }

    if name.len() > 128 {
        return Err("Container name must be 128 characters or less".to_string());
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphanumeric() {
        return Err("Container name must start with a letter or number".to_string());
    }

    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(format!(
                "Container name can only contain letters, numbers, hyphens, and underscores (invalid character: '{}')",
                c
            ));
        }
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err("Container name cannot start or end with a hyphen".to_string());
    }

    Ok(())
}

impl ContainerConfig {
    /// Check if this configuration uses rootful mode (requires sudo)
    pub fn is_rootful(&self) -> bool {
        self.connection_mode == ConnectionMode::Rootful
    }

    /// Check if this configuration uses rootless mode
    pub fn is_rootless(&self) -> bool {
        self.connection_mode == ConnectionMode::Rootless
    }

    /// Get the resolved data directory path
    /// If relative, resolves relative to the base directory selected at app startup
    /// Returns the resolved path
    pub fn get_resolved_data_path(&self) -> PathBuf {
        let data_path = PathBuf::from(&self.data_mapping);

        if data_path.is_absolute() {
            data_path
        } else {
            get_base_dir().join(&data_path)
        }
    }
}

/// Container status information
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// Container name
    pub name: String,
    /// Current status (running, exited, etc.)
    pub status: String,
    /// IP address if running
    pub ip_address: Option<String>,
    /// Image name
    pub image: String,
    /// Container ID
    pub id: String,
}

impl ContainerInfo {
    /// Check if container is running
    pub fn is_running(&self) -> bool {
        self.status.to_lowercase().contains("running") ||
        self.status.to_lowercase().contains("up")
    }
}

/// Result type for container operations
pub type ContainerResult<T> = Result<T, ContainerError>;

/// Error type for container operations
#[derive(Debug, Clone)]
pub struct ContainerError {
    pub message: String,
    pub details: Option<String>,
}

impl ContainerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(message: impl Into<String>, details: impl ToString) -> Self {
        Self {
            message: message.into(),
            details: Some(details.to_string()),
        }
    }
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref details) = self.details {
            write!(f, "{}: {}", self.message, details)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ContainerError {}

/// Container manager handles all container operations
pub struct ContainerManager {
    pub config: ContainerConfig,
}

impl ContainerManager {
    /// Create a new container manager with the given configuration
    pub fn new(config: ContainerConfig) -> Self {
        Self { config }
    }

    /// Create a new container manager with default configuration
    pub fn with_defaults() -> Self {
        Self::new(ContainerConfig::default())
    }

    /// Update the configuration at runtime (reloads from disk)
    pub fn reload_config(&mut self) {
        self.config = load_container_config();
        log::info!("Container config reloaded");
    }

    /// Update the configuration with the provided config
    pub fn update_config(&mut self, config: ContainerConfig) {
        self.config = config;
        log::info!("Container config updated");
    }

    /// Get the base command with optional pkexec for privilege escalation
    /// Uses pkexec to show the native GNOME PolicyKit authentication dialog
    /// In rootless mode, no privilege escalation is used
    ///
    /// Docker behavior:
    /// - Docker uses a daemon with socket permissions, not per-command elevation
    /// - Rootful: runs via system daemon at /var/run/docker.sock (user must be in docker group)
    /// - Rootless: runs via user daemon at $XDG_RUNTIME_DIR/docker.sock
    ///
    /// Podman behavior:
    /// - Rootful: uses pkexec for privilege escalation
    /// - Rootless: runs directly without elevation
    fn base_command(&self) -> Command {
        let in_flatpak = is_flatpak();

        match self.config.runtime {
            ContainerRuntime::Docker => {
                // Docker always runs directly - it uses socket permissions
                // For rootless Docker, the CLI auto-detects the user socket
                let mut cmd = if in_flatpak {
                    let mut c = Command::new("flatpak-spawn");
                    c.arg("--host");
                    c.arg(self.config.runtime.command());
                    c
                } else {
                    Command::new(self.config.runtime.command())
                };
                if self.config.is_rootless() {
                    // Ensure Docker uses the rootless socket if available
                    if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
                        let rootless_socket = format!("{}/docker.sock", xdg_runtime);
                        if std::path::Path::new(&rootless_socket).exists() {
                            if in_flatpak {
                                cmd.arg("--env");
                                cmd.arg(format!("DOCKER_HOST=unix://{}", rootless_socket));
                            } else {
                                cmd.env("DOCKER_HOST", format!("unix://{}", rootless_socket));
                            }
                        }
                    }
                }
                cmd
            }
            ContainerRuntime::Podman => {
                if self.config.is_rootful() {
                    if in_flatpak {
                        let mut cmd = Command::new("flatpak-spawn");
                        cmd.args(["--host", "pkexec", self.config.runtime.command()]);
                        cmd
                    } else {
                        let mut cmd = Command::new("pkexec");
                        cmd.arg(self.config.runtime.command());
                        cmd
                    }
                } else {
                    // Rootless mode - run directly without sudo/pkexec
                    if in_flatpak {
                        let mut cmd = Command::new("flatpak-spawn");
                        cmd.args(["--host", self.config.runtime.command()]);
                        cmd
                    } else {
                        Command::new(self.config.runtime.command())
                    }
                }
            }
        }
    }

    /// Get a command without privilege escalation (for rootless operations)
    fn rootless_command(&self) -> Command {
        if is_flatpak() {
            let mut cmd = Command::new("flatpak-spawn");
            cmd.args(["--host", self.config.runtime.command()]);
            cmd
        } else {
            Command::new(self.config.runtime.command())
        }
    }

    /// Execute a command and return the output
    fn execute(&self, args: &[&str]) -> ContainerResult<Output> {
        let output = self.base_command()
            .args(args)
            .output()
            .map_err(|e| ContainerError::with_details(
                "Failed to execute command",
                e.to_string()
            ))?;
        Ok(output)
    }

    /// Check if the container runtime is available (without triggering auth dialogs)
    /// This just checks if the runtime binary exists
    pub fn is_runtime_available(&self) -> bool {
        if is_flatpak() {
            Command::new("flatpak-spawn")
                .args(["--host", self.config.runtime.command(), "--version"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            Command::new(self.config.runtime.command())
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    }

    /// Check if rootless mode is available and working
    pub fn is_rootless_available(&self) -> bool {
        let in_flatpak = is_flatpak();

        match self.config.runtime {
            ContainerRuntime::Podman => {
                // Podman has a specific field to check for rootless mode
                let output = if in_flatpak {
                    Command::new("flatpak-spawn")
                        .args(["--host", self.config.runtime.command(), "info", "--format", "{{.Host.Security.Rootless}}"])
                        .output()
                } else {
                    Command::new(self.config.runtime.command())
                        .args(["info", "--format", "{{.Host.Security.Rootless}}"])
                        .output()
                };
                output.map(|o| {
                    o.status.success() &&
                    String::from_utf8_lossy(&o.stdout).trim() == "true"
                })
                .unwrap_or(false)
            }
            ContainerRuntime::Docker => {
                // Docker rootless mode uses a socket in XDG_RUNTIME_DIR
                // Check if the rootless docker socket exists and is accessible
                if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
                    let rootless_socket = format!("{}/docker.sock", xdg_runtime);
                    if std::path::Path::new(&rootless_socket).exists() {
                        // Try to connect to the rootless socket
                        let output = if in_flatpak {
                            Command::new("flatpak-spawn")
                                .args(["--host", self.config.runtime.command()])
                                .arg(format!("--env=DOCKER_HOST=unix://{}", rootless_socket))
                                .args(["info", "--format", "{{.ID}}"])
                                .output()
                        } else {
                            let mut cmd = Command::new(self.config.runtime.command());
                            cmd.env("DOCKER_HOST", format!("unix://{}", rootless_socket));
                            cmd.args(["info", "--format", "{{.ID}}"]);
                            cmd.output()
                        };
                        return output
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                    }
                }
                // Alternative: check if docker context shows rootless
                let output = if in_flatpak {
                    Command::new("flatpak-spawn")
                        .args(["--host", self.config.runtime.command(), "context", "ls", "--format", "{{.Name}}"])
                        .output()
                } else {
                    Command::new(self.config.runtime.command())
                        .args(["context", "ls", "--format", "{{.Name}}"])
                        .output()
                };
                output.map(|o| {
                    o.status.success() &&
                    String::from_utf8_lossy(&o.stdout).contains("rootless")
                })
                .unwrap_or(false)
            }
        }
    }

    /// Check if rootful mode is available (requires testing with pkexec for Podman)
    /// Note: For Podman, this may trigger an authentication dialog
    pub fn is_rootful_available(&self) -> bool {
        let in_flatpak = is_flatpak();

        match self.config.runtime {
            ContainerRuntime::Docker => {
                // Docker rootful mode uses the system daemon at /var/run/docker.sock
                // User must be in docker group or have sudo access
                // Check if we can connect to the system docker socket
                let system_socket = "/var/run/docker.sock";
                if std::path::Path::new(system_socket).exists() {
                    let output = if in_flatpak {
                        Command::new("flatpak-spawn")
                            .args(["--host", self.config.runtime.command()])
                            .arg(format!("--env=DOCKER_HOST=unix://{}", system_socket))
                            .args(["info", "--format", "{{.ID}}"])
                            .output()
                    } else {
                        let mut cmd = Command::new(self.config.runtime.command());
                        cmd.env("DOCKER_HOST", format!("unix://{}", system_socket));
                        cmd.args(["info", "--format", "{{.ID}}"]);
                        cmd.output()
                    };
                    return output
                        .map(|o| o.status.success())
                        .unwrap_or(false);
                }
                false
            }
            ContainerRuntime::Podman => {
                // For Podman, check if we can run with pkexec
                let output = if in_flatpak {
                    Command::new("flatpak-spawn")
                        .args(["--host", "pkexec", self.config.runtime.command(), "--version"])
                        .output()
                } else {
                    Command::new("pkexec")
                        .args([self.config.runtime.command(), "--version"])
                        .output()
                };
                output.map(|o| o.status.success())
                    .unwrap_or(false)
            }
        }
    }

    /// List all containers (running and stopped)
    pub fn list_containers(&self) -> ContainerResult<Vec<ContainerInfo>> {
        let output = self.execute(&[
            "ps", "-a",
            "--format", "{{.Names}}|{{.State}}|{{.Image}}|{{.ID}}"
        ])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ContainerError::with_details("Failed to list containers", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<ContainerInfo> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                ContainerInfo {
                    name: parts.first().unwrap_or(&"").to_string(),
                    status: parts.get(1).unwrap_or(&"unknown").to_string(),
                    image: parts.get(2).unwrap_or(&"").to_string(),
                    id: parts.get(3).unwrap_or(&"").to_string(),
                    ip_address: None, // Fetched separately when needed
                }
            })
            .collect();

        Ok(containers)
    }

    /// List only containers using our kali images
    pub fn list_kali_containers(&self) -> ContainerResult<Vec<ContainerInfo>> {
        let all = self.list_containers()?;
        Ok(all.into_iter()
            .filter(|c| {
                c.image.contains(&self.config.image_name) ||
                c.image.contains(&self.config.master_image)
            })
            .collect())
    }

    /// Get container IP address
    pub fn get_container_ip(&self, name: &str) -> ContainerResult<Option<String>> {
        // First try the top-level IPAddress (works for Podman and some Docker configs)
        let output = self.execute(&[
            "inspect", name,
            "--format", "{{.NetworkSettings.IPAddress}}"
        ])?;

        if output.status.success() {
            let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ip.is_empty() {
                return Ok(Some(ip));
            }
        }

        // Try Docker's bridge network format: .NetworkSettings.Networks.bridge.IPAddress
        let output = self.execute(&[
            "inspect", name,
            "--format", "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}"
        ])?;

        if output.status.success() {
            let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ip.is_empty() {
                // If multiple networks, take the first non-empty IP
                let first_ip = ip.split_whitespace().next().unwrap_or("").to_string();
                if !first_ip.is_empty() {
                    return Ok(Some(first_ip));
                }
            }
        }

        Ok(None)
    }

    /// Get detailed container info including IP
    pub fn get_container_info(&self, name: &str) -> ContainerResult<Option<ContainerInfo>> {
        let containers = self.list_containers()?;

        if let Some(mut info) = containers.into_iter().find(|c| c.name == name) {
            if info.is_running() {
                info.ip_address = self.get_container_ip(name)?;
            }
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    /// Check if an image exists
    pub fn image_exists(&self, image_name: &str) -> ContainerResult<bool> {
        let output = self.execute(&["image", "exists", image_name])?;
        Ok(output.status.success())
    }

    /// List available images
    pub fn list_images(&self) -> ContainerResult<Vec<String>> {
        let output = self.execute(&[
            "images", "--format", "{{.Repository}}:{{.Tag}}"
        ])?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines()
            .filter(|l| !l.trim().is_empty())
            .map(String::from)
            .collect())
    }

    /// Build container image from Dockerfile
    pub fn build_image(&self, working_dir: Option<&str>) -> ContainerResult<()> {
        let dir = working_dir.unwrap_or(".");

        let mut cmd = self.base_command();
        cmd.args([
            "build",
            "-t", &self.config.image_name,
            "-f", &self.config.dockerfile_path,
            dir
        ]);
        cmd.current_dir(dir);

        let status = cmd.status()
            .map_err(|e| ContainerError::with_details("Failed to build image", e))?;

        if status.success() {
            Ok(())
        } else {
            Err(ContainerError::new("Image build failed"))
        }
    }

    /// Ensure SSH key exists, generate if not
    /// Returns the public key content
    pub fn ensure_ssh_key(&self) -> ContainerResult<String> {
        let pubkey_path = self.get_ssh_pubkey_path();

        if pubkey_path.exists() {
            fs::read_to_string(&pubkey_path)
                .map_err(|e| ContainerError::with_details("Failed to read SSH key", e))
        } else {
            // Ensure config directory exists
            if let Some(parent) = pubkey_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| ContainerError::with_details("Failed to create SSH key directory", e))?;
            }

            // Generate new SSH key
            let privkey_path = self.get_ssh_privkey_path();
            let privkey_str = privkey_path.to_str().unwrap_or("");

            let status = if is_flatpak() {
                Command::new("flatpak-spawn")
                    .args(["--host", "ssh-keygen", "-t", "ed25519", "-f", privkey_str, "-N", ""])
                    .status()
            } else {
                Command::new("ssh-keygen")
                    .args(["-t", "ed25519", "-f", privkey_str, "-N", ""])
                    .status()
            }
            .map_err(|e| ContainerError::with_details("Failed to generate SSH key", e))?;

            if !status.success() {
                return Err(ContainerError::new("SSH key generation failed"));
            }

            fs::read_to_string(&pubkey_path)
                .map_err(|e| ContainerError::with_details("Failed to read generated SSH key", e))
        }
    }

    /// Get the absolute path to the SSH public key
    pub fn get_ssh_pubkey_path(&self) -> PathBuf {
        let configured_path = PathBuf::from(&self.config.ssh_pubkey_path);

        // If it's already absolute, use it as-is
        if configured_path.is_absolute() {
            configured_path
        } else {
            // Otherwise, resolve relative to config directory
            get_config_dir().join(&self.config.ssh_pubkey_path)
        }
    }

    /// Get the absolute path to the SSH private key
    pub fn get_ssh_privkey_path(&self) -> PathBuf {
        let pubkey_path = self.get_ssh_pubkey_path();
        let privkey_name = pubkey_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rootkey.pub")
            .replace(".pub", "");

        pubkey_path.parent()
            .map(|p| p.join(&privkey_name))
            .unwrap_or_else(|| PathBuf::from(&privkey_name))
    }

    /// Ensure data mapping directory exists and return the absolute path
    /// If the configured path is relative, it's resolved relative to the base directory
    /// selected when the app was opened (via the directory chooser dialog)
    /// If the directory doesn't exist, it will be created
    pub fn ensure_data_dir(&self) -> ContainerResult<PathBuf> {
        // Use the config helper to get the resolved path
        let absolute_path = self.config.get_resolved_data_path();

        // Create the directory if it doesn't exist
        if !absolute_path.exists() {
            fs::create_dir_all(&absolute_path)
                .map_err(|e| ContainerError::with_details(
                    &format!("Failed to create data directory: {}", absolute_path.display()),
                    e
                ))?;
        }

        // Now canonicalize to get the real absolute path
        fs::canonicalize(&absolute_path)
            .map_err(|e| ContainerError::with_details(
                &format!("Failed to resolve data directory path: {}", absolute_path.display()),
                e
            ))
    }

    /// Create and run a new container
    /// In rootless mode, uses port forwarding for SSH and mounts X11 socket
    /// In rootful mode, container gets a real IP address with full networking
    pub fn create_container(
        &self,
        name: &str,
        use_master: bool,
        detached: bool,
        temporary: bool,
    ) -> ContainerResult<()> {
        self.create_container_with_port(name, use_master, detached, temporary, None)
    }

    /// Create container with a specific SSH port (for rootless mode)
    pub fn create_container_with_port(
        &self,
        name: &str,
        use_master: bool,
        detached: bool,
        temporary: bool,
        ssh_port: Option<u16>,
    ) -> ContainerResult<()> {
        // Ensure prerequisites and get the absolute data path
        let data_path = self.ensure_data_dir()?;
        let ssh_key = self.ensure_ssh_key()?;

        // For rootless mode with X11 direct access enabled, set up xhost access
        if self.config.is_rootless() && self.config.enable_x11_direct {
            if let Ok(true) = Self::enable_x11_access() {
                // xhost +SI:localuser:$USER was successfully run
                // This allows only the current user's containers to connect to the X server
            }
        }

        let image = if use_master {
            &self.config.master_image
        } else {
            &self.config.image_name
        };

        let mut args: Vec<String> = vec!["run".to_string()];

        // Run mode
        if temporary {
            args.push("--rm".to_string());
        }
        if detached {
            args.push("-d".to_string());
        }
        args.push("-ti".to_string());

        // Environment variables
        args.extend(vec![
            "-e".to_string(), format!("SSHKEY={}", ssh_key.trim()),
            "-e".to_string(), format!("VNCEXPOSE={}", if self.config.vnc_expose { 1 } else { 0 }),
            "-e".to_string(), format!("VNCPORT={}", self.config.vnc_port),
            "-e".to_string(), format!("VNCPWD={}", self.config.vnc_password),
            "-e".to_string(), format!("VNCDISPLAY={}", self.config.vnc_display),
            "-e".to_string(), format!("VNCDEPTH={}", self.config.vnc_depth),
            "-e".to_string(), format!("NOVNCPORT={}", self.config.novnc_port),
        ]);

        // Volume mapping with SELinux label
        // data_path is already an absolute path from ensure_data_dir()
        args.extend(vec![
            "-v".to_string(),
            format!("{}:/data:rw,z", data_path.display()),
        ]);

        // Mode-specific configuration
        if self.config.is_rootless() {
            // Rootless mode: port forwarding for SSH, X11 socket mount
            let port = ssh_port.unwrap_or(self.config.base_ssh_port);
            args.extend(vec![
                "-p".to_string(), format!("{}:22", port),
            ]);

            // Mount X11 socket for GUI applications (only if explicitly enabled)
            // This is disabled by default for security - use noVNC instead
            if self.config.enable_x11_direct {
                if let Ok(display) = std::env::var("DISPLAY") {
                    args.extend(vec![
                        "-e".to_string(), format!("DISPLAY={}", display),
                    ]);
                    // Mount X11 socket with proper SELinux context
                    args.extend(vec![
                        "-v".to_string(), "/tmp/.X11-unix:/tmp/.X11-unix:rw".to_string(),
                    ]);

                    // Mount Xauthority for X11 authentication
                    // Try XDG_RUNTIME_DIR first (Wayland/modern), then HOME/.Xauthority
                    if let Ok(xauth) = std::env::var("XAUTHORITY") {
                        args.extend(vec![
                            "-e".to_string(), "XAUTHORITY=/tmp/.Xauthority".to_string(),
                            "-v".to_string(), format!("{}:/tmp/.Xauthority:ro,Z", xauth),
                        ]);
                    } else if let Ok(home) = std::env::var("HOME") {
                        let xauth_path = format!("{}/.Xauthority", home);
                        if std::path::Path::new(&xauth_path).exists() {
                            args.extend(vec![
                                "-e".to_string(), "XAUTHORITY=/tmp/.Xauthority".to_string(),
                                "-v".to_string(), format!("{}:/tmp/.Xauthority:ro,Z", xauth_path),
                            ]);
                        }
                    }

                    // For Wayland with XWayland, we may also need to handle this
                    // Pass through the user ID to help with socket permissions
                    if let Ok(uid) = std::env::var("UID") {
                        args.extend(vec![
                            "-e".to_string(), format!("HOST_UID={}", uid),
                        ]);
                    } else {
                        // Try to get UID from command
                        if let Ok(output) = Command::new("id").arg("-u").output() {
                            if output.status.success() {
                                let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                args.extend(vec![
                                    "-e".to_string(), format!("HOST_UID={}", uid),
                                ]);
                            }
                        }
                    }
                }
            }

            // Rootless still benefits from these capabilities for in-container tools
            args.extend(vec![
                "--cap-add".to_string(), "NET_ADMIN".to_string(),
                "--cap-add".to_string(), "NET_RAW".to_string(),
            ]);

            // Use proper SELinux context instead of disabling entirely
            // Only disable SELinux labeling if X11 direct access is enabled (required for X11 socket access)
            if self.config.enable_x11_direct {
                args.extend(vec![
                    "--security-opt".to_string(), "label=disable".to_string()
                ]);
            } else {
                // Use container_runtime_t context for better security while allowing container operations
                args.extend(vec![
                    "--security-opt".to_string(), "label=type:container_runtime_t".to_string()
                ]);
            }

            // Note: --device=/dev/net/tun typically doesn't work rootless
            // VPN functionality requires rootful mode
        } else {
            // Rootful mode: full capabilities and TUN device
            args.extend(vec![
                "--cap-add".to_string(), "NET_ADMIN".to_string(),
                "--cap-add".to_string(), "AUDIT_WRITE".to_string(),
                "--cap-add".to_string(), "NET_RAW".to_string(),
            ]);

            // TUN device for VPN support (requires root)
            args.extend(vec![
                "--device=/dev/net/tun".to_string(),
            ]);
        }

        // Container settings
        args.extend(vec![
            "--name".to_string(), name.to_string(),
            format!("--memory={}", self.config.memory_limit),
            format!("--cpus={}", self.config.cpu_limit),
            "-h".to_string(), name.to_string(),
            image.to_string(),
        ]);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let status = self.base_command()
            .args(&args_refs)
            .status()
            .map_err(|e| ContainerError::with_details("Failed to create container", e))?;

        if status.success() {
            Ok(())
        } else {
            Err(ContainerError::new("Container creation failed"))
        }
    }

    /// Get the SSH port for a container in rootless mode
    /// Returns None if in rootful mode (use IP instead)
    pub fn get_container_ssh_port(&self, name: &str) -> ContainerResult<Option<u16>> {
        if self.config.is_rootful() {
            return Ok(None);
        }

        // Query the mapped port for container's port 22
        let output = self.execute(&[
            "port", name, "22"
        ])?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output format is like "0.0.0.0:2222" or "[::]:2222"
        if let Some(port_str) = stdout.trim().split(':').last() {
            if let Ok(port) = port_str.parse::<u16>() {
                return Ok(Some(port));
            }
        }

        Ok(None)
    }

    /// Get the next available SSH port for rootless containers
    pub fn get_next_ssh_port(&self) -> ContainerResult<u16> {
        let containers = self.list_containers()?;
        let mut used_ports: Vec<u16> = Vec::new();

        for container in containers {
            if let Ok(Some(port)) = self.get_container_ssh_port(&container.name) {
                used_ports.push(port);
            }
        }

        let mut port = self.config.base_ssh_port;
        while used_ports.contains(&port) {
            port = port.checked_add(1)
                .ok_or_else(|| ContainerError::new("No available SSH ports"))?;
        }

        Ok(port)
    }

    /// Start a stopped container
    pub fn start_container(&self, name: &str) -> ContainerResult<()> {
        let output = self.execute(&["start", name])?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("Failed to start container", stderr))
        }
    }

    /// Stop a running container
    pub fn stop_container(&self, name: &str) -> ContainerResult<()> {
        let output = self.execute(&["stop", name])?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("Failed to stop container", stderr))
        }
    }

    /// Remove a container (must be stopped first)
    pub fn remove_container(&self, name: &str) -> ContainerResult<()> {
        let output = self.execute(&["rm", name])?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("Failed to remove container", stderr))
        }
    }

    /// Force remove a container (stop and remove)
    pub fn force_remove_container(&self, name: &str) -> ContainerResult<()> {
        let output = self.execute(&["rm", "-f", name])?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("Failed to force remove container", stderr))
        }
    }

    /// Commit container to master image
    pub fn commit_to_master(&self, container_name: &str) -> ContainerResult<()> {
        let output = self.execute(&[
            "commit", "-s", container_name, &self.config.master_image
        ])?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("Failed to commit container", stderr))
        }
    }

    /// Get SSH connection command for a container
    /// In rootful mode: connects via container IP
    /// In rootless mode: connects via localhost with port forwarding
    /// Note: Returns a plain host command - Flatpak wrapping is handled by terminal spawning
    pub fn get_ssh_command(&self, name: &str) -> ContainerResult<String> {
        let key_path = self.get_ssh_privkey_path();
        let key_path_str = key_path.to_string_lossy();

        if self.config.is_rootless() {
            // Rootless: use port forwarding to localhost
            let port = self.get_container_ssh_port(name)?
                .ok_or_else(|| ContainerError::new("Container has no SSH port mapping"))?;
            Ok(format!(
                "ssh -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i {} -p {} root@localhost -X -L 8181:127.0.0.1:8181",
                key_path_str, port
            ))
        } else {
            // Rootful: use container IP directly
            let ip = self.get_container_ip(name)?
                .ok_or_else(|| ContainerError::new("Container has no IP address"))?;
            Ok(format!(
                "ssh -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -i {} root@{} -X -L 8181:127.0.0.1:8181",
                key_path_str, ip
            ))
        }
    }

    /// Get SSH connection arguments for spawning in terminal
    /// In rootful mode: connects via container IP
    /// In rootless mode: connects via localhost with port forwarding
    /// Note: Returns plain host command args - Flatpak wrapping is handled by terminal spawning
    pub fn get_ssh_args(&self, name: &str) -> ContainerResult<Vec<String>> {
        let key_path = self.get_ssh_privkey_path();
        let key_path_str = key_path.to_string_lossy().to_string();

        let mut args = Vec::new();
        args.push("ssh".to_string());

        if self.config.is_rootless() {
            // Rootless: use port forwarding to localhost
            let port = self.get_container_ssh_port(name)?
                .ok_or_else(|| ContainerError::new("Container has no SSH port mapping"))?;
            args.extend([
                "-o".to_string(), "IdentitiesOnly=yes".to_string(),
                "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
                "-o".to_string(), "UserKnownHostsFile=/dev/null".to_string(),
                "-i".to_string(), key_path_str,
                "-p".to_string(), port.to_string(),
                "root@localhost".to_string(),
                "-X".to_string(),
                "-L".to_string(), "8181:127.0.0.1:8181".to_string(),
            ]);
        } else {
            // Rootful: use container IP with X11 forwarding
            let ip = self.get_container_ip(name)?
                .ok_or_else(|| ContainerError::new("Container has no IP address"))?;
            args.extend([
                "-o".to_string(), "IdentitiesOnly=yes".to_string(),
                "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
                "-i".to_string(), key_path_str.clone(),
                format!("root@{}", ip),
                "-X".to_string(),
                "-L".to_string(), "8181:127.0.0.1:8181".to_string(),
            ]);
        }

        Ok(args)
    }

    /// Get SSH tunnel arguments for VNC connection
    /// Creates an SSH tunnel that forwards a local port to the container's VNC port
    /// Returns (args, local_port) where local_port is the port to connect VNC to
    pub fn get_ssh_vnc_tunnel_args(&self, name: &str, vnc_port: u16) -> ContainerResult<(Vec<String>, u16)> {
        let key_path = self.get_ssh_privkey_path();
        let key_path_str = key_path.to_string_lossy().to_string();

        // Use a dynamic local port based on container name hash to avoid conflicts
        let local_port = 15900 + (name.bytes().fold(0u16, |acc, b| acc.wrapping_add(b as u16)) % 1000);

        let mut args = Vec::new();
        args.push("ssh".to_string());

        if self.config.is_rootless() {
            // Rootless: use port forwarding to localhost
            let port = self.get_container_ssh_port(name)?
                .ok_or_else(|| ContainerError::new("Container has no SSH port mapping"))?;
            args.extend([
                "-o".to_string(), "IdentitiesOnly=yes".to_string(),
                "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
                "-o".to_string(), "UserKnownHostsFile=/dev/null".to_string(),
                "-i".to_string(), key_path_str,
                "-p".to_string(), port.to_string(),
                "-N".to_string(), // No remote command
                "-L".to_string(), format!("{}:127.0.0.1:{}", local_port, vnc_port),
                "root@localhost".to_string(),
            ]);
        } else {
            // Rootful: use container IP directly
            let ip = self.get_container_ip(name)?
                .ok_or_else(|| ContainerError::new("Container has no IP address"))?;
            args.extend([
                "-o".to_string(), "IdentitiesOnly=yes".to_string(),
                "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
                "-i".to_string(), key_path_str,
                "-N".to_string(), // No remote command
                "-L".to_string(), format!("{}:127.0.0.1:{}", local_port, vnc_port),
                format!("root@{}", ip),
            ]);
        }

        Ok((args, local_port))
    }

    /// Get docker/podman exec command for direct container access (no SSH needed)
    /// This is the preferred method in rootless mode when prefer_exec is true
    /// Note: Returns a plain host command - Flatpak wrapping is handled by terminal spawning
    pub fn get_exec_command(&self, name: &str) -> String {
        match self.config.runtime {
            ContainerRuntime::Docker => {
                format!("{} exec -it {} bash", self.config.runtime.command(), name)
            }
            ContainerRuntime::Podman => {
                if self.config.is_rootful() {
                    format!("pkexec {} exec -it {} bash", self.config.runtime.command(), name)
                } else {
                    format!("{} exec -it {} bash", self.config.runtime.command(), name)
                }
            }
        }
    }

    /// Get exec arguments for spawning in terminal
    /// For Docker rootful mode or Podman rootful mode, this runs the exec directly
    /// (Docker uses socket permissions, Podman rootful containers are accessible to user after creation)
    /// Note: Returns plain host command args - Flatpak wrapping is handled by terminal spawning
    pub fn get_exec_args(&self, name: &str) -> Vec<String> {
        let mut args = Vec::new();

        // For Docker rootful, we need to ensure we use the correct socket
        // For Podman, exec on a rootful container works without pkexec since the container is running
        match self.config.runtime {
            ContainerRuntime::Docker => {
                args.push(self.config.runtime.command().to_string());
            }
            ContainerRuntime::Podman => {
                if self.config.is_rootful() {
                    // For Podman rootful containers, we need pkexec to exec into them
                    args.push("pkexec".to_string());
                    args.push(self.config.runtime.command().to_string());
                } else {
                    args.push(self.config.runtime.command().to_string());
                }
            }
        }

        args.extend([
            "exec".to_string(),
            "-it".to_string(),
            name.to_string(),
            "bash".to_string(),
        ]);
        args
    }

    /// Get the best connection command based on configuration
    /// Returns (command_string, is_exec) where is_exec indicates if using exec vs SSH
    pub fn get_connection_command(&self, name: &str) -> ContainerResult<(String, bool)> {
        if self.config.is_rootless() && self.config.prefer_exec {
            Ok((self.get_exec_command(name), true))
        } else {
            Ok((self.get_ssh_command(name)?, false))
        }
    }

    /// Get connection arguments for spawning in terminal
    /// Returns (args, is_exec) where is_exec indicates if using exec vs SSH
    pub fn get_connection_args(&self, name: &str) -> ContainerResult<(Vec<String>, bool)> {
        if self.config.is_rootless() && self.config.prefer_exec {
            Ok((self.get_exec_args(name), true))
        } else {
            Ok((self.get_ssh_args(name)?, false))
        }
    }

    /// Clear known hosts entry for container IP/localhost (for SSH reconnection)
    pub fn clear_ssh_known_host(&self, name: &str) -> ContainerResult<()> {
        let in_flatpak = is_flatpak();

        if self.config.is_rootless() {
            // For rootless, clear localhost entries for our port range
            // This is less precise but avoids accumulating stale entries
            if let Ok(Some(port)) = self.get_container_ssh_port(name) {
                let host_arg = format!("[localhost]:{}", port);
                if in_flatpak {
                    let _ = Command::new("flatpak-spawn")
                        .args(["--host", "ssh-keygen", "-R", &host_arg])
                        .output();
                } else {
                    let _ = Command::new("ssh-keygen")
                        .args(["-R", &host_arg])
                        .output();
                }
            }
        } else {
            // Rootful: clear by IP
            if let Some(ip) = self.get_container_ip(name)? {
                if in_flatpak {
                    let _ = Command::new("flatpak-spawn")
                        .args(["--host", "ssh-keygen", "-R", &ip])
                        .output();
                } else {
                    let _ = Command::new("ssh-keygen")
                        .args(["-R", &ip])
                        .output();
                }
            }
        }
        Ok(())
    }

    /// Execute a command inside a running container
    pub fn exec_in_container(&self, name: &str, command: &[&str]) -> ContainerResult<String> {
        let mut args = vec!["exec", name];
        args.extend(command);

        let output = self.execute(&args)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("Command execution failed", stderr))
        }
    }

    /// Get container logs
    pub fn get_logs(&self, name: &str, tail: Option<usize>) -> ContainerResult<String> {
        let mut args = vec!["logs".to_string()];

        if let Some(n) = tail {
            args.push("--tail".to_string());
            args.push(n.to_string());
        }
        args.push(name.to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.execute(&args_refs)?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    // ==================== X11 Support Methods ====================

    /// Enable X11 access for the current user only (runs xhost +SI:localuser:$USER)
    /// This is more secure than xhost +local: as it only allows the current user
    /// Returns Ok(true) if xhost was run successfully, Ok(false) if xhost not available
    pub fn enable_x11_access() -> ContainerResult<bool> {
        let in_flatpak = is_flatpak();

        // Check if xhost is available
        let xhost_check = if in_flatpak {
            Command::new("flatpak-spawn")
                .args(["--host", "which", "xhost"])
                .output()
        } else {
            Command::new("which")
                .arg("xhost")
                .output()
        };

        if xhost_check.is_err() || !xhost_check.unwrap().status.success() {
            return Ok(false);
        }

        // Get current username
        let username = std::env::var("USER").unwrap_or_else(|_| "root".to_string());

        // Run xhost +SI:localuser:$USER to allow only current user's local connections
        // This is more secure than +local: which allows any local user
        let output = if in_flatpak {
            Command::new("flatpak-spawn")
                .args(["--host", "xhost", &format!("+SI:localuser:{}", username)])
                .output()
                .map_err(|e| ContainerError::with_details("Failed to run xhost", e))?
        } else {
            Command::new("xhost")
                .arg(format!("+SI:localuser:{}", username))
                .output()
                .map_err(|e| ContainerError::with_details("Failed to run xhost", e))?
        };

        if output.status.success() {
            log::info!("X11 access enabled for user: {}", username);
            Ok(true)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ContainerError::with_details("xhost command failed", stderr))
        }
    }

    /// Disable X11 access for the current user (runs xhost -SI:localuser:$USER)
    /// Call this when done with GUI containers for better security
    pub fn disable_x11_access() -> ContainerResult<bool> {
        let in_flatpak = is_flatpak();

        // Get current username
        let username = std::env::var("USER").unwrap_or_else(|_| "root".to_string());

        let output = if in_flatpak {
            Command::new("flatpak-spawn")
                .args(["--host", "xhost", &format!("-SI:localuser:{}", username)])
                .output()
                .map_err(|e| ContainerError::with_details("Failed to run xhost", e))?
        } else {
            Command::new("xhost")
                .arg(format!("-SI:localuser:{}", username))
                .output()
                .map_err(|e| ContainerError::with_details("Failed to run xhost", e))?
        };

        if output.status.success() {
            log::info!("X11 access disabled for user: {}", username);
        }

        Ok(output.status.success())
    }

    /// Cleanup X11 access - should be called on application shutdown
    /// This ensures we don't leave xhost permissions open after the app closes
    pub fn cleanup_x11_access() {
        if let Err(e) = Self::disable_x11_access() {
            log::warn!("Failed to cleanup X11 access: {}", e);
        }
    }

    /// Check X11 configuration and return diagnostic information
    /// Returns a struct with details about X11 availability and configuration
    pub fn diagnose_x11() -> X11Diagnostic {
        let mut diag = X11Diagnostic::default();

        // Check DISPLAY environment variable
        diag.display = std::env::var("DISPLAY").ok();
        diag.has_display = diag.display.is_some();

        // Check if X11 socket exists
        diag.x11_socket_exists = std::path::Path::new("/tmp/.X11-unix").exists();

        // Check XAUTHORITY
        diag.xauthority = std::env::var("XAUTHORITY").ok();
        if diag.xauthority.is_none() {
            // Try default location
            if let Ok(home) = std::env::var("HOME") {
                let default_xauth = format!("{}/.Xauthority", home);
                if std::path::Path::new(&default_xauth).exists() {
                    diag.xauthority = Some(default_xauth);
                }
            }
        }
        diag.has_xauthority = diag.xauthority.is_some();

        // Check if xhost is available
        let in_flatpak = is_flatpak();
        diag.xhost_available = if in_flatpak {
            Command::new("flatpak-spawn")
                .args(["--host", "which", "xhost"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            Command::new("which")
                .arg("xhost")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };

        // Check current xhost access control
        if diag.xhost_available {
            let xhost_output = if in_flatpak {
                Command::new("flatpak-spawn")
                    .args(["--host", "xhost"])
                    .output()
            } else {
                Command::new("xhost").output()
            };
            if let Ok(output) = xhost_output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Check for both old-style LOCAL: and new-style SI:localuser:username
                let username = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
                diag.xhost_local_enabled = stdout.contains("LOCAL:") ||
                                           stdout.contains(&format!("SI:localuser:{}", username)) ||
                                           stdout.contains("access control disabled");
                diag.xhost_output = Some(stdout.to_string());
            }
        }

        // Check if we're on Wayland with XWayland
        diag.wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
        diag.is_wayland = diag.wayland_display.is_some();

        // Overall readiness check
        diag.is_ready = diag.has_display &&
                        diag.x11_socket_exists &&
                        (diag.xhost_local_enabled || diag.has_xauthority);

        diag
    }

    /// Test X11 connectivity from within a container
    /// Runs xdpyinfo or xeyes inside the container to verify X11 works
    pub fn test_x11_in_container(&self, name: &str) -> ContainerResult<X11TestResult> {
        let mut result = X11TestResult::default();

        // First check if container is running
        let containers = self.list_containers()?;
        let container = containers.iter().find(|c| c.name == name);

        match container {
            Some(c) if !c.is_running() => {
                result.error = Some("Container is not running".to_string());
                return Ok(result);
            }
            None => {
                result.error = Some("Container not found".to_string());
                return Ok(result);
            }
            _ => {}
        }

        // Try to run xdpyinfo inside the container
        let xdpyinfo_result = self.exec_in_container(name, &["xdpyinfo", "-display", ":0"]);
        match xdpyinfo_result {
            Ok(output) => {
                result.xdpyinfo_works = !output.is_empty() && !output.contains("unable to open");
                result.xdpyinfo_output = Some(output);
            }
            Err(e) => {
                result.xdpyinfo_works = false;
                result.xdpyinfo_output = Some(format!("Failed: {}", e));
            }
        }

        // Check if DISPLAY is set in container
        let display_result = self.exec_in_container(name, &["printenv", "DISPLAY"]);
        result.container_display = display_result.ok().map(|s| s.trim().to_string());

        // Check if XAUTHORITY is set
        let xauth_result = self.exec_in_container(name, &["printenv", "XAUTHORITY"]);
        result.container_xauthority = xauth_result.ok().map(|s| s.trim().to_string());

        // Check if X11 socket is accessible
        let socket_result = self.exec_in_container(name, &["ls", "-la", "/tmp/.X11-unix/"]);
        result.x11_socket_accessible = socket_result.is_ok();
        result.x11_socket_output = socket_result.ok();

        result.success = result.xdpyinfo_works;

        Ok(result)
    }

    /// Prepare the host for X11 GUI containers in rootless mode
    /// This sets up xhost access and returns diagnostic information
    pub fn prepare_x11_for_rootless(&self) -> ContainerResult<X11Diagnostic> {
        // Enable X11 access
        let _ = Self::enable_x11_access();

        // Return current diagnostic state
        Ok(Self::diagnose_x11())
    }
}

/// X11 diagnostic information
#[derive(Debug, Default, Clone)]
pub struct X11Diagnostic {
    /// Whether X11 appears ready for container use
    pub is_ready: bool,
    /// DISPLAY environment variable value
    pub display: Option<String>,
    /// Whether DISPLAY is set
    pub has_display: bool,
    /// Whether /tmp/.X11-unix exists
    pub x11_socket_exists: bool,
    /// XAUTHORITY path if found
    pub xauthority: Option<String>,
    /// Whether XAUTHORITY file exists
    pub has_xauthority: bool,
    /// Whether xhost command is available
    pub xhost_available: bool,
    /// Whether xhost +local: is enabled
    pub xhost_local_enabled: bool,
    /// Raw xhost output
    pub xhost_output: Option<String>,
    /// WAYLAND_DISPLAY if on Wayland
    pub wayland_display: Option<String>,
    /// Whether running on Wayland (with XWayland)
    pub is_wayland: bool,
}

impl X11Diagnostic {
    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        if self.is_ready {
            lines.push("✅ X11 appears ready for container GUI apps".to_string());
        } else {
            lines.push("❌ X11 is not fully configured".to_string());
        }

        if let Some(ref display) = self.display {
            lines.push(format!("  DISPLAY: {}", display));
        } else {
            lines.push("  ❌ DISPLAY not set".to_string());
        }

        if self.x11_socket_exists {
            lines.push("  ✅ X11 socket exists (/tmp/.X11-unix)".to_string());
        } else {
            lines.push("  ❌ X11 socket not found".to_string());
        }

        if let Some(ref xauth) = self.xauthority {
            lines.push(format!("  XAUTHORITY: {}", xauth));
        } else {
            lines.push("  ⚠️  XAUTHORITY not found".to_string());
        }

        if self.xhost_available {
            if self.xhost_local_enabled {
                lines.push("  ✅ xhost local access enabled".to_string());
            } else {
                lines.push("  ⚠️  xhost local access not enabled (run: xhost +SI:localuser:$USER)".to_string());
            }
        } else {
            lines.push("  ⚠️  xhost not available".to_string());
        }

        if self.is_wayland {
            lines.push(format!("  ℹ️  Running on Wayland ({}), using XWayland",
                self.wayland_display.as_deref().unwrap_or("unknown")));
        }

        lines.join("\n")
    }

    /// Get list of issues that need to be fixed
    pub fn issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if !self.has_display {
            issues.push("DISPLAY environment variable is not set".to_string());
        }

        if !self.x11_socket_exists {
            issues.push("X11 socket (/tmp/.X11-unix) does not exist".to_string());
        }

        if !self.has_xauthority && !self.xhost_local_enabled {
            issues.push("No XAUTHORITY file and xhost local access not enabled".to_string());
        }

        if self.xhost_available && !self.xhost_local_enabled {
            issues.push("Run 'xhost +SI:localuser:$USER' to allow container X11 access".to_string());
        }

        issues
    }
}

/// Result of testing X11 inside a container
#[derive(Debug, Default, Clone)]
pub struct X11TestResult {
    /// Whether X11 test was successful
    pub success: bool,
    /// Whether xdpyinfo ran successfully
    pub xdpyinfo_works: bool,
    /// xdpyinfo output
    pub xdpyinfo_output: Option<String>,
    /// DISPLAY inside container
    pub container_display: Option<String>,
    /// XAUTHORITY inside container
    pub container_xauthority: Option<String>,
    /// Whether X11 socket is accessible in container
    pub x11_socket_accessible: bool,
    /// Output from ls on X11 socket dir
    pub x11_socket_output: Option<String>,
    /// Error message if test couldn't run
    pub error: Option<String>,
}

impl X11TestResult {
    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        if let Some(ref error) = self.error {
            return format!("❌ Test failed: {}", error);
        }

        if self.success {
            lines.push("✅ X11 is working in container".to_string());
        } else {
            lines.push("❌ X11 is not working in container".to_string());
        }

        if let Some(ref display) = self.container_display {
            lines.push(format!("  Container DISPLAY: {}", display));
        } else {
            lines.push("  ❌ DISPLAY not set in container".to_string());
        }

        if self.x11_socket_accessible {
            lines.push("  ✅ X11 socket accessible".to_string());
        } else {
            lines.push("  ❌ X11 socket not accessible".to_string());
        }

        if let Some(ref xauth) = self.container_xauthority {
            lines.push(format!("  Container XAUTHORITY: {}", xauth));
        }

        if !self.success {
            if let Some(ref output) = self.xdpyinfo_output {
                lines.push(format!("  xdpyinfo output: {}", output.lines().next().unwrap_or("")));
            }
        }

        lines.join("\n")
    }
}

/// Gets the container config file path
pub fn get_container_config_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("container_config.yaml");
    path
}

/// Load container configuration from file
pub fn load_container_config() -> ContainerConfig {
    let path = get_container_config_path();

    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_yaml::from_str(&content) {
                return config;
            }
        }
    }

    ContainerConfig::default()
}

/// Save container configuration to file
pub fn save_container_config(config: &ContainerConfig) -> Result<(), String> {
    let path = get_container_config_path();

    let yaml = serde_yaml::to_string(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, yaml)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_config_default() {
        let config = ContainerConfig::default();
        assert_eq!(config.image_name, "kali-build");
        assert_eq!(config.master_image, "kali-master");
        assert_eq!(config.vnc_port, 5900);
    }

    #[test]
    fn test_container_runtime() {
        assert_eq!(ContainerRuntime::Podman.command(), "podman");
        assert_eq!(ContainerRuntime::Docker.command(), "docker");
        assert!(ContainerRuntime::Podman.needs_sudo_by_default());
        assert!(!ContainerRuntime::Docker.needs_sudo_by_default());
        assert!(ContainerRuntime::Podman.supports_pkexec());
        assert!(!ContainerRuntime::Docker.supports_pkexec());
        assert_eq!(ContainerRuntime::Podman.pkexec_command(), "pkexec");
    }

    #[test]
    fn test_container_info_is_running() {
        let running = ContainerInfo {
            name: "test".to_string(),
            status: "running".to_string(),
            ip_address: None,
            image: "kali".to_string(),
            id: "abc123".to_string(),
        };
        assert!(running.is_running());

        let stopped = ContainerInfo {
            name: "test".to_string(),
            status: "exited".to_string(),
            ip_address: None,
            image: "kali".to_string(),
            id: "abc123".to_string(),
        };
        assert!(!stopped.is_running());
    }
}
