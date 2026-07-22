# Near: Universal Keyboard-First TUI Platform Blueprint

## Executive Decision

Near should not be implemented as “Far Manager for macOS.” It should be a reusable application platform whose first reference application is a two-panel file workspace.

The platform's durable abstraction is:

> **semantic commands operating on contextual resources, presented through composable terminal surfaces, and invoked through user-defined keymaps.**

Far Manager proves the interaction model, but Far-specific concepts must be generalized:

| Far concept | Near abstraction |
|---|---|
| Active/passive panel | Focused and peer `ResourceView` |
| File panel | `CollectionSurface<Item>` backed by a `Provider` |
| Function-key bar | Contextual command hints generated from the keymap |
| Main/user/plugin menus | Filtered projections of one `CommandRegistry` |
| File association | Typed `HandlerRule` selected from resource metadata |
| Metasymbol | Structured command-template variable |
| Plugin panel | Provider-backed application surface |
| Macro | Recorded or scripted command invocation sequence |
| Command line | PTY-backed shell surface with shared context injection |
| Viewer/editor | Resource handlers that can be embedded or launched externally |

This makes the Far aesthetic and workflow language reusable for file management, process management, archive browsing, Git workspaces, remote systems, databases, package managers, logs, and other terminal applications.

## Product Thesis

Near should feel like a coherent terminal operating environment rather than a collection of unrelated CLI programs:

- Every application shares the same colors, borders, menus, dialogs, key grammar, command palette, help system, history, task UI, and configuration paths.
- Every action has a stable semantic command identifier independent of its current key binding.
- Every visible key hint is generated from the active keymap rather than hard-coded.
- Every resource can expose capabilities such as view, edit, copy, delete, inspect, search, execute, mount, or compare.
- Every long-running action is a task with progress, cancellation, diagnostics, and an inspectable result.
- External tools such as Vim, Neovim, HIEW-like hex editors, `less`, `rg`, `fd`, `git`, and ordinary zsh commands are first-class participants.

## Design Principles

### Preserve Far's strengths

1. **Keyboard certainty** — common actions are one key or one modifier away.
2. **Stable spatial model** — the focused surface and its peer define source, destination, and comparison context.
3. **Visible affordances** — menus, key bars, status lines, and help reveal what can happen now.
4. **Current item plus explicit selection** — commands operate predictably without requiring selection for single-item work.
5. **Terminal continuity** — shell execution does not require abandoning the workspace.
6. **Low ceremony** — opening, viewing, editing, copying, searching, and applying commands remain immediate.
7. **Progressive extensibility** — simple configuration handles common cases; plugins handle new domains.

### Improve on Far's limitations

1. Separate command semantics from keyboard encoding.
2. Use portable resource providers rather than drive-letter assumptions.
3. Make destructive operations recoverable where the platform permits.
4. Expose asynchronous operations consistently instead of blocking the interface.
5. Give plugins explicit capabilities and stable versioned interfaces.
6. Treat Unicode, terminal capability variation, and accessibility as foundational concerns.
7. Preserve compatibility with external tools instead of duplicating mature editors and shells.

### Non-goals

- Building a full terminal emulator before the file workspace is useful.
- Reimplementing Vim, Neovim, or a complete IDE in the core platform.
- Making mouse interaction the primary navigation model.
- Exposing Ratatui or Crossterm types as the public application/plugin API.
- Loading arbitrary native dynamic libraries as the long-term plugin contract.
- Guaranteeing pixel-identical Far rendering; Near preserves grammar and rhythm, not Windows console artifacts.

## Architecture Overview

```text
┌──────────────────────────────── Applications ────────────────────────────────┐
│ near-fm  near-view  near-edit  near-hex  near-proc  near-pick  third parties │
└───────────────────────────────┬───────────────────────────────────────────────┘
                                │ semantic application API
┌───────────────────────────────▼───────────────────────────────────────────────┐
│ near-shell: workspace, panes, overlays, menus, help, keybar, command palette │
├───────────────────────────────────────────────────────────────────────────────┤
│ near-runtime: model/update/effect loop, commands, contexts, tasks, history   │
├───────────────────────────────────────────────────────────────────────────────┤
│ near-ui: semantic surfaces, layout, focus, dialog and collection primitives │
├───────────────────────────────────────────────────────────────────────────────┤
│ near-theme       near-keymap       near-config       near-plugin-host        │
├───────────────────────────────────────────────────────────────────────────────┤
│ near-resource    near-fs    near-pty    near-search    near-process           │
├───────────────────────────────────────────────────────────────────────────────┤
│ near-terminal: capabilities, normalized input, renderer, clipboard, restore │
├───────────────────────────────────────────────────────────────────────────────┤
│ Ratatui Core / Crossterm / platform adapters / Wasmtime / Tokio              │
└───────────────────────────────────────────────────────────────────────────────┘
```

Applications depend on the Near API. Near depends on Ratatui. Applications and plugins should not directly depend on Ratatui data structures because doing so would freeze rendering details into the ecosystem.

## Core Domain Model

### Stable identifiers

All extensible concepts use namespaced string identifiers:

```rust
pub struct CommandId(pub Arc<str>);    // "near.fs.copy"
pub struct ContextId(pub Arc<str>);    // "workspace.panel.file"
pub struct RoleId(pub Arc<str>);       // "panel.item.selected.focused"
pub struct ProviderId(pub Arc<str>);   // "near.local-fs"
pub struct HandlerId(pub Arc<str>);    // "near.external.nvim"
pub struct CapabilityId(pub Arc<str>); // "resource.write"
```

Names are configuration and plugin ABI. Rust enums may wrap built-ins internally, but serialized interfaces must remain open-ended.

### Resources and locations

```rust
pub trait ResourceProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn schemes(&self) -> &[&str];
    async fn list(&self, location: &Location, request: ListRequest)
        -> Result<ListPage, ProviderError>;
    async fn stat(&self, resource: &ResourceRef)
        -> Result<ResourceMetadata, ProviderError>;
    async fn open(&self, resource: &ResourceRef, mode: OpenMode)
        -> Result<ResourceStream, ProviderError>;
    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet;
}
```

`Location` is URI-like and provider-neutral: `file:///Users/alex`, `archive+file:///tmp/a.zip!/src`, `ssh://host/etc`, `proc://local`, or `git://repo/status`.

`ResourceMetadata` has a portable core and typed extension fields. The core includes name, kind, byte size, timestamps, permissions summary, owner, MIME hint, hidden state, link target, and stable identity where available. Platform metadata such as macOS extended attributes, APFS clone support, Finder tags, Windows attributes, or Linux capabilities belongs in extensions.

### Focus, cursor, and selection

Each collection surface owns:

- A current location.
- A cursor item.
- An ordered explicit selection set.
- Sort, filter, view, and column state.
- Navigation history.
- A provider-defined continuation token for large or remote listings.

Commands receive an immutable `ActionContext`:

```rust
pub struct ActionContext {
    pub focused_surface: SurfaceId,
    pub peer_surface: Option<SurfaceId>,
    pub current: Option<ResourceRef>,
    pub selected: Vec<ResourceRef>,
    pub location: Option<Location>,
    pub peer_location: Option<Location>,
    pub workspace: WorkspaceId,
    pub capabilities: CapabilitySet,
}
```

The canonical target rule is: explicit selection when non-empty, otherwise current item. Commands can override this only when their metadata makes the behavior visible.

### Commands

A command is the system's unit of behavior, discoverability, automation, and testing:

```rust
pub trait Command: Send + Sync {
    fn descriptor(&self) -> &CommandDescriptor;
    fn availability(&self, context: &ActionContext) -> Availability;
    fn invoke(&self, context: ActionContext, args: Value) -> CommandFuture;
}

pub struct CommandDescriptor {
    pub id: CommandId,
    pub title: LocalizedText,
    pub description: LocalizedText,
    pub category: Vec<String>,
    pub argument_schema: Option<JsonSchema>,
    pub safety: SafetyClass,
    pub repeatability: Repeatability,
    pub undo: UndoSupport,
}
```

Command results produce effects rather than mutating UI objects directly:

```rust
pub enum Effect {
    Dispatch(CommandInvocation),
    StartTask(TaskSpec),
    OpenSurface(SurfaceSpec),
    OpenOverlay(OverlaySpec),
    Navigate(SurfaceId, Location),
    Notify(Notification),
    RequestRender,
    Quit(QuitReason),
}
```

This preserves deterministic tests and allows macros, menus, keymaps, plugins, RPC, and command palettes to invoke the same behavior.

## Runtime and Event Loop

Use an Elm/Redux-like single-owner application model without forcing applications to adopt a web framework vocabulary:

```text
terminal input ─┐
filesystem event│
task progress   ├─> Event -> update(Model) -> Effects -> services
PTY output      │                         └-> render semantic scene
plugin event   ─┘
```

- The UI thread owns model mutation, focus, overlays, and render scheduling.
- Tokio tasks perform filesystem, search, hashing, preview, plugin, and PTY I/O.
- Bounded channels apply backpressure; task events carry stable task and generation IDs.
- Stale listing/search results are discarded when their generation no longer matches the surface.
- Rendering is event-driven with coalescing and a maximum refresh rate, not an unconditional busy loop.
- A panic-safe terminal guard must restore raw mode, keyboard protocol state, mouse mode, cursor visibility, and alternate screen.

## Terminal Layer

### Recommended foundation

- **Ratatui 0.30+** for buffers, layout, style primitives, and test backend.
- **Crossterm** for cross-platform terminal I/O and keyboard enhancement support.
- Request Kitty keyboard protocol disambiguation and event types where supported, then degrade to legacy encoding.
- Maintain a `TerminalCapabilities` snapshot for colors, Unicode width policy, keyboard protocol, mouse, focus events, synchronized updates, clipboard, image protocols, and terminal identity.

### Why wrap Ratatui

Ratatui intentionally leaves the event loop and application state to the application. Near needs stronger conventions: focus routing, overlays, semantic roles, command lookup, hint generation, async task integration, and terminal restoration. Near's public render API should describe semantic surfaces while an adapter translates them to Ratatui buffers.

### Rendering contract

Every renderable element produces cells using semantic roles rather than concrete colors:

```rust
pub struct CellStyle {
    pub role: RoleId,
    pub modifiers: StyleModifiers,
}
```

The theme resolver maps a role through state fallbacks:

```text
panel.item.directory.selected.focused
panel.item.selected.focused
panel.item.focused
panel.item
text
```

This enables Far-like four-state item colors without forcing every application to implement color matrices.

### Responsive layout tiers

- **Compact**: one primary surface, abbreviated key bar, overlays consume most of the screen.
- **Standard**: two peer surfaces, command line, status, full key bar.
- **Wide**: optional preview/inspector or task surface in addition to peer surfaces.

Applications declare preferred layout relationships; the shell chooses the tier from terminal dimensions and minimum surface constraints.

## Input and Keymap Language

The keymap is a first-class runtime, not a static map from key to callback.

### Normalized input

```rust
pub struct KeyStroke {
    pub logical: Key,
    pub physical: Option<PhysicalKey>,
    pub text: Option<String>,
    pub modifiers: Modifiers,
    pub kind: KeyKind,
}
```

The platform retains both logical and physical keys when the terminal protocol provides them. Key bindings normally match logical keys; layout-independent bindings can request the physical key.

### Context stack

Bindings resolve from the most specific active context outward:

```text
overlay.confirm.delete
dialog
workspace.panel.file
workspace.panel
workspace
global
```

Each context may prepend, replace, remove, or inherit bindings. A binding can include an availability expression based on capabilities and state, but complex business logic stays in command availability.

### Binding forms

- Single strokes: `F5`, `Ctrl+R`, `Alt+F1`.
- Sequences: `g g`, `Space f f`.
- Chords where the terminal can report them reliably.
- Command arrays for atomic macro-like composition.
- Parameterized commands.
- Text insertion only in surfaces that explicitly accept text.

The resolver is a trie with an explicit sequence timeout. Pending sequences are displayed in the status area. Ambiguous prefixes can either wait, execute immediately, or require an explicit terminator according to binding metadata.

### Discoverability

The runtime generates:

- Contextual function-key bar entries.
- A searchable command palette.
- A “which-key” continuation overlay for partial sequences.
- Conflict diagnostics with origin file and context.
- Help pages listing effective bindings rather than defaults.

See `specs/keymap.toml` for the proposed syntax.

## Theme System

Themes are data-only, hot-reloadable, and shared across the suite.

### Theme contents

- Semantic role styles.
- Border glyph sets and fallback ASCII glyphs.
- Spacing and density tokens.
- Status severity styles.
- File/resource classification rules.
- Optional 16-color, 256-color, and truecolor variants.

Themes do not control layout logic or command behavior. Applications may introduce namespaced roles, but must provide fallbacks to core roles.

### Far compatibility preset

Ship `far-classic` as a default preset:

- Blue panel background.
- Cyan/white borders and headings.
- Green/cyan selection and cursor distinctions.
- Gray dialogs with focused control emphasis.
- High-contrast function-key labels.
- Single-cell box drawing with ASCII fallback.

Also ship an accessible high-contrast theme and a terminal-native theme that respects the user's palette.

See `specs/theme.toml` for a concrete role hierarchy.

## Surface and Widget Library

The reusable library should provide behavior-rich surfaces rather than a large catalog of decorative widgets.

### Essential surfaces

1. `CollectionSurface` — virtualized items, cursor, selection, sorting, filtering, columns, incremental find.
2. `TreeSurface` — lazy hierarchical navigation with stable expansion state.
3. `ViewerSurface` — text, hex, wrapping, search, code-page handling, bookmarks.
4. `EditorHostSurface` — minimal built-in text editing plus external editor handoff.
5. `TerminalSurface` — PTY grid, shell process, scrollback, selection, input modes.
6. `InspectorSurface` — metadata or quick preview for the peer cursor.
7. `TaskSurface` — progress, logs, cancellation, retry, and completed task history.
8. `MenuSurface` — nested command lists with hotkeys and filtering.
9. `DialogSurface` — forms, validation, focus traversal, default/cancel actions.
10. `HelpSurface` — generated command, keymap, and application documentation.

### Surface protocol

```rust
pub trait Surface {
    fn id(&self) -> SurfaceId;
    fn contexts(&self) -> SmallVec<[ContextId; 4]>;
    fn capabilities(&self) -> CapabilitySet;
    fn update(&mut self, event: &Event, context: &mut UpdateContext) -> UpdateResult;
    fn scene(&self, area: Rect, context: &RenderContext) -> Scene;
    fn cursor(&self) -> Option<CursorRequest>;
}
```

Focus routing and command dispatch belong to the shell/runtime, not to individual surfaces.

## Filesystem and Operation Engine

### Local filesystem provider

The first provider targets macOS but exposes portable semantics:

- Async/paged directory enumeration with metadata hydration separated from name listing.
- Symlink-aware navigation and operations.
- Hidden-file policy distinct from `.gitignore` search policy.
- Birth, modification, access, and change timestamps where available.
- POSIX mode, owner/group, ACL summary, extended attributes, Finder tags, and quarantine metadata on macOS.
- Stable identity from device/inode where available, never from display path alone.

### File operations

Copy, move, rename, link, trash, delete, wipe, mkdir, touch, and attribute changes are `OperationPlan`s:

```rust
pub struct OperationPlan {
    pub id: OperationId,
    pub kind: OperationKind,
    pub sources: Vec<ResourceRef>,
    pub destination: Option<Location>,
    pub conflict_policy: ConflictPolicy,
    pub metadata_policy: MetadataPolicy,
    pub verification: VerificationPolicy,
    pub recovery: RecoveryPolicy,
}
```

Planning is separate from execution. The preview dialog displays resolved source count, destination, expected conflicts, cross-device behavior, symlink policy, and recoverability.

### Safety classes

- `ReadOnly` — no confirmation.
- `Reversible` — may execute directly with undo record or trash semantics.
- `Confirmable` — confirmation based on user policy and scope.
- `Destructive` — explicit confirmation with a typed or held-key safeguard for high-impact operations.
- `Privileged` — launch through a narrow platform helper only when required.

Near should prefer macOS Trash over deletion, APFS clone/copyfile capabilities where safe, and atomic rename on the same filesystem. It must report when a move becomes copy-plus-delete.

### Operation journal

Maintain an append-only local journal for plans, progress, conflict decisions, results, and undo metadata. Backends may report an execution-time target override when a platform API chooses the final destination name; the completed item records that exact location for restoration. The journal is not a filesystem transaction guarantee; it is evidence for recovery, retry, audit, and user-facing history.

## Search, Filter, and Selection Language

Use one predicate model across panels, recursive search, operation scopes, and plugin collections:

```text
kind == file and ext in ["rs", "toml"] and size < 1MiB and modified < 7d
```

The first implementation can expose structured dialog fields and compile them to an AST. A textual query language can follow after the AST stabilizes.

Recommended building blocks:

- `ignore::WalkBuilder` for recursive traversal respecting `.ignore` and Git ignore rules when requested.
- `globset` for compiled glob groups.
- `regex` for content and metadata expressions.
- `nucleo-matcher` for interactive fuzzy matching.
- `notify` for change events, with polling fallback for filesystems that do not emit reliable events.

Panel filtering must not silently inherit Git ignore behavior. Search presets should make ignore policy explicit.

## Shell and External Tool Integration

### Two execution modes

1. **Suspend-and-run** — restore the terminal, run an interactive child attached to the original terminal, then resume and redraw Near. This is the reliable MVP path for Vim, Neovim, HIEW-like tools, SSH, and arbitrary TUI programs.
2. **Embedded PTY** — host zsh or another program inside a `TerminalSurface` using `portable-pty` plus a terminal parser such as `vt100`. This is a later milestone because correct keyboard, resize, OSC, mouse, alternate-screen, clipboard, and nested-TUI behavior is substantial.

Do not delay the file manager for embedded PTY support. Suspend-and-run preserves the most important external-editor workflow with far less risk.

### zsh behavior

- Default shell resolution follows `$SHELL`, then platform account data, then `/bin/zsh` on macOS.
- Login/interactive flags are explicit configuration; Near must not guess that every command needs `-l` or `-i`.
- Command templates execute as argv by default, not shell strings.
- Shell evaluation is opt-in and visibly marked because quoting and injection behavior differ.
- Export structured context through environment variables and temporary JSON when invoking external tools.
- Support shell-side directory synchronization through OSC 7 in embedded sessions and an optional zsh integration script.

### External handlers

Handler rules select internal or external tools by resource predicates and requested action:

```text
action=view + mime=text/*       -> near.viewer
action=edit + env.NVIM exists   -> external:nvim
action=inspect + mime=application/octet-stream -> external:hexyl
```

Arguments are arrays containing typed template expressions, not interpolated shell text.

## Configuration Model

Use TOML for user-authored static configuration because it is familiar in Rust tooling, supports comments, and has mature parsers. Keep generated state out of config files.

```text
~/.config/near/
├── config.toml
├── keymap.toml
├── theme.toml
├── handlers.toml
├── macros.toml
└── plugins/

~/Library/Application Support/near/  # macOS state, cache, history, journal
```

Honor XDG paths on Unix when configured. On macOS, use platform-appropriate state/cache directories while accepting XDG overrides for CLI users.

Configuration loads in layers:

1. Built-in defaults.
2. Platform defaults.
3. Installed theme/plugin defaults.
4. User configuration.
5. Workspace-local trusted configuration.
6. Command-line overrides.

Every effective value records its origin for diagnostics. Workspace-local commands and plugins require explicit trust.

## Extension Model

### Extension tiers

1. **Configuration** — themes, keymaps, handler rules, menus, command templates.
2. **Process extensions** — JSON-RPC or message-framed child processes; easiest multi-language integration and failure isolation.
3. **WebAssembly components** — capability-controlled in-process providers, commands, inspectors, and metadata extensions using WIT contracts.
4. **Native Rust integration** — compile-time crates for trusted first-party functionality, not the public plugin ABI.

### Why WebAssembly components

The Component Model provides typed cross-language interfaces through WIT, and Wasmtime denies ambient system resources unless the host grants them. This matches Near's need for versioned APIs and explicit filesystem/process/network capabilities better than arbitrary native libraries.

WASI and the Component Model continue to evolve, so Near should not make plugins a phase-one dependency. Stabilize command, resource, and surface contracts in Rust first; expose a deliberately smaller WIT API after real applications validate them.

See `specs/plugin.wit` for a draft command/provider boundary.

### Plugin capabilities

Possible grants include:

- Read selected resources.
- Read or write configured locations.
- Spawn declared programs.
- Access network hosts or schemes.
- Register commands, handlers, roles, providers, or inspectors.
- Store plugin-private configuration/state.
- Post notifications or launch tasks.

Plugins never receive the whole application model or raw terminal output.

## Proposed Cargo Workspace

```text
near/
├── Cargo.toml
├── crates/
│   ├── near-core/           # IDs, values, errors, capabilities
│   ├── near-terminal/       # terminal session, input normalization, capability probing
│   ├── near-render/         # semantic scene and Ratatui adapter
│   ├── near-theme/          # role resolution and theme loading
│   ├── near-keymap/         # parser, trie resolver, contexts, diagnostics
│   ├── near-command/        # registry, descriptors, invocation, availability
│   ├── near-runtime/        # event/update/effect loop and task coordination
│   ├── near-ui/             # reusable surfaces and focus/layout primitives
│   ├── near-shell/          # application chrome and workspace composition
│   ├── near-resource/       # provider and resource contracts
│   ├── near-fs/             # local filesystem provider and operation planner
│   ├── near-search/         # predicate AST, traversal, content search
│   ├── near-archive/        # archive provider and mutation plans
│   ├── near-sftp/           # secure remote provider and transfer plans
│   ├── near-pty/            # external execution and embedded PTY
│   ├── near-plugin-api/     # versioned host-facing Rust contracts
│   ├── near-plugin-host/    # process and Wasm hosts
│   ├── near-testkit/        # fake terminal, providers, event scripts, snapshots
│   └── near-app/            # batteries-included application builder facade
├── apps/
│   ├── near-fm/
│   ├── near-view/
│   ├── near-proc/
│   └── near-demo/
├── themes/
├── wit/
└── docs/
```

Avoid creating all crates on day one. Begin with five physical crates—`near-core`, `near-terminal`, `near-ui`, `near-fs`, and `near-fm`—while enforcing the logical boundaries above. Split crates when compile time, ownership, or API publication justifies it.

## Reference Application Suite

### `near-fm`

The primary validation app:

- Dual local/provider panels.
- File operations and operation task queue.
- Quick view and metadata inspector.
- Search results as a provider-backed panel.
- Command line and suspend-to-external-tool workflow.
- Menus, histories, folder bookmarks, filters, and configurable columns.

### `near-view`

Standalone/embeddable text and hex viewer using the same commands and theme. It validates that viewer behavior is not coupled to the file manager.

### `near-proc`

Process collection and task inspector. It validates non-filesystem providers and privileged actions.

### `near-pick`

Embeddable fuzzy resource picker that can print selected paths/URIs to stdout. It validates composability with shell pipelines.

### Later applications

- `near-git` status, tree, commit, and diff resources.
- Additional archive formats through `near-archive` provider adapters.
- Additional remote protocols through the provider and operation contracts proven by `near-sftp`.
- `near-hex` deeper binary inspection if external tools prove insufficient.
- `near-db` database object and query workspace.

## Public Application API

The ergonomic end state should resemble:

```rust
fn main() -> near::Result<()> {
    near::App::builder("acme.logs")
        .title("Log Workspace")
        .theme_from_user_config()
        .commands(log_commands())
        .provider(LogProvider::new())
        .workspace(|workspace| {
            workspace.peer_collections("services", "entries")
                .inspector(LogInspector::new())
                .command_line(false)
        })
        .run()
}
```

The builder is convenience. Advanced applications can directly compose runtime, shell, and surfaces.

## Dependency Recommendations

| Concern | Initial choice | Boundary rule |
|---|---|---|
| TUI buffer/layout | Ratatui 0.30+ | Hidden behind `near-render`/`near-ui` public API |
| Terminal I/O | Crossterm | Only `near-terminal` consumes raw events |
| Async runtime | Tokio | Domain APIs return futures but avoid leaking Tokio types where practical |
| Serialization/config | Serde + TOML | Version every persisted schema |
| Diagnostics | `tracing` | Commands/tasks receive correlation IDs |
| Errors | `thiserror`; `anyhow` in binaries | Typed library errors, contextual app errors |
| Search traversal | `ignore` | Explicit policy for hidden and ignore files |
| Fuzzy matching | `nucleo-matcher` | Reuse matcher allocation per surface |
| File watching | `notify` | Polling fallback and manual refresh remain available |
| PTY | `portable-pty` on Unix; `conpty` on Windows | Isolate behind `near-pty` |
| Terminal parser | `vt100` initially | Replaceable adapter; test nested TUI behavior before committing |
| Plugin sandbox | Wasmtime Component Model | Phase later; WIT API smaller than Rust API |
| Unicode | `unicode-segmentation`, `unicode-width` | Central width/grapheme policy in terminal layer |
| Clipboard | OSC 52 plus platform command fallback | User-configurable due security and terminal variance |

Pin exact dependency versions in the workspace lockfile, but keep the blueprint at capability/version-family level because the ecosystem changes rapidly.

## macOS-First Requirements

1. Build and test on current Apple Silicon and Intel macOS targets where CI availability permits.
2. Handle Unicode normalization differences without renaming or lossy path conversion.
3. Keep paths as `OsString`/bytes internally; display uses a reversible escaped representation for invalid text.
4. Integrate Trash using platform APIs or a narrowly scoped helper rather than shelling out to unsafe text commands.
5. Display and optionally edit POSIX permissions, ACL summaries, extended attributes, Finder tags, and quarantine state.
6. Detect cross-volume operations and APFS clone opportunities.
7. Support Terminal.app, iTerm2, Kitty, WezTerm, Ghostty, and tmux with a capability/degradation test matrix.
8. Make Option-key behavior diagnosable because terminal profiles differ in whether Option produces Meta, text composition, or escape prefixes.
9. Follow `$SHELL` and zsh conventions without requiring shell startup-file changes for basic operation.
10. Provide Homebrew formula artifacts and signed/notarized binaries only after core UX stabilizes.

## Portability Strategy

Platform-specific behavior is implemented through capability traits:

```rust
pub trait PlatformServices {
    fn trash(&self) -> Option<&dyn TrashService>;
    fn reveal(&self) -> Option<&dyn RevealService>;
    fn privilege(&self) -> Option<&dyn PrivilegeService>;
    fn metadata(&self) -> &dyn PlatformMetadataService;
    fn clipboard(&self) -> &dyn ClipboardService;
}
```

Linux follows after the local provider, operation engine, and terminal matrix are stable. Windows follows after APIs avoid Unix-only assumptions and can map to ConPTY, recycle bin, alternate data, ACLs, and drive/provider roots. Crossterm and `portable-pty` make these targets feasible, but portability must be proven in CI and integration tests rather than inferred from dependency claims.

## Testing and Verification

### Unit tests

- Key sequence parsing, precedence, timeout, and conflict reporting.
- Theme role fallback and terminal color degradation.
- Command availability and target selection.
- Resource URI parsing and provider routing.
- Operation planning and conflict-policy matrices.
- Query AST parsing and evaluation.

### Model tests

Feed events into a model and assert state/effects without a terminal. Every command should have tests for unavailable, successful, failed, cancelled, and stale-result paths where applicable.

### Render tests

Use Ratatui's test backend through Near's adapter and snapshot semantic screens at compact, standard, and wide sizes. Snapshot both cells and role IDs so theme-independent structural regressions are visible.

### Filesystem integration tests

Use temporary filesystems to test symlinks, hard links, permissions, xattrs, sparse files, Unicode names, conflicts, cancellation, cross-device behavior when CI supports it, and recovery journal entries.

### PTY tests

Script zsh, Vim/Neovim smoke tests, resize, alternate screen, bracketed paste, OSC 7, signal forwarding, and terminal restoration. Test suspend-and-run separately from embedded PTY.

### Golden workflow tests

Represent workflows as event scripts:

```text
open /tmp/source and /tmp/dest
select *.txt
press F5
assert copy dialog target == /tmp/dest
confirm
await task completion
assert destination listing contains selected items
```

These become the executable fidelity contract derived from the Far research.

### Performance budgets

- Key-to-render p95 below 16 ms for ordinary navigation on a warm local directory.
- No full-directory metadata stat before first paint for large folders.
- Smooth navigation through at least 100,000 virtualized entries.
- Search and copy progress delivered without starving input.
- Idle CPU near zero when no tasks or animations are active.
- Terminal always restored after normal exit, command failure, panic, and common termination signals.

## Delivery Roadmap

### Phase 0 — interaction laboratory

Build a single binary with fake data that proves:

- Dual surfaces, focus swap, cursor, selection, menu, dialog, status, and key bar.
- Semantic roles and `far-classic` theme.
- Command registry and contextual keymap resolution.
- Deterministic event-model and render snapshots.

**Exit gate:** the Far visual grammar is recognizable, effective bindings drive every visible hint, and no command is directly bound inside a widget.

### Phase 1 — usable local file workspace

- Local filesystem provider.
- Directory navigation, sorting, filtering, selection, bookmarks, histories.
- Copy, move, rename, mkdir, Trash/delete, and operation progress.
- Internal text/hex quick view.
- Suspend-and-run external viewer/editor/shell commands.

**Exit gate:** a developer can use `near-fm` for ordinary daily local file navigation without data-loss-prone behavior.

### Phase 2 — platform extraction

- Stabilize `near-core`, command, keymap, theme, resource, runtime, and surface APIs.
- Build `near-view` and `near-proc` without depending on file-manager internals.
- Publish architecture and application examples.

**Exit gate:** at least two non-file-manager applications reuse the same shell and interaction runtime.

### Phase 3 — advanced workflows

- Recursive search and results provider.
- File associations/handler rules and command templates.
- Compare, bulk apply, filters, configurable columns, metadata editing.
- Task history, operation journal, undo where supported.
- Embedded zsh PTY as an experimental feature.

**Exit gate:** core Far workflows are covered by golden tests and embedded PTY failures cannot corrupt terminal state.

### Phase 4 — extension platform

- Process extension protocol.
- WIT definitions and Wasmtime host.
- Plugin capability grants, package manifest, trust UI, and diagnostics.
- Archive and Git providers as first-party extension proofs.

**Exit gate:** a separately versioned plugin adds commands and a provider without linking against application internals.

### Phase 5 — ecosystem and portability

- Linux support matrix and packages.
- Windows adaptation.
- Theme/plugin registry and signed release metadata.
- API compatibility policy, migration tools, and long-term support channels.

## Major Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Recreating Far too literally | Require every core concept to work for a non-filesystem provider. |
| Overbuilding framework before product | Validate each abstraction in `near-fm`; extract only after a second consumer exists. |
| Keybinding inconsistency | Stable command IDs, context stack, generated hints/help, conflict diagnostics. |
| Terminal incompatibility | Capability probing, legacy fallback, terminal matrix, panic-safe restoration. |
| Data loss in file operations | Plan/execute split, safety classes, Trash default, journal, extensive filesystem tests. |
| Embedded shell complexity | Ship suspend-and-run first; keep PTY parser replaceable and feature-gated. |
| Plugin ABI churn | Delay public ABI, version WIT worlds, expose narrower interfaces than internal Rust APIs. |
| Async state races | Generation IDs, single-owner model mutation, bounded channels, cancellable tasks. |
| Theme fragmentation | Required core role fallbacks and validation tooling. |
| macOS-specific assumptions | Provider/platform capability traits and Linux CI beginning in Phase 1. |

## Recommended First Implementation Slice

The first code milestone should contain only:

1. Normalized terminal session and input events.
2. Semantic role/theme resolver.
3. Command registry plus keymap trie and context stack.
4. Workspace shell with two generic collection surfaces, menu, dialog, status, and key bar.
5. Fake provider with scripted workflow tests.
6. Local filesystem read-only provider.

Do not start copy/move, PTY embedding, plugins, or a general editor until the command/keymap/theme architecture is proven by the interaction laboratory. Those features depend on the abstraction; they should not define it accidentally.

## Research Basis

Research snapshot: June 23, 2026.

- Far Manager behavior and UX: repository documents under `docs/farmanager-3-*.md` and the corpus under `assets/farmanager-ux/`.
- Ratatui architecture, widgets, backends, and 0.30 modularization: <https://ratatui.rs/concepts/widgets/>, <https://ratatui.rs/concepts/backends/>, <https://ratatui.rs/highlights/v030/>.
- Crossterm event and keyboard enhancement support: <https://docs.rs/crossterm/latest/crossterm/>.
- Kitty progressive keyboard protocol and terminal support list: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>.
- Cross-platform PTY API: <https://docs.rs/portable-pty/latest/portable_pty/>.
- Terminal stream parser: <https://docs.rs/vt100/latest/vt100/>.
- Yazi keymap and plugin design comparison: <https://yazi-rs.github.io/docs/configuration/keymap/>, <https://yazi-rs.github.io/docs/plugins/overview/>.
- Helix command/key remapping comparison: <https://docs.helix-editor.com/remapping.html>.
- Recursive ignore-aware traversal: <https://docs.rs/ignore/latest/ignore/>.
- Cross-platform file watching: <https://docs.rs/notify/latest/notify/>.
- Fuzzy matching: <https://docs.rs/nucleo-matcher/latest/nucleo_matcher/>.
- Wasmtime Component Model embedding and WIT: <https://docs.wasmtime.dev/api/wasmtime/component/>, <https://component-model.bytecodealliance.org/>.
