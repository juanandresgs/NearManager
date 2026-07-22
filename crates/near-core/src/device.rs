use crate::Location;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovableDevice {
    pub id: String,
    pub label: String,
    pub mount: Option<Location>,
    pub system_path: String,
    pub can_disconnect: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDisconnectReport {
    pub device: String,
    pub action: String,
    pub audit: String,
}

pub trait RemovableDeviceService: Send + Sync {
    /// Lists currently attached removable devices.
    ///
    /// # Errors
    ///
    /// Returns platform discovery, permission, or parsing failures.
    fn list_devices(&self) -> Result<Vec<RemovableDevice>, String>;

    /// Flushes, unmounts, and disconnects one exact device identifier.
    ///
    /// # Errors
    ///
    /// Returns unsupported-device, busy, permission, or platform command failures.
    fn disconnect(&self, id: &str) -> Result<DeviceDisconnectReport, String>;
}
