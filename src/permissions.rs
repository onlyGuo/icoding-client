use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct FullDiskAccessStatus {
    pub required: bool,
    pub granted: bool,
    pub detail: String,
}

#[cfg(target_os = "macos")]
pub fn full_disk_access_status() -> FullDiskAccessStatus {
    use std::{fs, io::ErrorKind};

    let Some(home) = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) else {
        return FullDiskAccessStatus {
            required: true,
            granted: false,
            detail: "cannot resolve the current user's home directory".to_string(),
        };
    };

    let directory_probes = [home.join("Library/Mail")];
    for path in directory_probes {
        match fs::read_dir(&path) {
            Ok(mut entries) => match entries.next().transpose() {
                Ok(_) => {
                    return FullDiskAccessStatus {
                        required: true,
                        granted: true,
                        detail: format!("verified access to {}", path.display()),
                    };
                }
                Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                    return denied(path.display().to_string());
                }
                Err(error) => {
                    return FullDiskAccessStatus {
                        required: true,
                        granted: false,
                        detail: format!("failed to verify {}: {error}", path.display()),
                    };
                }
            },
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                return denied(path.display().to_string());
            }
            Err(error) => {
                return FullDiskAccessStatus {
                    required: true,
                    granted: false,
                    detail: format!("failed to verify {}: {error}", path.display()),
                };
            }
        }
    }

    let file_probes = [
        home.join("Library/Safari/History.db"),
        home.join("Library/Messages/chat.db"),
    ];
    for path in file_probes {
        match fs::File::open(&path) {
            Ok(_) => {
                return FullDiskAccessStatus {
                    required: true,
                    granted: true,
                    detail: format!("verified access to {}", path.display()),
                };
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                return denied(path.display().to_string());
            }
            Err(error) => {
                return FullDiskAccessStatus {
                    required: true,
                    granted: false,
                    detail: format!("failed to verify {}: {error}", path.display()),
                };
            }
        }
    }

    FullDiskAccessStatus {
        required: true,
        granted: false,
        detail: "no protected macOS location was available to verify Full Disk Access".to_string(),
    }
}

#[cfg(target_os = "macos")]
fn denied(path: String) -> FullDiskAccessStatus {
    FullDiskAccessStatus {
        required: true,
        granted: false,
        detail: format!("macOS denied access to protected location {path}"),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn full_disk_access_status() -> FullDiskAccessStatus {
    FullDiskAccessStatus {
        required: false,
        granted: true,
        detail: "Full Disk Access is only required on macOS".to_string(),
    }
}

pub fn require_startup_permissions(open_settings: bool) -> Result<()> {
    let status = full_disk_access_status();
    if status.granted {
        return Ok(());
    }

    if open_settings {
        open_full_disk_access_settings()?;
    }

    bail!(
        "Full Disk Access is required before the agent can start: {}. Grant iCoding Client access in System Settings > Privacy & Security > Full Disk Access, then restart the app",
        status.detail
    )
}

#[cfg(target_os = "macos")]
pub fn open_full_disk_access_settings() -> Result<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status()
        .context("failed to open macOS Full Disk Access settings")?;
    if !status.success() {
        bail!("failed to open macOS Full Disk Access settings: open exited with {status}");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn open_full_disk_access_settings() -> Result<()> {
    Ok(())
}
