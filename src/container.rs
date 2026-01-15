//! Container management for PenEnv
//!
//! Manages Kali/pentest containers using podman or docker.
//! Provides functionality to build, create, start, stop, and connect to containers.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::process::{Command, Output};
use std::path::PathBuf;
use std::fs;
use crate::config::get_config_dir;

/// Container runtime choice - podman or docker
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
pub enum ContainerRuntime {
    #[default]
    Podman,
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

    /// Check if this runtime needs elevated privileges for networking access
    pub fn needs_sudo(&self) -> bool {
        // Podman needs elevated privileges for full networking access
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

/// Container configuration settings
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContainerConfig {
    /// Container runtime (podman or docker)
    pub runtime: ContainerRuntime,
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
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            runtime: ContainerRuntime::default(),
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

    /// Get the base command with optional pkexec for privilege escalation
    /// Uses pkexec to show the native GNOME PolicyKit authentication dialog
    fn base_command(&self) -> Command {
        if self.config.runtime.needs_sudo() {
            let mut cmd = Command::new("pkexec");
            cmd.arg(self.config.runtime.command());
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

    /// Check if the container runtime is available
    /// When using pkexec, this will show the native GNOME PolicyKit authentication dialog
    pub fn is_runtime_available(&self) -> bool {
        let result = if self.config.runtime.needs_sudo() {
            Command::new("pkexec")
                .args([self.config.runtime.command(), "--version"])
                .output()
        } else {
            Command::new(self.config.runtime.command())
                .arg("--version")
                .output()
        };

        result.map(|o| o.status.success()).unwrap_or(false)
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
        let output = self.execute(&[
            "inspect", name,
            "--format", "{{.NetworkSettings.IPAddress}}"
        ])?;

        if !output.status.success() {
            return Ok(None);
        }

        let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if ip.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ip))
        }
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
    pub fn ensure_ssh_key(&self) -> ContainerResult<String> {
        let pubkey_path = PathBuf::from(&self.config.ssh_pubkey_path);

        if pubkey_path.exists() {
            fs::read_to_string(&pubkey_path)
                .map_err(|e| ContainerError::with_details("Failed to read SSH key", e))
        } else {
            // Generate new SSH key
            let privkey_path = self.config.ssh_pubkey_path.replace(".pub", "");

            let status = Command::new("ssh-keygen")
                .args(["-t", "ed25519", "-f", &privkey_path, "-N", ""])
                .status()
                .map_err(|e| ContainerError::with_details("Failed to generate SSH key", e))?;

            if !status.success() {
                return Err(ContainerError::new("SSH key generation failed"));
            }

            fs::read_to_string(&pubkey_path)
                .map_err(|e| ContainerError::with_details("Failed to read generated SSH key", e))
        }
    }

    /// Ensure data mapping directory exists
    pub fn ensure_data_dir(&self) -> ContainerResult<()> {
        let data_path = PathBuf::from(&self.config.data_mapping);

        if !data_path.exists() {
            fs::create_dir_all(&data_path)
                .map_err(|e| ContainerError::with_details("Failed to create data directory", e))?;
        }

        Ok(())
    }

    /// Create and run a new container
    pub fn create_container(
        &self,
        name: &str,
        use_master: bool,
        detached: bool,
        temporary: bool,
    ) -> ContainerResult<()> {
        // Ensure prerequisites
        self.ensure_data_dir()?;
        let ssh_key = self.ensure_ssh_key()?;

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
        let data_path = fs::canonicalize(&self.config.data_mapping)
            .unwrap_or_else(|_| PathBuf::from(&self.config.data_mapping));
        args.extend(vec![
            "-v".to_string(),
            format!("{}:/data:rw,z", data_path.display()),
        ]);

        // Capabilities for networking tools
        args.extend(vec![
            "--cap-add".to_string(), "NET_ADMIN".to_string(),
            "--cap-add".to_string(), "AUDIT_WRITE".to_string(),
            "--cap-add".to_string(), "NET_RAW".to_string(),
        ]);

        // TUN device for VPN support
        args.extend(vec![
            "--device=/dev/net/tun".to_string(),
        ]);

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
    pub fn get_ssh_command(&self, name: &str) -> ContainerResult<String> {
        let ip = self.get_container_ip(name)?
            .ok_or_else(|| ContainerError::new("Container has no IP address"))?;

        let key_path = self.config.ssh_pubkey_path.replace(".pub", "");
        Ok(format!(
            "ssh -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -i {} root@{} -X -L 8181:127.0.0.1:8181",
            key_path, ip
        ))
    }

    /// Get SSH connection arguments for spawning in terminal
    pub fn get_ssh_args(&self, name: &str) -> ContainerResult<Vec<String>> {
        let ip = self.get_container_ip(name)?
            .ok_or_else(|| ContainerError::new("Container has no IP address"))?;

        let key_path = self.config.ssh_pubkey_path.replace(".pub", "");
        Ok(vec![
            "ssh".to_string(),
            "-o".to_string(), "IdentitiesOnly=yes".to_string(),
            "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
            "-i".to_string(), key_path,
            format!("root@{}", ip),
            "-X".to_string(),
            "-L".to_string(), "8181:127.0.0.1:8181".to_string(),
        ])
    }

    /// Clear known hosts entry for container IP (for SSH reconnection)
    pub fn clear_ssh_known_host(&self, name: &str) -> ContainerResult<()> {
        if let Some(ip) = self.get_container_ip(name)? {
            let _ = Command::new("ssh-keygen")
                .args(["-R", &ip])
                .output();
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
        assert!(ContainerRuntime::Podman.needs_sudo());
        assert!(!ContainerRuntime::Docker.needs_sudo());
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
