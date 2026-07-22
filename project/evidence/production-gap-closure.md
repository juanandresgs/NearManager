# Production Gap Closure Evidence

This evidence record tracks implementation work for the seven parity gaps identified by the
production autonomy plan. The items remain `partial` until the macOS and Linux operator matrix in
`docs/near-production-workflow-testing-guide.md` is completed at one candidate revision.

The current dirty macOS candidate also completes the locally executable release path: optimized
`near-fm`, `near-view`, `near-proc`, and `near-demo` binaries build, the deterministic archive
smoke-tests every binary, and checksum, provenance, and SPDX SBOM generation pass. Production
publication remains gated on a clean signed candidate plus the platform/operator requirements.

## Terminal and Interaction

- `FAR-PANEL-007`: filename lookup now runs only after effective keymap resolution reports no
  match. Enhanced modifier-only events are merged into the following character; direct legacy Alt
  chords remain supported. Focus loss clears held state. Focused tests cover cycling, acceptance,
  restoration, enhanced holds, legacy fallback, and bound Alt chord precedence.
- `FAR-MENU-002`: keybar layers remain derived from the effective keymap. Held layers are enabled
  only in enhanced mode; legacy mode always renders the honest base layer and records a diagnostic.
- `FAR-PLAT-001`: `TerminalSession` selects enhanced keyboard input from known terminal capability
  signals, with an explicit `NEAR_KEYBOARD_PROTOCOL` override, rather than issuing a blocking
  startup query. The resulting mode is propagated into `FarWorkspace`, which owns capability-aware
  held state and clears it on focus loss. Operator restoration and terminal matrix evidence is
  still required.
- `tools/test_tmux_terminal_workflows.py` now proves the real PTY path for legacy keybar rendering,
  Alt lookup cycling/no-match, history/Help/Tasks navigation, embedded shell and REPL output,
  warn/keep-open/close lifecycle behavior, workspace restoration, and host-shell restoration after
  quit. It is deliberately labeled a non-operator precheck, not terminal-matrix completion.

## Typed Settings

- `FAR-CUSTOM-004`: `near-config::ConfigurationCoordinator` exposes public typed descriptors,
  values, provenance, platform availability, live/new-surface/restart scopes, validation hooks,
  ordered apply, reverse rollback, atomic persistence restoration, reset, and external reload.
- The coordinator is re-exported through `near-app`. Unit tests prove ordered application, one
  candidate persistence write, rollback after a later failure, restart detection, and last-valid
  external reload behavior.
- `near-ui::SettingsSurface`, re-exported by `near-app`, generically renders typed descriptors,
  filters across identifiers/categories/provenance, stages typed edits and resets, marks changed
  values, reports apply scope, hides advanced descriptors until explicit F6 disclosure while still
  searching them, and exports coordinator candidates. Near FM exposes every shipped runtime
  settings document through this surface. `interface.startup_panel` supplies a real restart-scoped
  candidate and the tmux workflow proves persistence, no live focus mutation, and application after
  process restart. Current-revision direct operator workflow evidence still remains required before
  parity verification.

## Shell, Viewer, and Editor

- `FAR-SHELL-006`: `near-pty::ShellProfile` is versioned and supports platform-default, login,
  interactive, and clean modes, explicit programs/arguments/startup commands, environment
  inheritance, and exit policy declaration. macOS resolves the account shell with `dscl`; Linux
  uses `getent` before environment fallback. Each shared session captures its resolved profile;
  Near FM labels the mode and policy and enforces warn, keep-open, and close behavior for running
  and completed children without silently abandoning a process.
- `FAR-VIEW-005`: layered `viewer.toml` controls wrap, hex, encoding, internal/external/association
  open policy, and per-resource restoration. New viewer surfaces consume defaults before optional
  resource state restoration.
- `FAR-EDIT-005`: layered `editor.toml` now includes internal/external/association open policy,
  persistent blocks, tab size, and expand-tabs defaults. Workspace routing honors the configured
  policy while retaining explicit external-edit and association commands.

## Production Enforcement

- `tools/qualify.py production` now requires all parity records to be verified, clean source,
  macOS/Linux operator evidence, a separate exact-revision `NearTuiProof`, release packages,
  checksums, provenance, and an SPDX SBOM.
- Release-package qualification extracts every archive and executes stable help/version contracts
  for all four binaries. Version-2 provenance fails closed on revision mismatch, dirty source,
  archive/member/checksum drift, or smoke-record drift. The deterministic SPDX 2.3 generator
  describes all workspace packages and the complete locked dependency graph without network access.
- `tools/workflow_evidence.py` initializes, records, hashes, and validates the terminal/operator
  matrix. Evidence is revision-bound and artifacts are rehashed during qualification.
- These enforcement gates intentionally fail until external operator and private consumer evidence
  exists; implementation tests alone do not upgrade parity status.
