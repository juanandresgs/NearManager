# Interaction Discovery Inventory

This inventory separates code-backed behavior from direct operator observations. A discovery record
remains pending until every terminal and Far observation named by its source list is captured; tests
alone do not promote it to complete.

## Panels — `DISC-PANEL-001`

- Edge and page movement: `Left`/`Home`, `Right`/`End`, and visible-row paging are executable
  conformance cases.
- Horizontal scrolling: `Alt+Left/Right`, `Ctrl+Alt+Left/Right`, and `Alt+Home/End` are owned by
  `CollectionSurface`; Unicode filenames are sliced on display-column boundaries.
- Panel resizing: `DualSurfaceLayout` owns horizontal and vertical resize clamping, reset, rendering
  geometry, and pointer-side hit testing. Far bindings resize by ten columns or five rows.
- Refresh, sort, and filter retention: collection snapshots retain surviving focus, selection, and
  horizontal offset. Same-location provider refresh restores the snapshot as pages arrive.
- Quick view synchronization: cursor, first/last, page, selection movement, panel focus, sorting,
  and comparison paths call `refresh_quick_view`; direct operator observation remains required.

## Selection — `DISC-SELECT-001`

- Non-contiguous selection: plain navigation preserves the selected set; Shift arrows and Insert
  toggle only the current row and move.
- Numeric keypad protocol: not declared supported until terminal protocol discovery is complete.
- Selected-first sorting: sorting retains the focused resource after selection changes.
- Refresh and operation retention: snapshot restoration retains surviving selected resources;
  completed move/trash items disappear and failed survivors remain selected.
- Folder and parent rules: selectability is provider-neutral metadata. Parent/navigation rows carry
  a denial reason and never enter target resolution.

## Operations — `DISC-OPERATIONS-001`

- Empty selection falls back to the current selectable resource.
- Non-empty selection takes precedence for ordinary mutation commands.
- `Shift+F5`, `Shift+F6`, and `Shift+F8` use `CollectionTargetScope::CurrentOnly` and ignore selection.
- Copy retains surviving selection; move/trash refresh removes completed resources while retaining
  failed survivors.
- Partial failure is asserted through operation preview, task history, provider refresh, filesystem
  state, and final semantic selection rendering.

## Other Surfaces — `DISC-SURFACES-001`

- `ListNavigation` now owns filtered cursor movement, paging, viewport following, boundaries, and
  visible-row targeting for menus, settings, tasks, command history, folder history, and resource
  history.
- Public `MenuSurface` now has a default `surface.menu` keymap context instead of relying on Near
  workspace internals.
- Help and inspector surfaces implement explicit Home, End, PageUp, and PageDown behavior.
- The real tmux PTY precheck now pages an 80-entry command history from Home through End and back,
  exercises Help Home/End/PageUp/PageDown, and confirms Tasks accepts edge/page commands and
  returns through Escape; this is automated prequalification, not operator observation.
- A temporary-panel command revealed that diagnostics could track a task while the Tasks surface
  remained empty. `near-runtime::TaskRecord` now owns visible lifecycle transitions, and the real
  PTY workflow proves command completion remains in history while copy-as-reference adds no task.
- The settings operator workflow exposed an internal confirmed command in the generic command
  palette. Command descriptors now own zero-argument invokability, and palettes exclude commands
  whose required values must first be collected by a dialog or another application surface.
- Viewer/editor corpus review found that `editor.tab_size` and `editor.expand_tabs` were visible,
  persisted settings that the editor never consumed, and that new-surface settings were being
  pushed into open editors during reload. Document policies now live in `near-config`; new editors
  capture one immutable policy, tab insertion and rendering use it, and the real PTY verifies exact
  space-versus-tab bytes. The class-level rule is that every writable descriptor needs a visible
  effect regression and an application-scope regression, not just schema/persistence coverage.
- The same audit found that Viewer guidance advertised automatic and UTF-16 decoding that its
  parser rejected, while the single persistence switch still wrote state and could not retain
  position, bookmarks, encoding, and view mode independently. Encoding detection, encoded newline
  navigation/search, and policy-owned state filtering now have owner, render, application, and
  public-consumer regressions. The class rule remains: guidance, accepted values, runtime behavior,
  and persisted state must be generated from one contract and tested together.
- Shell profiles are captured per spawned PTY session, including resolved mode and close policy.
  `warn`, `keep-open`, and `close` have visible running/completed-child decisions, and child exit
  wakes the shared blocking reactor rather than depending on polling or a subsequent keypress.
- The real tmux PTY precheck exercises every close policy, retained output, same-child resume, and
  automatic close after child exit; this remains prequalification rather than terminal-matrix proof.
- Viewer, editor, dialog focus cycling, overlay restoration, resize behavior, and terminal handoff
  still require the declared direct operator walkthrough before this record can become complete.

## Terminal Input — `DISC-TERMINAL-001`

- Enhanced input tracks modifier press/release and clears held state on focus loss.
- Keymap repeat dispatch is limited to single-stroke navigation keys by default. Bindings may
  explicitly opt in or out; protected function-key actions remain press-only, and an unrelated
  repeat cannot consume or reset a pending chord.
- Legacy input remains explicitly degraded where release events or keypad identity are unavailable.
- Legacy terminals also encode `Ctrl+Shift+letter` like `Ctrl+letter`. The editor persistent-block
  binding exposed this collision in a real tmux PTY; the command palette is currently the honest
  fallback while terminal capability-aware keymap presentation remains open as `IF-009`.
- A direct Konsole-plus-tmux workflow exposed F9 arriving as a plain Escape followed by literal
  `[20~` after overlay transitions. `near-terminal` now owns a bounded fragmented-sequence
  coalescer, with a pure parser regression and a real tmux split-write reproduction; applications
  never repair terminal byte fragments in command or surface handlers.
- Exact behavior still requires the Terminal.app, iTerm2, Ghostty, GNOME Terminal, Konsole, tmux,
  and SSH protocol matrix. No keypad identity support is inferred from ordinary navigation tests.

## Escaped-Fault Rule

The original missing panel navigation exposed a class failure: behavior was patched at application
composition without a reusable interaction inventory. Each discovered class now has a reusable
owner, an application integration test, a semantic render assertion, a public facade export, and an
operator scenario. Future escaped interaction faults must add both a minimal reproduction and a
class-level conformance case.
