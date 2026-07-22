# Near Temporary Panels

Near Temporary Panels are mutable collections of references to original provider resources. They do not copy file content into a synthetic filesystem and they remain distinct from saved search and extension-generated panels.

## Current Workflow

- `Alt+F1` and `Alt+F2` include a **Temporary** entry for the requested side. The numbered slot
  menu shows reference counts and up to three persisted names before a slot is opened.
- `Alt+Shift+0` through `Alt+Shift+9` opens one of ten persistent slots in the focused side.
- `Alt+Shift+F12` lists all slots, their reference counts, and the side currently displaying a slot.
- Slot contents and modes survive Near restarts. `Shift+F7` clears the focused slot and
  `Ctrl+Shift+F7` clears every non-safe slot; both actions remove references only.
- With a Temporary Panel on the passive side, `F5` or the copy command adds the active panel's selected resources, or its current resource when nothing is selected. Duplicate source identities are ignored and no filesystem copy is planned.
- `F5` never moves, renames, or deletes a source resource. A lost or cleared state document can
  lose only the collected references, never the underlying files. State replacement retains a
  recovery copy and a failed save keeps the in-memory collection available.
- Operations invoked while a Temporary Panel is active use each row's original `ResourceRef`, so provider routing and safety policy remain authoritative.
- `F7` removes selected references, or the current reference, from the slot without deleting or modifying the source resources.
- `Ctrl+PageUp` leaves the Temporary Panel and navigates to the current resource's source parent.
  When the provider listing arrives asynchronously, Near focuses the exact original `ResourceRef`
  rather than merely opening its parent.
- `Alt+Shift+F2` exports the active slot as a UTF-8 list. Resource rows use provider-qualified
  identities, so importing the list preserves routing instead of guessing from display names.
- Commands → Temporary panel import accepts `append` or `replace`. Absolute local paths and
  provider-qualified resources are validated before insertion; invalid lines are reported.
- `tmp:` is the built-in command prefix. `+0` through `+9` select a slot, `+safe`/`-safe` set
  mutation policy, `+any`/`-any` control arbitrary-line ingestion, and
  `+replace`/`-replace` select replace or append mode. A trailing quoted path imports that list.
- `tmp:<command` executes through the configured command-line executor without blocking input and
  ingests standard output using the same validation, `+any`, and replace/append rules. When invoked
  from a Temporary Panel, the peer provider location supplies the shell working directory.
- `tmp:+menu"list-file"` renders the list as a menu. `|label|action` provides an accelerator label,
  `|-|` creates a disabled separator, resource actions navigate or reveal, registered prefixes are
  dispatched, and other actions are copied to the command line.
- `+full` gives the focused Temporary Panel the complete panel viewport; `-full` restores the dual
  panel composition without losing the slot contents.
- Safe mode is shown as `R` in the slot menu and prevents adding/removing references or planning
  source mutations.
- In `+any` mode, unresolved lines remain virtual rows. Enter copies the row text to Near's command
  line; those rows are never submitted to a resource provider.
- Refresh re-stats every provider resource. Missing or disconnected resources remain in the slot
  with `near.temporary-panel.stale` metadata instead of disappearing silently.

## Incomplete Parity

The real tmux PTY precheck exercises menu-based slot opening, slot isolation, F5 copy-as-reference,
F3/F4 source-provider dispatch, F7 reference-only removal, Ctrl+PageUp source reveal,
Alt+Shift+F2 provider-qualified export and replace import, safe-mode mutation denial, command and
menu ingestion, full-panel composition, and stale-reference retention while checking source state.

`FAR-EXT-004` remains partial because current-revision direct macOS/Linux operator terminal
evidence is required and automatic list-file opening, recursive folder-expansion policy, and the
Far `Alt+Shift+F3` passive-panel reveal workflow remain pending.

Near's provider-qualified export format is an intentional provider-neutral extension of Far's
local-path list. Imports continue to accept absolute local paths for interoperability.
