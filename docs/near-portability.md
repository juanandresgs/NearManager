# Near Portability Contract

Near keeps commands, keymaps, themes, providers, resources, surfaces, workflows, and test interfaces platform-neutral. Operating-system differences live behind terminal, filesystem, configuration-directory, handler, process-launcher, and release adapters.

## Linux

- Configuration follows `$NEAR_CONFIG_HOME`, then `$XDG_CONFIG_HOME/near`, then `~/.config/near`; application data follows `$NEAR_DATA_HOME`, `$XDG_DATA_HOME/near`, or `~/.local/share/near`. `--config-root`, `--data-root`, and `--portable` provide process-local profile redirection without mutating the environment.
- Local file identities preserve arbitrary Unix bytes in `Location`; portable metadata contains size, timestamps, Unix mode, owner IDs, and device/inode identity.
- macOS-only Finder tags, quarantine, and package metadata are absent, not errors or fake values.
- Trash uses the Freedesktop `Trash/files` and `Trash/info` layout with a `.trashinfo` original path and deletion timestamp.
- Default external opening uses structured `xdg-open` arguments. Embedded terminals use `$SHELL` or `/bin/sh` through the native PTY backend.
- Removable-device discovery uses `lsblk`; safe disconnection uses fixed-argument `udisksctl` unmount and power-off commands after exact identity revalidation.
- Native Linux CI executes the complete workspace test and clippy suites.

## Windows

- Configuration and data use `APPDATA`, `LOCALAPPDATA`, and `PROGRAMDATA` platform roots.
- Drive-letter and UNC paths round-trip through the same provider `Location` abstraction using reversible UTF-16LE percent encoding.
- Portable metadata remains unchanged. Windows file attributes, reparse-point state, ACL JSON, and alternate-stream JSON are typed metadata extensions.
- Trash delegates to the Windows Recycle Bin through a structured non-interactive PowerShell invocation. Symlink creation uses file/directory platform APIs, and unsupported Unix mode mutation fails explicitly.
- Embedded terminals use `COMSPEC` through the native `conpty` adapter by default. `NEAR_SHELL` can select PowerShell or another shell; a Windows-only test verifies interactive input, output, resize, and exit status through ConPTY. Unix platforms retain the `portable-pty` adapter.
- Removable-drive discovery uses a fixed PowerShell query; safe disconnection uses `mountvol.exe <drive> /p` after exact identity revalidation.
- Native Windows CI executes the complete workspace test and clippy suites. Cross-target compilation from macOS additionally verifies that no Unix-only public API leaks into shared crates or applications.

## Platform Expectations

The same scripted model workflows run on every CI platform. Unix PTY integration uses the native `$SHELL` or `/bin/sh` and conditionally exercises Vim and SSH when installed; Seatbelt tests remain macOS-specific. ConPTY, UTF-16 path, and Windows metadata tests are Windows-specific. Capability queries and typed metadata extensions report what a provider can do rather than manufacturing platform errors.

ZIP browsing, extraction, creation, and update use the shared Rust archive provider on every supported operating system. Archive locations remain provider-neutral; only the source and destination local-path adapter varies by platform. Temporary-file replacement uses atomic overwrite where available and a recovery backup where Windows requires replacement to be staged.

On macOS, removable devices are limited to mounted `/dev/disk*` resources below `/Volumes`; safe disconnection uses `/usr/sbin/diskutil eject` with structured arguments and exact identity revalidation.

Linux packages are produced as compressed release archives in the current release workflow. Distribution-specific packages may wrap those binaries, configuration examples, and license files. Packagers should install user configuration under XDG paths, system configuration under `/etc/near`, and must not replace the Freedesktop Trash implementation with permanent deletion.

## Windows Compatibility Verification

The x86-64 GNU target links every workspace test executable with MinGW before native CI. A local Wine 9 compatibility run additionally verifies the shared application harnesses, CLI child-process paths, platform roots, reversible drive/UNC locations, file attributes, reparse state, ACL/alternate-stream field-local handling, provider workflows, semantic rendering, and Windows path navigation.

Wine does not implement the Windows ConPTY output path or ship Windows PowerShell for Recycle Bin automation, so those two facilities remain covered only by the native `windows-latest` CI job. Compatibility-layer success is supplemental evidence and is never substituted for the native Windows gate.
