# Building Applications with Near

## Intended Use

Near is appropriate when an application benefits from:

- Dense keyboard-driven navigation.
- One or more browsable collections.
- Context-sensitive operations and menus.
- Inspectors, viewers, dialogs, task progress, histories, or shell/tool integration.
- A visual and interaction language shared with other Near applications.

It is not intended to replace simple one-shot CLI commands. A command that reads arguments, prints output, and exits should remain an ordinary CLI and can optionally be invoked by a Near command.

## Application Composition

Most applications provide five things:

1. **Providers** that expose resources or records.
2. **Surfaces** that present them.
3. **Commands** that act on the current context.
4. **Contexts and default bindings** that make commands efficient.
5. **Roles** that theme application-specific visual meaning.

The Near shell supplies terminal lifecycle, focus, layout, menus, overlays, command palette, help, key hints, notifications, and task presentation.

## Smallest Useful Application

```rust
use near::prelude::*;

fn main() -> near::Result<()> {
    near::App::builder("example.bookmarks")
        .title("Bookmarks")
        .provider(BookmarkProvider::load()?)
        .command(OpenBookmark)
        .workspace(|workspace| {
            workspace.collection("bookmark://all")
                .columns(["name", "url", "tags"])
                .inspector(BookmarkInspector)
        })
        .run()
}
```

The actual facade should stay this compact even if internal runtime construction is more explicit.

## Designing a Provider

A provider should model a domain as navigable resources rather than as rendered rows.

Good resource models:

- Filesystem: folders contain files and folders.
- Processes: hosts contain processes; processes expose threads, files, ports, or metadata.
- Git: repositories contain worktrees, changes, commits, trees, and refs.
- Database: connections contain schemas, tables, views, and query results.
- Logs: services contain streams; streams contain records and time windows.

Avoid placing presentation-only values in resource identity. A row number, sort index, rendered label, or terminal coordinate is not a resource ID.

Providers should:

- Return a fast first page before expensive metadata is ready.
- Advertise capabilities per resource.
- Be cancellable and generation-safe.
- Use stable URI-like references.
- Report partial failures rather than collapsing an entire collection when one item cannot be read.

## Designing Commands

Commands are named product capabilities. Choose IDs that describe intent rather than keys or UI controls:

```text
good: acme.logs.follow
good: near.resource.copy-to-peer
bad:  acme.on-f5
bad:  acme.open-dialog-3
```

Commands should be reusable from keymaps, menus, the command palette, macros, tests, and plugins. They should not depend on which of those mechanisms invoked them.

### Availability

Use command availability for conditions that require domain logic:

- A process can be signaled.
- A connection is writable.
- At least two comparable resources exist.
- The focused provider supports deletion.

Use keymap `when` expressions only for simple contextual filtering and presentation. The command remains the authority.

### Effects and tasks

Return immediate UI effects for navigation, overlays, and notifications. Long-running or cancellable work becomes a task. Tasks should expose phase, item count, byte count where meaningful, cancellation support, logs, and final outcomes.

## Choosing a Surface

### Collection

Use for a sorted/filterable list with cursor and selection. Most applications start here.

`CollectionSurface` owns cursor visibility, viewport paging, and visible-row hit testing. Render the
surface once at its actual area before issuing page commands; subsequent `near.collection.page`
commands use that recorded viewport height. Applications should use `viewport()` and
`item_at_visible_row()` rather than maintaining parallel scroll offsets.

Rows that are structural rather than selectable use `CollectionEntry::with_selection_denial` with
an operator-facing reason. Rows that must remain ahead of normal sorting use
`CollectionEntry::with_sort_priority`. These are generic collection contracts; applications should
not teach the surface about magic filenames or domain-specific resource kinds.

### Tree

Use when hierarchy itself is the main navigation structure and lazy expansion is meaningful.

### Inspector

Use for details that follow the cursor without changing focus. Keep expensive inspectors asynchronous and cancel stale work.

### Viewer

Use for scrollable/searchable immutable content. Provide a resource stream rather than pre-rendered terminal lines.

### Terminal

Use only when the domain genuinely requires an interactive child process. Prefer a normal command or suspend-and-run handler for one-shot tools.

### Custom surface

Build a custom surface when existing surfaces cannot express the interaction, not merely to change colors or borders. Custom surfaces still use Near focus, commands, contexts, roles, and scene primitives.

## Peer Workspaces

Peer surfaces generalize Far's active/passive panels. They are useful when commands naturally combine two contexts:

- Source and destination.
- Local and remote.
- Before and after.
- Available and installed.
- Working tree and repository history.
- Query catalog and query results.

Do not force two panes into applications that do not benefit from a peer relationship. Near also supports one collection plus an inspector, dashboards, tabs, and compact single-surface layouts.

## Keymap Design

Application defaults should:

- Bind semantic commands in application contexts.
- Reuse suite-wide commands such as help, quit, command palette, focus movement, and overlay controls.
- Provide function-key hints only for the highest-value current actions.
- Avoid binding printable keys in text-entry contexts.
- Offer optional navigation presets rather than mixing Far and Vim grammar unpredictably.
- Include descriptions because they drive help and sequence overlays.

Users own the final effective keymap. An application should not treat a default binding as a permanent API.

## Theme Roles

Applications introduce roles only for semantic distinctions that a theme may reasonably want to style:

```text
acme.logs.level.error
acme.logs.level.warning
acme.logs.timestamp
acme.logs.source
```

Each role declares a core fallback such as `status.error`, `status.warning`, `text.muted`, or `text`. Do not create roles for arbitrary widget coordinates.

## External Tool Integration

Prefer structured handlers:

```toml
[[handler]]
id = "acme.edit-neovim"
action = "edit"
when = "resource.scheme == 'file' && env.NVIM != null"
program = "nvim"
args = ["{resource.path}"]
mode = "suspend"
```

Use shell mode only when shell syntax is the feature. Structured argv preserves filenames containing spaces, newlines, glob characters, and shell metacharacters.

## CLI Composability

Every Near application should consider non-interactive modes:

- Accept resources from argv and stdin.
- Emit selected resources or command results as plain text, JSON, or NUL-delimited records.
- Provide `--no-ui` or subcommands for automatable operations where appropriate.
- Return meaningful exit codes.
- Never emit terminal control sequences when stdout is not a terminal unless explicitly requested.

For example, `near-pick --print0 | xargs -0 ...` should compose cleanly with zsh and other shells.

## Configuration Ownership

Applications may add namespaced sections to shared files or use their own files below the Near config directory. They should not rewrite user-authored configuration. Generated state belongs in state/cache storage.

Configuration errors should identify:

- File and line/column.
- Invalid field or value.
- Expected schema version.
- Whether the last valid value remains active.
- A suggested correction when one is unambiguous.

## Testing an Application

An application test suite should contain:

1. Provider contract tests.
2. Command availability and effect tests.
3. Workflow event scripts.
4. Compact/standard/wide render snapshots.
5. Theme independence checks.
6. Keymap rebinding tests for primary commands.
7. Cancellation and stale-result tests for every async surface.
8. CLI pipe/input/output tests for non-interactive modes.

## Publishing an Application

Before publishing, verify:

- Command, context, role, provider, and handler IDs use a stable namespace.
- No public API leaks application-internal model types unnecessarily.
- Default keymap entries have descriptions and no unresolved conflicts.
- All custom roles have core fallbacks.
- Terminal restoration is covered by integration tests.
- macOS and Linux behavior is documented even if one remains unsupported.
- Configuration and persisted state schemas declare versions.
- A migration note exists for every breaking user-facing schema change.

## Decision Checklist

When adding a feature, ask in order:

1. Is this an existing command with new availability or arguments?
2. Is it a new semantic command?
3. Does it need a task or only an immediate effect?
4. Is the data an existing resource type or a new provider?
5. Can an existing surface present it?
6. Does it add a genuinely themeable semantic role?
7. Does it belong in configuration, a process extension, a Wasm component, or trusted native code?
8. Can another Near application reuse the abstraction without inheriting this application's assumptions?

If the final answer is no, keep the feature local rather than expanding the platform API prematurely.
