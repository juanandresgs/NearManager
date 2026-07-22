# Near Trash, Permanent Delete, and Wipe

Near routes all deletion modes through immutable operation plans, the normal confirmation policy, background execution, itemized outcomes, and the operation journal. The UI never removes a resource directly.

## Trash

F8 plans reversible trash. Platform defaults are:

- macOS: `NSFileManager.trashItem(at:resultingItemURL:)`, invoked by a bounded child-helper so the native API runs on its process main thread;
- Linux: the Freedesktop Trash `files` directory plus `.trashinfo` metadata containing the original path and deletion timestamp;
- Windows: the shell Recycle Bin operation through the platform file API bridge.

On macOS, the platform chooses collision-safe destination names without presenting replace or rename decisions. The helper returns the actual resulting Trash path, and Near records that path together with the original source in the execution summary and journal so restoration does not guess the platform's collision suffix. The helper is terminated after 30 seconds and reports an item failure instead of freezing the TUI. Near's restoration record is authoritative; Finder's separate “Put Back” availability is not assumed.

Cross-device trash falls back to copy-then-delete where the platform permits it. The operation preview remains configurable because trash is classified as reversible.

## Restore Last Trash

After a Trash task completes, Files → Restore last Trash creates a new immutable restore plan from
the completed item outcomes. The plan uses each platform-selected Trash location as its source and
the journaled pre-Trash location as its destination. Existing destinations are never replaced
silently: the normal restore preview exposes skip, replace, and collision-safe rename decisions.
Completed restores are removed from the retained restoration set; failed, skipped, cancelled, and
pending items remain available for another restore attempt. Linux restoration also removes the
matching `.trashinfo` sidecar after the filesystem move succeeds.

On macOS, both Trash and Restore execute through bounded native helper modes. Near does not require
direct directory access to the privacy-protected `~/.Trash` folder, and restoration still uses the
exact collision-safe path returned by the native Trash API.

## Permanent Delete

Shift+Delete creates a `Delete` plan with no recovery destination. Files and recursive directories are always classified as destructive and high impact. The preview therefore requires two separate Enter actions: first arm the irreversible operation, then confirm it. This safeguard cannot be disabled by confirmation configuration.

Permanent deletion removes directory trees only after the exact recursive targets are visible in the plan. Outcomes remain journaled per item.

## Wipe

Ctrl+Shift+Delete opens a wipe dialog. The operator chooses 1–7 overwrite passes; alternating zero and one-byte patterns are written across the current length, each pass is synced, and the regular file is then deleted. The pass count is displayed on its own operation-preview line. Wipe is always destructive and high impact, so it uses the same two-step confirmation.

Wipe deliberately rejects directories, symbolic links, non-files, read-only files, and unsupported providers before execution. Cancellation between chunks or passes leaves the file in place but may leave its contents partially overwritten; the item outcome reports that state.

Overwrite-based wiping is **not a guarantee of physical erasure** on SSDs, flash translation layers, snapshots, copy-on-write filesystems, compressed storage, journaled replicas, remote providers, or backups. Cryptographic erase, full-volume secure erase, and platform device-management tools remain outside a file manager's safe abstraction. Near exposes this limitation in the dialog and documentation rather than implying stronger guarantees.
