# Near Terminal Workspaces

Near retains the Far dual-panel workspace as its default composition while allowing persistent
terminal sessions to participate as tabs, peer panes, and reversible full-screen user screens.

## Interaction Model

- `Ctrl+Alt+N` creates and activates another persistent native-shell tab.
  `Ctrl+Shift+T` remains an enhanced-keyboard alias.
- `Ctrl+Alt+PageUp` and `Ctrl+Alt+PageDown` cycle terminal tabs without affecting editor or panel
  screen order. The corresponding `Ctrl+Shift` chords remain enhanced-keyboard aliases.
- **Commands → Terminal tabs** lists every retained terminal and exposes pane placement, hide,
  zoom, and close actions.
- A terminal may replace the left or right panel view. `Ctrl+Alt+P` moves focus from the terminal
  to its peer; ordinary `Tab` moves from the file panel back to the terminal pane. This command is
  intentionally denied while the terminal is full screen because no peer pane is visible.
- `Ctrl+O` zooms a terminal pane to the complete user screen and restores the exact previous pane
  when pressed again. From the ordinary panels screen it retains the classic user-screen toggle.
- F12 lists panels, editors, and every terminal as numbered retained screens. Keys `1` through `9`
  switch directly, and terminal rows show the foreground process (`codex`, `vim`, `python`, the
  account shell, and so on) plus running/exited and active state.

Each tab owns an independent PTY, shell profile, working directory, scrollback, process lifecycle,
and output wake. Hidden tabs continue running. Only the selected tab is projected into the dock,
pane, or full-screen viewport, preventing competing input or size ownership.

Legacy terminals cannot distinguish several `Ctrl+Shift+letter` chords from their unshifted
control character. Near therefore advertises `Ctrl+Alt` primary bindings and retains the
`Ctrl+Shift` forms only where the terminal reports enhanced modified keys.
When a terminal is full screen or occupies a peer pane, the bottom keybar displays the active
terminal-workspace bindings directly, including New, previous/next tab, peer focus, zoom, and
screen selection.

## Reusable Ownership

`TabRegistry<T>` owns stable tab identity, selection, cycling, close behavior, and titles.
`ZoomablePanePresentation` owns base, first/second pane, and reversible full-screen state. Neither
type depends on PTYs, file panels, Far Manager, Ratatui, or Crossterm. Near FM owns the default
bindings, menu placement, shell close-policy presentation, and dual-panel composition.

## Experimental Limits

This branch intentionally proves two peer panes with tab stacks before introducing arbitrary split
trees. It does not yet persist terminal tabs across Near process restart, forward mouse protocol
events into pane-hosted terminal applications, or provide a structured cross-agent message bus.
