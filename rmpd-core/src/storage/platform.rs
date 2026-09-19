use crate::error::{Result, RmpdError};
use std::path::Path;
use std::process::Command;

/// Parse URI into protocol and address (e.g. "nfs://server/path" -> ("nfs", "server/path"))
fn parse_uri(uri: &str) -> Result<(String, String)> {
    if let Some(pos) = uri.find("://") {
        let protocol = uri[..pos].trim().to_lowercase();
        let address = uri[pos + 3..].trim();
        if protocol.is_empty() || address.is_empty() {
            return Err(RmpdError::Storage(format!("Invalid URI format: {uri}")));
        }
        Ok((protocol, address.to_string()))
    } else {
        Err(RmpdError::Storage(format!("Invalid URI format: {uri}")))
    }
}

/// Validate mount options before joining with commas for `mount -o`.
fn validate_mount_options(options: &[String]) -> Result<()> {
    for opt in options {
        if opt.trim().is_empty() {
            return Err(RmpdError::Storage(
                "Invalid mount option: empty option".to_string(),
            ));
        }
        if opt.contains(',') {
            return Err(RmpdError::Storage(format!(
                "Invalid mount option (contains comma): {opt}"
            )));
        }
        if opt.contains('\0') {
            return Err(RmpdError::Storage(
                "Invalid mount option: contains NUL byte".to_string(),
            ));
        }
    }
    Ok(())
}

fn has_username_option(options: &[String]) -> bool {
    options.iter().any(|opt| {
        let trimmed = opt.trim_start();
        trimmed == "username" || trimmed.starts_with("username=")
    })
}

fn is_already_unmounted_message(stderr: &str) -> bool {
    let msg = stderr.to_lowercase();
    msg.contains("not mounted") || msg.contains("not currently mounted")
}

#[cfg(any(test, target_os = "macos"))]
fn mount_output_contains_mountpoint(output: &str, mountpoint: &str) -> bool {
    output.lines().any(|line| {
        if let Some(on_idx) = line.find(" on ") {
            let after_on = &line[on_idx + 4..];
            if let Some(flags_idx) = after_on.find(" (") {
                return &after_on[..flags_idx] == mountpoint;
            }
        }
        false
    })
}

/// Convert a Path to a UTF-8 str, returning a descriptive error on failure
fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| RmpdError::Storage(format!("Invalid UTF-8 in path: {}", path.display())))
}

/// Platform-agnostic mount backend trait
pub trait MountBackend: Send + Sync {
    /// Mount a remote filesystem
    fn mount(&self, uri: &str, mountpoint: &Path, options: &[String]) -> Result<()>;

    /// Unmount a filesystem
    fn unmount(&self, mountpoint: &Path) -> Result<()>;

    /// Check if a path is currently mounted
    fn is_mounted(&self, mountpoint: &Path) -> bool;
}

/// Linux mount backend using system mount commands
#[cfg(target_os = "linux")]
pub struct LinuxMountBackend;

#[cfg(target_os = "linux")]
impl Default for LinuxMountBackend {
    fn default() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl LinuxMountBackend {
    pub fn new() -> Self {
        Self
    }

    /// Execute mount command and check result
    fn execute_mount_command(
        &self,
        fs_type: &str,
        source: &str,
        target: &str,
        options: &[String],
    ) -> Result<()> {
        validate_mount_options(options)?;

        let mut cmd = Command::new("mount");
        cmd.arg("-t").arg(fs_type).arg(source).arg(target);

        if !options.is_empty() {
            cmd.arg("-o").arg(options.join(","));
        }

        let output = cmd
            .output()
            .map_err(|e| RmpdError::Storage(format!("Failed to execute mount command: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RmpdError::Storage(format!("Mount failed: {stderr}")));
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl MountBackend for LinuxMountBackend {
    fn mount(&self, uri: &str, mountpoint: &Path, options: &[String]) -> Result<()> {
        let (protocol, address) = parse_uri(uri)?;

        match protocol.as_str() {
            "nfs" => {
                // NFS mount: mount -t nfs server:/path /mountpoint
                tracing::info!("mounting NFS: {} -> {}", address, mountpoint.display());
                self.execute_mount_command("nfs", &address, path_to_str(mountpoint)?, options)
            }
            "smb" | "cifs" => {
                // SMB/CIFS mount: mount -t cifs //server/share /mountpoint
                let cifs_path = if address.starts_with("//") {
                    address.clone()
                } else {
                    format!("//{}", address)
                };

                tracing::info!("mounting CIFS: {} -> {}", cifs_path, mountpoint.display());

                // Add guest option if no credentials provided
                let mut mount_options = options.to_vec();
                if !has_username_option(options) {
                    mount_options.push("guest".to_string());
                }

                self.execute_mount_command(
                    "cifs",
                    &cifs_path,
                    path_to_str(mountpoint)?,
                    &mount_options,
                )
            }
            "webdav" | "http" | "https" => {
                // WebDAV would require davfs2 to be installed
                // mount -t davfs http://server/path /mountpoint
                tracing::warn!("webDAV mounting requires davfs2 to be installed and configured");

                // Try with davfs
                self.execute_mount_command("davfs", uri, path_to_str(mountpoint)?, options)
            }
            _ => Err(RmpdError::Storage(format!(
                "Unsupported protocol: {protocol}"
            ))),
        }
    }

    fn unmount(&self, mountpoint: &Path) -> Result<()> {
        tracing::info!("unmounting: {}", mountpoint.display());

        let output = Command::new("umount")
            .arg(path_to_str(mountpoint)?)
            .output()
            .map_err(|e| RmpdError::Storage(format!("Failed to execute umount: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check if it's because it's not mounted
            if is_already_unmounted_message(&stderr) {
                return Ok(()); // Already unmounted, treat as success
            }

            return Err(RmpdError::Storage(format!("Unmount failed: {stderr}")));
        }

        Ok(())
    }

    fn is_mounted(&self, mountpoint: &Path) -> bool {
        // Check /proc/mounts to see if path is mounted
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            let mountpoint_str = mountpoint.to_string_lossy();
            mounts.lines().any(|line| {
                line.split_whitespace()
                    .nth(1)
                    .map(|mp| mp == mountpoint_str)
                    .unwrap_or(false)
            })
        } else {
            false
        }
    }
}

/// macOS mount backend using system mount commands
#[cfg(target_os = "macos")]
pub struct MacOSMountBackend;

#[cfg(target_os = "macos")]
impl MacOSMountBackend {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl MountBackend for MacOSMountBackend {
    fn mount(&self, uri: &str, mountpoint: &Path, _options: &[String]) -> Result<()> {
        let (protocol, address) = parse_uri(uri)?;

        match protocol.as_str() {
            "nfs" => {
                // macOS NFS mount: mount -t nfs server:/path /mountpoint
                let output = Command::new("mount")
                    .arg("-t")
                    .arg("nfs")
                    .arg(&address)
                    .arg(path_to_str(mountpoint)?)
                    .output()
                    .map_err(|e| RmpdError::Storage(format!("Mount command failed: {e}")))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(RmpdError::Storage(format!("Mount failed: {stderr}")));
                }
                Ok(())
            }
            "smb" | "cifs" => {
                // macOS SMB mount: mount -t smbfs //server/share /mountpoint
                let smb_path = if address.starts_with("//") {
                    address.clone()
                } else {
                    format!("//{}", address)
                };

                let output = Command::new("mount")
                    .arg("-t")
                    .arg("smbfs")
                    .arg(&smb_path)
                    .arg(path_to_str(mountpoint)?)
                    .output()
                    .map_err(|e| RmpdError::Storage(format!("Mount command failed: {e}")))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(RmpdError::Storage(format!("Mount failed: {stderr}")));
                }
                Ok(())
            }
            _ => Err(RmpdError::Storage(format!(
                "Unsupported protocol: {protocol}"
            ))),
        }
    }

    fn unmount(&self, mountpoint: &Path) -> Result<()> {
        let output = Command::new("umount")
            .arg(path_to_str(mountpoint)?)
            .output()
            .map_err(|e| RmpdError::Storage(format!("Unmount command failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if is_already_unmounted_message(&stderr) {
                return Ok(());
            }
            return Err(RmpdError::Storage(format!("Unmount failed: {stderr}")));
        }

        Ok(())
    }

    fn is_mounted(&self, mountpoint: &Path) -> bool {
        // Use mount command to check if mounted
        if let Ok(output) = Command::new("mount").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mountpoint_str = mountpoint.to_string_lossy();
            mount_output_contains_mountpoint(&stdout, &mountpoint_str)
        } else {
            false
        }
    }
}

/// Stub backend for unsupported platforms
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub struct StubMountBackend;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl MountBackend for StubMountBackend {
    fn mount(&self, _uri: &str, _mountpoint: &Path, _options: &[String]) -> Result<()> {
        Err(RmpdError::Storage(
            "Mounting not supported on this platform".to_string(),
        ))
    }

    fn unmount(&self, _mountpoint: &Path) -> Result<()> {
        Err(RmpdError::Storage(
            "Unmounting not supported on this platform".to_string(),
        ))
    }

    fn is_mounted(&self, _mountpoint: &Path) -> bool {
        false
    }
}

/// Get the default mount backend for the current platform
pub fn get_default_backend() -> Box<dyn MountBackend> {
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxMountBackend::new())
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(MacOSMountBackend::new())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Box::new(StubMountBackend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri() {
        let (proto, addr) = parse_uri("nfs://192.168.1.100/music").unwrap();
        assert_eq!(proto, "nfs");
        assert_eq!(addr, "192.168.1.100/music");

        let (proto, addr) = parse_uri("smb://server/share").unwrap();
        assert_eq!(proto, "smb");
        assert_eq!(addr, "server/share");
    }

    #[test]
    fn test_parse_uri_invalid() {
        let result = parse_uri("invalid_uri");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_uri_rejects_empty_parts() {
        assert!(parse_uri("://server/path").is_err());
        assert!(parse_uri("nfs://").is_err());
        assert!(parse_uri("nfs://   ").is_err());
    }

    #[test]
    fn test_parse_uri_normalizes_protocol_and_trims() {
        let (proto, addr) = parse_uri("  SMB:// server/share  ").unwrap();
        assert_eq!(proto, "smb");
        assert_eq!(addr, "server/share");
    }

    #[test]
    fn test_validate_mount_options_rejects_invalid_inputs() {
        assert!(validate_mount_options(&["".to_string()]).is_err());
        assert!(validate_mount_options(&["user,name=foo".to_string()]).is_err());
        assert!(validate_mount_options(&["user\0name=foo".to_string()]).is_err());
    }

    #[test]
    fn test_validate_mount_options_accepts_normal_inputs() {
        assert!(validate_mount_options(&["ro".to_string(), "username=foo".to_string()]).is_ok());
    }

    #[test]
    fn test_has_username_option_exact_key_only() {
        assert!(has_username_option(&["username=alice".to_string()]));
        assert!(has_username_option(&["username".to_string()]));
        assert!(has_username_option(&["  username=bob".to_string()]));
        assert!(!has_username_option(&["notusername=alice".to_string()]));
        assert!(!has_username_option(&["guest".to_string()]));
    }

    #[test]
    fn test_mount_output_parser_matches_exact_mountpoint() {
        let out = "//server/share on /mnt/music (smbfs, nodev, nosuid)\n//server/other on /mnt/music2 (smbfs)\n";
        assert!(mount_output_contains_mountpoint(out, "/mnt/music"));
        assert!(!mount_output_contains_mountpoint(out, "/mnt/mus"));
    }

    #[test]
    fn test_is_already_unmounted_message_patterns() {
        assert!(is_already_unmounted_message("umount: /tmp/x: not mounted"));
        assert!(is_already_unmounted_message(
            "umount: /tmp/x: not currently mounted"
        ));
        assert!(!is_already_unmounted_message("permission denied"));
    }

    #[test]
    fn test_path_to_str_valid_utf8_path() {
        let p = Path::new("/tmp/music");
        assert_eq!(path_to_str(p).unwrap(), "/tmp/music");
    }

    #[cfg(unix)]
    #[test]
    fn test_path_to_str_rejects_non_utf8_path() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let bytes = b"/tmp/invalid-\xff";
        let non_utf8 = Path::new(OsStr::from_bytes(bytes));
        let err = path_to_str(non_utf8).unwrap_err();
        assert!(err.to_string().contains("Invalid UTF-8 in path"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_execute_mount_command_rejects_invalid_options_before_spawn() {
        let backend = LinuxMountBackend::new();
        let err = backend
            .execute_mount_command(
                "nfs",
                "server:/music",
                "/mnt/music",
                &["user,name=bad".to_string()],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("Invalid mount option"),
            "got: {err}"
        );
    }

    #[test]
    fn test_mount_output_parser_ignores_malformed_lines() {
        let out = "garbage line\n/dev/disk1s1 on / (apfs, local, read-only)\n";
        assert!(!mount_output_contains_mountpoint(out, "/mnt/music"));
        assert!(mount_output_contains_mountpoint(out, "/"));
    }

    #[test]
    fn test_get_default_backend() {
        let backend = get_default_backend();
        // Just ensure it doesn't panic
        assert!(!backend.is_mounted(Path::new("/nonexistent")));
    }
}
