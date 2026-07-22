# M3 Editor Save As and Recovery Evidence

Date: 2026-06-23

## Implemented Slice

- `F2` saves with the active encoding, BOM, and EOL format. `Shift+F2` and `Ctrl+Shift+S` open provider-neutral Save As.
- Save As selects a registered provider ID and location plus UTF-8, UTF-16LE, UTF-16BE, or Latin-1 encoding, optional BOM, LF/CRLF/CR line endings, and explicit create/replace approval.
- Latin-1 output detects unrepresentable Unicode and refuses to write until lossy replacement is explicitly confirmed.
- Normal saves retain the opened `ResourceVersion`. A conflict exposes reload, read-only local/external comparison, and keep-local overwrite choices.
- Reload replaces the editor buffer and version, comparison preserves both versions, and keep-local deliberately removes the stale-version precondition.

## Automated Evidence

- Editor tests verify UTF-16BE+BOM+CRLF Save As bytes and identity replacement.
- Editor tests prove lossy Latin-1 output does not mutate the provider before confirmation.
- Editor tests exercise compare, reload, and keep-local behavior against a provider that changes after open.
- A filesystem workspace test routes `F2`, renders the conflict menu and comparison, reloads the external version, and then overwrites a second external change with the kept local buffer.
- A filesystem workspace test proves a Latin-1 target is not created before lossy confirmation and contains explicit replacement bytes after confirmation.

## Requirement Status

- `REQ-EDIT-001` remains verified with expanded format and conflict-recovery acceptance criteria.
- `FAR-EDIT-004` is verified.
