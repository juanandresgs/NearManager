# Near Layered Configuration

Near configuration is a versioned merge system rather than a collection of unrelated file lookups. `near-config` resolves TOML documents through one deterministic precedence model and records the winning source position for every effective leaf value.

## Precedence

Layers always resolve in this order, regardless of discovery or insertion order:

1. built-in defaults;
2. platform defaults;
3. installed plugin defaults, ordered by priority and source ID;
4. user configuration;
5. trusted workspace configuration;
6. command-line overrides.

Tables merge recursively. Scalars and arrays replace the earlier value. This makes the result predictable and prevents accidental concatenation semantics from differing between themes, keymaps, handlers, and application-specific documents.

Workspace files are rejected unless their layer is explicitly trusted. In `near-fm`, trust is granted by `--trust-workspace` or `NEAR_TRUST_WORKSPACE=1`.

## Provenance and Diagnostics

`EffectiveConfig` stores a `ConfigOrigin` for every leaf or replaced array. Origins contain layer kind, source path, line, column, and migration source version when applicable.

Parse, schema, migration, trust, and typed-value errors report:

- source file;
- line and column;
- field when known;
- error category and correction-oriented message.

The Far Settings command renders a searchable category catalog and the effective origins for keymap, theme, confirmation, handler, macro, panel-mode, editor, history, highlighting, user-menu, description, and filter documents. Its advanced topic indexes every field and provenance line, including settings without a dedicated live dialog.

## Versioning and Migration

Every document declares `schema = 1`. A `ConfigEngine` may register explicit one-way `ConfigMigration` steps. Old documents are advanced step by step, and generated values retain the old schema version in their origin metadata. Newer unsupported schemas fail before merge.

Migrations never run implicitly without a registered step. This prevents old fields from being silently reinterpreted.

## Atomic Reload

`ConfigManager` owns the last valid `EffectiveConfig`. Reload resolves and validates a complete candidate first. Success atomically replaces the current document; failure records the diagnostic and returns `retained_last_valid = true` without modifying active values.

Applications can use this primitive to watch files without accepting a partially valid candidate. `near-fm` now transactionally reloads keymap, interface, confirmation, panel-mode, viewer, editor, history, and shell documents through the running workspace. Keymap candidates preserve the complete binding tree and replace the runtime resolver only after full validation. File watching and the remaining configuration domains still require `ConfigManager` integration.

## macOS Far Integration

For each shipped document, `near-fm` discovers:

- the embedded default;
- `/Library/Application Support/Near/<document>`;
- sorted plugin defaults below the user config root;
- the user document;
- `.near/<document>` when trusted;
- an explicit CLI or environment path.

The user root honors `NEAR_CONFIG_HOME`, then `XDG_CONFIG_HOME/near`, then `~/Library/Application Support/near`.

Supported CLI overrides are `--keymap`, `--theme`, `--confirmations`, `--handlers`, `--macros`, `--panel-modes`, `--editor`, `--history`, `--interface`, `--highlighting`, `--user-menu`, `--descriptions`, and `--filters`. Environment equivalents remain available for automation, including `NEAR_PANEL_MODES`, `NEAR_EDITOR`, `NEAR_HISTORY`, `NEAR_INTERFACE`, `NEAR_HIGHLIGHTING`, `NEAR_USER_MENU`, `NEAR_DESCRIPTIONS`, and `NEAR_FILTERS`.

## Panel Modes

`panel-modes.toml` assigns independent `defaults.left` and `defaults.right` mode IDs. The built-in `compact`, `medium`, `full`, and `metadata` modes always exist; custom `[[modes]]` entries can replace a built-in ID or add another menu entry.

`interface.toml` controls the status row, function keybar, tree indentation, menu wrapping, dialog focus wrapping, and command-line completion. All fields apply live after validation; hiding the keybar also disables its mouse hit target.

`editor.toml` controls editor interaction defaults. `persistent_blocks = true` keeps stream or column blocks active after copy and ordinary cursor movement; the runtime `near.editor.toggle-persistent-blocks` command can change the active session immediately.

`history.toml` sets independent unlocked-entry retention limits for command, folder, and viewed/edited resource histories. Locked entries do not count against these limits. Command, folder, view, and edit histories are persistent, searchable, clearable, and retain unavailable provider entries with their latest diagnostic.

`highlighting.toml` defines ordered panel-decoration rules. Each rule has a stable ID, editable priority, optional parent, provider-neutral metadata predicate, semantic role, one-cell mark, and numeric sort group. Parent and child predicates compose, while child role, mark, and sort-group values override inherited values. Invalid predicates, unknown parents, duplicate IDs, empty rules, and inheritance cycles fail before startup. Shift+F11 toggles sort groups; the F9 Highlighting report shows the effective priority order and inherited predicate count.

`user-menu.toml` defines ordered global and local typed automation entries. F2 opens the global scope and Shift+F2 opens the local scope. Structured argv is the default; explicit shell scripts are visibly labeled and receive quoted metasymbol expansion. See `docs/near-user-menus.md`.

`descriptions.toml` defines ordered file and folder sidecar names, explicit UTF-8/BOM/Latin-1 behavior, update policy, and catalog visibility. Ctrl+Z edits selected/current descriptions; F9 exposes folder-description viewing and editing. See `docs/near-descriptions.md`.

`filters.toml` defines named mask groups and reusable provider-neutral panel predicates. Ctrl+Shift+F or F9 opens the focused panel's filter menu; active state is independent per panel and visible in its border. See `docs/near-filters.md`.

Each `[[modes.columns]]` chooses `name`, `extension`, `size`, `modified`, `created`, `accessed`, `kind`, `owner`, `permissions`, or `description`. Omit `width` to consume remaining space, or set a terminal-cell width. `alignment` accepts `left`, `center`, or `right`. Invalid schemas, duplicate IDs, empty layouts, and unknown defaults fail before the workspace starts.
