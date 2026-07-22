# Near Removable-Device Panels

Near exposes attached removable media through the provider-neutral `near.removable-devices` provider at `device://attached`. Device rows are virtual resources, while their metadata preserves the exact platform identifier, system path, optional mount location, and whether safe disconnection is currently supported.

## Resource Contract

- `near.device.id` is the stable identifier accepted by the platform service.
- `near.device.system-path` is the native device or volume path used for diagnostics.
- `near.device.mount` is present only when the device has a mounted filesystem location.
- `near.device.disconnectable` reports the current platform capability.
- `device.disconnect` is advertised only for a resource that the latest platform listing marks disconnectable.

The workspace merges current-resource provider capabilities into command availability. `near.device.disconnect` therefore remains unavailable for fixed disks, stale rows, unsupported platforms, and arbitrary resources that merely resemble devices.

## Safe Disconnection

`PlatformRemovableDeviceService` refreshes the platform device list and requires an exact identifier match before launching any native command. It never passes device values through a shell.

- macOS lists mounted `/dev/disk*` devices below `/Volumes` and runs `/usr/sbin/diskutil eject <system-path>`.
- Linux lists removable block devices through `lsblk` and runs fixed-argument `udisksctl unmount` followed by `udisksctl power-off`.
- Windows lists removable logical disks through a fixed PowerShell discovery script and runs `mountvol.exe <drive> /p`.

Unsupported discovery, vanished devices, non-removable resources, and unsuccessful native commands return explicit errors without claiming success.

## Diagnostics

Every disconnect attempt has a provider-domain correlation beneath the initiating command. Successful events retain the exact device ID, action description, executable, arguments, exit status, and bounded process output. Failures retain the device ID and error. The same audit text is appended to configuration diagnostics so a user can review the native action after the panel refreshes.

