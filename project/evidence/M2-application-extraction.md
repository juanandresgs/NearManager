# M2 Application Extraction Evidence

Date: 2026-06-23

## Implemented Slice

- `near-ui::SurfaceApplication` runs any single backend-independent surface through the shared terminal lifecycle, signals, keymap sequences, semantic scene rendering, help, and command palette.
- `near-app` provides the application-facing builder facade and reexports provider, resource, scene, surface, viewer, configuration, and bounded-task contracts without exposing backend crates.
- `near-view` accepts a file path, `-`/stdin, `file://`, `proc://`, and `plugin://` resources; it uses the shared viewer surface, theme, keymap, help, application runtime, and provider streams.
- When stdout is not a terminal, `near-view` emits exact source bytes with no terminal control sequences.
- `near-proc` uses the process provider, `ResourceRef`, `CollectionSurface`, a domain surface, shared commands/keymap/theme/help/palette, and `TaskPool` for process snapshot loading.
- The `near.process.toggle-details` domain command is present in the effective keymap, contextual help, and normal command palette.

## Automated Evidence

- `near-view` CLI tests prove exact binary file and stdin output, `file://` input, non-filesystem `plugin://` input, and absence of direct `near-fm`, Ratatui, and Crossterm dependencies.
- A macOS PTY test proves the real viewer enters/leaves the alternate screen, opens shared contextual help, dismisses overlays, and exits cleanly.
- Single-surface runtime tests prove help, palette routing, viewer hex mode, and operation without any peer surface.
- `near-proc` tests prove its domain command appears in help and palette and changes the rendered process surface.
- The process application boundary test proves it contains no filesystem path assumptions and no direct backend or file-manager dependency.
- `near-app` builder tests prove shared theme/keymap configuration is required and a backend-independent viewer application builds and snapshots.

## Requirement Status

- `REQ-APP-001` is verified by file, stdin, and provider-URI workflows, shared configuration files, PTY behavior, and dependency-boundary tests.
- `REQ-APP-002` is verified by `near-proc` provider/resource/surface/command/keymap/theme/task composition and domain-command discovery tests.
- `REQ-API-001` is verified by facade and application compile tests plus the structural public API audit.
