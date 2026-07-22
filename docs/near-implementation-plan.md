# Near Implementation Plan

## Delivery Strategy

Build Near through vertical slices, not by independently completing framework crates. Every milestone must produce a runnable application, test a real Far-derived workflow, and leave the architecture more reusable than before.

The order is intentionally:

1. Interaction semantics.
2. Read-only filesystem usefulness.
3. Safe file mutations.
4. Extraction for other applications.
5. Advanced workflows and PTY embedding.
6. Third-party extensions.

Starting with plugins, embedded terminals, or a generalized editor would consume substantial effort before the command/keymap/theme contract is proven.

## Repository Bootstrap

Initial workspace:

```text
Cargo.toml
rust-toolchain.toml
crates/
  near-core/
  near-terminal/
  near-ui/
  near-fs/
apps/
  near-fm/
  near-demo/
tests/
  workflows/
themes/
  far-classic.toml
```

Add crates only when they have a clear owner and dependency boundary. The logical modules in `near-platform-blueprint.md` can initially live inside these five crates.

### Workspace policies

- One pinned stable Rust toolchain, with the minimum supported Rust version declared after the first release.
- `cargo fmt --check`, Clippy with warnings denied for project code, unit/integration tests, and documentation builds in CI.
- Dependency policy and license checks through `cargo-deny`.
- Supply-chain vulnerability reporting through RustSec tooling.
- No `unsafe` outside explicitly audited platform modules; each use requires a local safety explanation and tests.
- Feature flags represent optional capabilities, not mutually incompatible product editions.

## Workstream Map

| Workstream | Primary output | Depends on |
|---|---|---|
| Terminal session | Reliable startup/input/render/restore | None |
| Semantic rendering | Role-based scene and Far theme | Terminal session |
| Command runtime | Stable IDs, availability, invocation | Core types |
| Keymap runtime | Context trie, hints, diagnostics | Command runtime, input normalization |
| Shell/workspace | Panes, overlays, focus, menus, status | Rendering, commands, keymaps |
| Resource model | Providers, locations, metadata, capabilities | Core types |
| Filesystem | Local provider and metadata | Resource model |
| Operations | Plans, tasks, conflicts, journal | Filesystem, runtime |
| Search | Predicates, traversal, result providers | Filesystem, resource model |
| External tools | Suspend/run, handlers, templates | Terminal session, commands |
| PTY | Embedded zsh and terminal surface | External tools, shell/workspace |
| Platform API | Builder, docs, examples | Two real applications |
| Extensions | Process protocol and Wasm host | Stable platform API |

## Epic 1 — Terminal Lifecycle

### Deliverables

- `TerminalSession` RAII guard.
- Alternate-screen and inline-mode options.
- Raw mode, cursor, resize, focus, paste, mouse, and keyboard events normalized into Near events.
- Kitty keyboard protocol negotiation when available.
- Synchronized-update support when available.
- Signal and panic restoration paths.
- Diagnostic command that prints detected terminal capabilities.

### Acceptance

- Terminal state restores after ordinary quit, panic, failed initialization, `Ctrl+C`, and an external command round trip.
- Key event fixtures cover legacy encoding and enhanced protocol events.
- Terminal.app, iTerm2, Kitty, WezTerm, Ghostty, and tmux results are recorded in a compatibility table.

## Epic 2 — Semantic Rendering and Theme Resolution

### Deliverables

- `Scene`, `Node`, `TextRun`, `CellStyle`, semantic role IDs, and a Ratatui adapter.
- Theme loader and validator using the schema demonstrated in `specs/theme.toml`.
- Role fallback resolver and terminal color degradation.
- Border/glyph selection with Unicode and ASCII fallback.
- `far-classic`, terminal-native, and high-contrast themes.
- Hot reload with error reporting that preserves the last valid theme.

### Acceptance

- Identical scene snapshots can render under all three themes.
- Missing application-specific roles fall back to required core roles.
- 16-color rendering retains focus and selection distinctions.

## Epic 3 — Commands and Keymaps

### Deliverables

- Command registry, descriptors, categories, argument values, safety classes, and availability.
- Context stack and key-sequence trie.
- TOML keymap loader matching `specs/keymap.toml`.
- Prepend/replace/remove/inherit layering.
- Conflict and unreachable-binding diagnostics with source locations.
- Generated function-key bar, effective-key help, and command palette.
- Command invocation recording suitable for macros and workflow tests.

### Acceptance

- No widget or application model matches raw keys directly.
- Rebinding `F5` immediately changes command execution, key bar text, and help output.
- Prefix sequences, timeout behavior, repeated keys, and enhanced-key distinctions are deterministic under a fake clock.

## Epic 4 — Workspace Shell

### Deliverables

- Focus tree and two-peer workspace layout.
- Collection, menu, dialog, notification, status, command-line placeholder, and help surfaces.
- Overlay stack with modal and non-modal behavior.
- Compact, standard, and wide layout policies.
- Surface lifecycle, focus restoration, and cursor ownership.
- Fake resource provider and interaction demo.

### Acceptance

- Golden tests cover focus swap, panel swap, menu navigation, selection, confirmation, resize, and help.
- All visible interaction uses semantic commands and generated hints.
- The demo remains usable at 60×18 and scales to wide terminals.

## Epic 5 — Resource and Local Filesystem Model

### Deliverables

- `Location`, `ResourceRef`, metadata, capability sets, provider registry, and paged listing contract.
- macOS local filesystem provider.
- Progressive metadata loading and cancellation.
- Navigation history, bookmarks, sort, columns, hidden policy, and refresh.
- File watching with polling/manual fallback.
- Reversible display encoding for paths not valid as Unicode.

### Acceptance

- First paint of a large directory does not wait for all metadata.
- Symlinks, packages, hidden files, Unicode names, and permission failures are represented without crashes or silent path changes.
- A stale listing result cannot replace a newer navigation state.

## Epic 6 — Viewer and External Tool Handoff

### Deliverables

- Internal text/hex viewer surface with search, wrapping, encodings, offset navigation, and bookmarks.
- Quick-view inspector connected to peer cursor changes.
- Handler rules and safe structured argument templates.
- Suspend-and-run external tools.
- Default examples for `$PAGER`, `$EDITOR`, Vim/Neovim, `less`, and `hexyl`/`xxd`.

### Acceptance

- Opening and returning from Vim/Neovim restores Near exactly.
- Handler templates never pass selected paths through a shell unless the rule explicitly requests shell evaluation.
- Large files can be viewed without loading the complete file into memory.

## Epic 7 — Safe File Operations

### Deliverables

- Copy, move, rename, mkdir, link, Trash, delete, and attribute operation plans.
- Preview/confirmation dialogs driven by plan metadata.
- Background tasks, byte/item progress, cancellation, conflict questions, retry, and result summaries.
- macOS Trash service and same-volume atomic rename.
- Metadata policies and symlink policies.
- Append-only operation journal.

### Acceptance

- Integration matrix covers file/directory, symlink, hard link, overwrite, same-volume, cross-volume, cancellation, read-only, permission failure, and insufficient space scenarios.
- Default deletion uses Trash where available.
- A cancelled or failed operation reports exactly which items completed and which did not.
- Destructive actions cannot run through an unavailable or stale command context.

## Epic 8 — Search, Filters, and Result Providers

### Deliverables

- Predicate AST for names, kinds, sizes, timestamps, attributes, and content.
- Panel filter dialog and reusable saved filters.
- Recursive traversal with explicit hidden/Git-ignore policy.
- Streaming content search with encoding/binary policy.
- Search result provider navigable like an ordinary panel.
- Fuzzy find and command/history matching through a reused `nucleo-matcher` instance.

### Acceptance

- Search can be cancelled without blocking input.
- Results remain linked to source locations and can view, edit, reveal, or transfer resources.
- Filter semantics are identical when used by a panel and by an operation scope.

## Epic 9 — Histories, Templates, and Macros

### Deliverables

- Folder, command, viewer/editor, search, and task histories.
- Typed context-template expressions replacing Far metasymbol string substitution.
- User-defined command menus generated from command invocations.
- Command-sequence recording and replay.
- Conditional macro execution based on context/capabilities.

### Acceptance

- Recorded macros contain command IDs and typed arguments, not raw terminal bytes.
- Renamed keybindings do not break macros.
- A macro cannot bypass command availability or safety confirmation policy unless explicitly trusted and configured.

## Epic 10 — Second and Third Applications

### `near-view`

Extract and ship the viewer as a standalone binary accepting paths, stdin, and provider URIs.

### `near-proc`

Implement a process provider, process detail inspector, filtering, signals, and safe privileged-action handling.

### Acceptance

- Both applications use the same terminal, command, keymap, theme, shell, help, and configuration systems.
- Neither depends on `near-fm` modules.
- At least one third example application can be written using only documented public APIs.

## Epic 11 — Public Platform API

### Deliverables

- `near-app` facade and application builder.
- Public surface/provider/command extension traits.
- API examples for collection browser, dashboard, and picker.
- Semver policy and deprecation annotations.
- Stable core role, command, and context naming conventions.
- `near doctor` configuration and terminal diagnostics.

### Acceptance

- A new application can register a provider and render peer collections without importing Ratatui or Crossterm.
- Public documentation contains complete runnable examples.
- API review confirms platform types do not expose file-manager assumptions.

## Epic 12 — Embedded PTY

### Deliverables

- `portable-pty` process lifecycle and resize adapter.
- Terminal parser adapter behind a replaceable trait.
- PTY surface rendering, scrollback, selection, paste, and search.
- zsh launch profiles, environment propagation, OSC 7 directory synchronization, title/progress callbacks, and signal forwarding.
- Input-mode routing for nested TUIs and mouse-aware children.

### Acceptance

- Interactive zsh, `ssh`, Vim/Neovim, and common full-screen programs work in the supported terminal matrix or are explicitly documented as degraded.
- Exiting or crashing the child never corrupts the Near terminal session.
- Embedded mode remains optional; suspend-and-run always remains available.

## Epic 13 — Extension Host

### Deliverables

- Versioned plugin manifest and package layout.
- Out-of-process message protocol for commands and providers.
- Wasmtime Component Model host using a reviewed revision of `specs/plugin.wit`.
- Capability grant storage and trust UI.
- Plugin lifecycle, timeout, resource limits, diagnostics, and disable/recovery flow.
- First-party archive and Git provider plugins.

### Acceptance

- A plugin crash or trap does not terminate the application.
- Undeclared filesystem, process, and network access is denied.
- Plugin commands appear in keymaps, menus, hints, help, and macros exactly like built-in commands.
- Host compatibility errors identify required and provided interface versions.

## Epic 14 — Linux and Windows Portability

### Linux

- Test common terminal emulators and tmux.
- Implement Trash/XDG integration, metadata differences, and packaging.

### Windows

- Map provider roots to drives, UNC paths, and shell locations.
- Validate ConPTY, recycle bin, ACLs, reparse points, alternate data streams, and long path behavior.

### Acceptance

- Platform capability reports explain unavailable operations rather than hiding them.
- Shared workflow suites run on all supported systems with platform-specific expectations isolated.

## CI and Release Matrix

### Per pull request

- Formatting, Clippy, unit/model/render tests on macOS and Linux.
- Filesystem integration tests on macOS and Linux.
- Schema fixture tests for config, theme, keymap, journal, and plugin manifests.
- Documentation link and example compilation checks.

### Scheduled

- Terminal emulator compatibility harness where automation permits.
- Large directory, search, copy, and rendering benchmarks.
- Dependency/security/license audit.
- Windows build and workflow tests beginning before the platform API is declared stable.

### Release channels

- `nightly`: automated builds for active development.
- `preview`: milestone builds with schema migration notes.
- `stable`: signed tags, changelog, checksums, SBOM, Homebrew formula, and reproducible build documentation.

Persisted schemas and plugin interfaces require explicit versions and migration code before stable releases.

## Team Parallelization

The safest early split is:

- **Runtime/UI:** terminal lifecycle, scene, theme, focus, workspace.
- **Interaction:** command registry, keymap, help, palette, workflow harness.
- **Filesystem:** provider, metadata, operation planner/executor, macOS integration.
- **Quality:** fake providers, snapshot tooling, workflow runner, terminal matrix.

PTY and plugins should not become independent workstreams until the core contracts they consume have passed Phase 2 extraction.

## Definition of Platform Readiness

Near is ready to call a reusable platform only when all are true:

1. `near-fm`, `near-view`, and one non-filesystem application share the runtime.
2. Applications define behavior through commands and surfaces without raw key matching.
3. Themes and keymaps apply consistently across the suite.
4. Public APIs do not expose Ratatui, Crossterm, local-path, or dual-panel assumptions unnecessarily.
5. Golden workflows and render tests prove interaction consistency.
6. A documented third-party example can be built without reading internal application code.

