# Settings Catalog

Near's **Options → Settings** command opens a full-screen searchable typed settings surface. The current catalog exposes runtime key-sequence policy plus every safely editable field in the shipped `interface.toml`, `confirmations.toml`, `viewer.toml`, `editor.toml`, `history.toml`, `panel-modes.toml`, and `shell.toml` documents. Interface policy covers the system status row, context keybar, tree indentation, menu boundaries, dialog focus cycling, and command-line completion from history and focused-panel names. The remaining documents cover sequence timeout and pending-sequence display, lower-impact preview policy, viewer encoding and opening policy, editor tab and opening policy, independent left/right panel defaults, independent command/folder/resource retention limits, and native shell program, mode, arguments, close policy, and environment inheritance. Destructive, privileged, and high-impact previews remain mandatory and cannot be weakened through settings.

- `Enter` toggles Boolean values.
- `F4` opens the generic value editor for Boolean, integer, string, and string-list settings.
- `F5` reloads externally edited settings documents as one validated runtime candidate.
- `Delete` stages the descriptor's declared default.
- `Home`, `End`, `PageUp`, and `PageDown` navigate the filtered catalog.
- Typing filters titles, descriptions, identifiers, categories, and provenance text.

Each row displays its exact winning layer and source before the title, value, and description. Complete-document persistence and successful external reload update provenance for every field in that document together. Live settings affect the running workspace immediately; new-surface settings affect viewers, editors, or shell sessions opened after the change. Invalid values and persistence failures are rejected without changing the effective runtime setting.

`near-fm` atomically replaces the complete versioned keymap, interface, confirmation, viewer, editor, history, panel-mode, or shell document in the active writable configuration layer. Keymap edits preserve all contexts and bindings, validate the complete replacement, and swap the runtime keymap without restarting. Portable profiles and explicit document overrides therefore persist through the same path from which they are loaded. External reload parses every available document before changing runtime state; one invalid document retains the complete last-valid settings set. Embedders without a document store retain explicit session-scoped behavior.

`prefer_physical_keys = true` is rejected rather than silently ignored because the current terminal event contract does not expose physical key identity. Modifier press, repeat, and release remain supported independently through enhanced keyboard mode.

Advanced descriptors are hidden by default, disclosed explicitly with `F6`, and remain searchable
while hidden. Every currently shipped runtime settings document has typed descriptors, provenance,
validation, persistence, reset, reload, and declared live/new-surface scope. Additional future
settings domains must enter through the same descriptor and coordinator contracts rather than a
domain-specific dialog.
