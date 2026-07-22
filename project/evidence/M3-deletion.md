# M3 Deletion Evidence

Date: 2026-06-23

## Implemented Slice

- F8 uses the platform trash path or service through a reversible operation plan.
- Shift+Delete plans permanent file or recursive-directory deletion with no recovery policy.
- Ctrl+Shift+Delete accepts 1–7 overwrite passes for writable regular files before deletion.
- Delete and wipe are always destructive, high-impact plans requiring arm and confirm actions.
- Wipe rejects directories, links, read-only files, invalid pass counts, and unsupported providers.
- Operation previews expose the wipe pass count and all outcomes remain journaled.

## Automated Evidence

- Local filesystem tests prove platform-capability reporting, high-impact delete planning, wipe pass validation, regular-file restrictions, journaled execution, and removal.
- Workspace tests prove both key bindings, visible SSD/COW limitations, dedicated wipe-pass preview, and two-step confirmation before mutation.
- Operation-preview tests prove high-impact plans cannot execute after only one confirmation action.

## Requirement Status

`FAR-OPS-003` is verified by native trash integration, mandatory-confirmation permanent deletion, executable wipe policy, and explicit physical-storage limitations.
