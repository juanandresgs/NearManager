use std::process::Command;

use near_core::{DeviceDisconnectReport, Location, RemovableDevice, RemovableDeviceService};

#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformRemovableDeviceService;

impl RemovableDeviceService for PlatformRemovableDeviceService {
    fn list_devices(&self) -> Result<Vec<RemovableDevice>, String> {
        list_devices()
    }

    fn disconnect(&self, id: &str) -> Result<DeviceDisconnectReport, String> {
        let device = self
            .list_devices()?
            .into_iter()
            .find(|device| device.id == id)
            .ok_or_else(|| format!("removable device {id} is no longer attached"))?;
        if !device.can_disconnect {
            return Err(format!(
                "device {} cannot be safely disconnected",
                device.label
            ));
        }
        disconnect_device(&device)
    }
}

#[cfg(target_os = "macos")]
fn list_devices() -> Result<Vec<RemovableDevice>, String> {
    let output = Command::new("/sbin/mount")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("mount exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_macos_mount)
        .collect())
}

#[cfg(target_os = "macos")]
fn parse_macos_mount(line: &str) -> Option<RemovableDevice> {
    let (device, rest) = line.split_once(" on ")?;
    let (mount, _) = rest.split_once(" (")?;
    if !device.starts_with("/dev/disk") || !mount.starts_with("/Volumes/") {
        return None;
    }
    Some(RemovableDevice {
        id: device.to_owned(),
        label: mount.rsplit('/').next().unwrap_or(device).to_owned(),
        mount: Some(Location::new(format!("file://{mount}"))),
        system_path: device.to_owned(),
        can_disconnect: true,
    })
}

#[cfg(target_os = "macos")]
fn disconnect_device(device: &RemovableDevice) -> Result<DeviceDisconnectReport, String> {
    run_disconnect(
        "/usr/sbin/diskutil",
        &["eject", device.system_path.as_str()],
        device,
        "diskutil eject",
    )
}

#[cfg(target_os = "linux")]
fn list_devices() -> Result<Vec<RemovableDevice>, String> {
    let output = Command::new("lsblk")
        .args(["-P", "-o", "PATH,LABEL,MOUNTPOINT,RM,TYPE"])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("lsblk exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_linux_device)
        .collect())
}

#[cfg(target_os = "linux")]
fn parse_linux_device(line: &str) -> Option<RemovableDevice> {
    let fields = parse_quoted_fields(line);
    if fields.get("RM").map(String::as_str) != Some("1") {
        return None;
    }
    let path = fields.get("PATH")?.clone();
    let mount = fields.get("MOUNTPOINT").filter(|value| !value.is_empty());
    let label = fields
        .get("LABEL")
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| path.rsplit('/').next().map(str::to_owned))?;
    Some(RemovableDevice {
        id: path.clone(),
        label,
        mount: mount.map(|mount| Location::new(format!("file://{mount}"))),
        system_path: path,
        can_disconnect: true,
    })
}

#[cfg(target_os = "linux")]
fn disconnect_device(device: &RemovableDevice) -> Result<DeviceDisconnectReport, String> {
    run_disconnect(
        "udisksctl",
        &["unmount", "-b", device.system_path.as_str()],
        device,
        "udisksctl unmount",
    )?;
    run_disconnect(
        "udisksctl",
        &["power-off", "-b", device.system_path.as_str()],
        device,
        "udisksctl power-off",
    )
}

#[cfg(windows)]
fn list_devices() -> Result<Vec<RemovableDevice>, String> {
    let script = "Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=2' | ForEach-Object { \"$($_.DeviceID)|$($_.VolumeName)\" }";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("PowerShell exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (drive, label) = line.trim().split_once('|')?;
            Some(RemovableDevice {
                id: drive.to_owned(),
                label: if label.is_empty() { drive } else { label }.to_owned(),
                mount: Some(Location::new(format!("file:///{drive}/"))),
                system_path: drive.to_owned(),
                can_disconnect: true,
            })
        })
        .collect())
}

#[cfg(windows)]
fn disconnect_device(device: &RemovableDevice) -> Result<DeviceDisconnectReport, String> {
    run_disconnect(
        "mountvol.exe",
        &[device.system_path.as_str(), "/p"],
        device,
        "mountvol /p",
    )
}

fn run_disconnect(
    executable: &str,
    arguments: &[&str],
    device: &RemovableDevice,
    action: &str,
) -> Result<DeviceDisconnectReport, String> {
    let output = Command::new(executable)
        .args(arguments)
        .output()
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !output.status.success() {
        return Err(format!(
            "{action} failed for {}: {}",
            device.system_path,
            if stderr.is_empty() { stdout } else { stderr }
        ));
    }
    Ok(DeviceDisconnectReport {
        device: device.id.clone(),
        action: action.to_owned(),
        audit: format!(
            "executable={executable} args={arguments:?} status={} output={}",
            output.status,
            if stdout.is_empty() { "<none>" } else { &stdout }
        ),
    })
}

#[cfg(target_os = "linux")]
fn parse_quoted_fields(line: &str) -> std::collections::BTreeMap<String, String> {
    let mut fields = std::collections::BTreeMap::new();
    for field in line.split_whitespace() {
        if let Some((name, value)) = field.split_once('=') {
            fields.insert(name.to_owned(), value.trim_matches('"').to_owned());
        }
    }
    fields
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn macos_mount_parser_only_accepts_external_volume_mounts() {
        let device = parse_macos_mount(
            "/dev/disk4s1 on /Volumes/Backup (apfs, local, nodev, nosuid, journaled)",
        )
        .unwrap();
        assert_eq!(device.id, "/dev/disk4s1");
        assert_eq!(device.label, "Backup");
        assert_eq!(device.mount.unwrap().as_str(), "file:///Volumes/Backup");
        assert!(parse_macos_mount("/dev/disk3s1 on / (apfs, local)").is_none());
    }
}
