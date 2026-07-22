# Intermediate Development Feedback Reassessment

Date: 2026-07-13

## Conclusions

The intermediate feedback was valid and exposed systemic gaps rather than isolated polish issues.
Current source and qualification evidence show that the most visible regressions have since been
implemented: panel edge/page navigation, Shift and non-contiguous selection, resource-kind
markers, repeated-key handling, direct PTY output, default-application opening, mounted-volume
denial, exact collision-safe Trash restoration, settings back navigation, and emergency quit.

That does **not** make the original production objective complete. The current dirty-source local
wave qualification passes 32 gates and 456 tests with no failures, but `project/far-parity.toml` still
records 14 partial items. Exact-revision macOS evidence now proves the disposable mount and private
consumer scenarios while 50 direct scenario/terminal combinations remain pending; Linux operator
evidence is absent. Production qualification additionally requires complete macOS and Linux
operator evidence, every parity item verified, and release artifacts from the qualified revision.

The correct current interpretation is therefore:

- **Implemented and freshly prequalified** means the complaint has direct current code plus focused
  and/or real-PTY evidence.
- **Implemented but operator-unverified** means the relevant behavior exists, but the terminal or
  platform matrix required by the production plan is incomplete.
- **Partial** means the narrow complaint is improved, but the broader Far workflow named by the
  parity record is not yet fully evidenced.

## Feedback Matrix

| Intermediate feedback | Current assessment | Direct evidence | Remaining gap |
|---|---|---|---|
| Qualification work consumed cycles without visible functionality | **Infrastructure works, production claim still unavailable.** The current dirty-source local wave result passes 32 gates and 456 tests. | `.near/qualification/qualification.json`; `tools/qualify.py`; `specs/qualification.toml` | Current operator evidence is incomplete; 14 parity items remain partial. Qualification is necessary for safety and regression control, but it is not a substitute for completing the direct interaction matrix. |
| Left/Right should jump to top/bottom like Far | **Implemented and PTY-prequalified.** | `crates/near-ui/src/workspace.rs:14869`; `.near/qualification/tmux-terminal-workflows.json` assertions for edge navigation | Direct Terminal.app/iTerm2/Ghostty/GNOME/Konsole operator matrix remains pending under `OP-PANEL-NAVIGATION-001`. |
| Shift+Arrow did not highlight | **Implemented with semantic rendering evidence.** | `crates/near-ui/src/workspace.rs:15088`; `crates/near-ui/tests/interaction_kernel.rs` | Full operator matrix remains pending; numeric-keypad selection remains explicitly undeclared. |
| Plain arrows could not create non-contiguous selections after Shift selection | **Implemented and real-PTY prequalified.** | `crates/near-ui/src/workspace.rs:15117`; `.near/qualification/tmux-terminal-workflows.json` non-contiguous-selection assertion | Same operator-matrix limitation as direct selection parity. |
| Home/End/PageUp/PageDown did not navigate | **Implemented and real-PTY prequalified.** | `crates/near-ui/src/workspace.rs:15012`; `.near/qualification/tmux-terminal-workflows.json` Home/End/PageDown assertions | Direct operator walkthrough remains pending across declared terminals. |
| Files and folders looked identical | **Implemented and visible without color.** | `crates/near-ui/src/collection.rs:1746`; `.near/qualification/tmux-terminal-workflows.json` directory-marker assertion | More resource classes still require operator inspection, but ordinary file/directory distinction is proven. |
| Multiple Near processes consumed approximately one core each | **Runaway regression fixed.** Fresh measurement is 0.240% of one core for idle `near-fm`; hangup exits in about 0.27 seconds. | `.near/qualification/logs/idle-cpu.log`; `crates/near-terminal/src/reactor.rs`; `tools/test_idle_cpu.py` | The architecture review still calls for lower wake/resource cost and a smaller coordinator; the critical 100% CPU behavior is not present in current qualification. |
| Held/repeated navigation keys were ignored | **Navigation repeat is fixed; the reassessment also corrected an overbroad first fix.** Enhanced repeats now dispatch only for single-stroke navigation keys by default, bindings can explicitly opt in or out, protected actions remain press-only, and repeats cannot alter pending chords. | `crates/near-ui/src/keymap.rs`; `crates/near-ui/tests/keymap.rs`; `crates/near-ui/src/workspace.rs`; `crates/near-terminal/src/input.rs` | `FAR-PLAT-001` and `FAR-MENU-002` remain partial until press/repeat/release and modifier behavior is recorded across the declared terminals. |
| Bottom terminal was not a real terminal; output appeared in a viewer | **Core complaint fixed, broader shell parity partial.** Panel command entry and `Ctrl+O` share one persistent PTY; output remains terminal output and REPL state survives hide/show. Reassessment exposed and fixed command-line Space routing, previously inert close-policy settings, invisible close warnings, and child exits that did not wake the blocking reactor. The real-PTY precheck now exercises a live Python REPL plus `warn`, `keep-open`, and `close` lifecycle decisions. | `crates/near-pty/src/lib.rs`; `crates/near-runtime/src/lib.rs`; `crates/near-ui/src/workspace_terminal.rs`; `crates/near-ui/src/terminal_surface.rs`; `docs/near-embedded-pty.md`; `.near/qualification/tmux-terminal-workflows.json` shell assertions | Full nested-application, SSH, focus, paste, resize, native-profile startup, and terminal matrix remains operator-pending. `FAR-SHELL-001`, `FAR-SHELL-005`, and `FAR-SHELL-006` remain partial. |
| Menu structure differed from Far | **Structurally corrected and real-PTY prequalified, exact parity still partial.** F9 opens the active side menu directly, exposes the five-category bar, Left/Right traverse all five categories, and Tab/Shift+Tab switch directly between panel menus. | `project/evidence/menu-action-assessment.md`; `crates/near-ui/src/workspace.rs`; `specs/menu-actions.toml`; `.near/qualification/tmux-terminal-workflows.json` menu assertions | `FAR-MENU-001` remains partial until current macOS and Linux operator workflows confirm ordering, accelerators, nesting, and last-menu behavior. |
| Some menu entries did nothing | **Static inert actions are prevented.** Activation must produce an effect or an explicit denial; disabled choices retain the menu and explain why. | `crates/near-ui/src/workspace.rs:10651`; `tools/validate_menu_actions.py`; `project/evidence/menu-action-assessment.md` | Static-route proof is not semantic proof of every dynamic/provider workflow. Operator inspection remains necessary. |
| Enter on a file opened the rudimentary viewer instead of the right application | **Narrow complaint fixed and real-PTY prequalified.** Enter uses the configured Open association; F3 remains the internal viewer; a separate process without an Open handler reports an explicit denial and does not fall back to the viewer. | `crates/near-ui/src/workspace.rs:19520`; `.near/qualification/tmux-terminal-workflows.json` association success and denial assertions | Full association editing, ordering, conditions, and platform-native operator proof remain partial under `FAR-AUTO-002`. |
| Temporary Panel mechanism was missing | **Implemented and real-PTY prequalified, not parity-verified.** Ten slots, copy-as-reference, source-provider action dispatch, contextual F7 removal, exact source reveal, UTF-8 interchange, `tmp:` modes, command capture, labeled menus, safe/full modes, and slot isolation exist. | `crates/near-ui/src/workspace.rs`; `docs/near-temporary-panels.md`; `.near/qualification/tmux-terminal-workflows.json`; `specs/operator-workflows.toml` `OP-TEMP-PANEL-001` | Direct current-revision macOS/Linux operator evidence is absent. `FAR-EXT-004` and `ABST-FAR-002` remain partial for that reason. |
| Deleting `/Volumes/Extreme Pro` did nothing | **Safety behavior implemented; deletion is intentionally denied.** Mounted roots never reach operation planning and the UI points to the Hotplug/device workflow instead. Ordinary Trash operations now record the platform-selected collision-safe destination and Files → Restore last Trash plans the exact original destination with explicit conflict handling. | `crates/near-ui/src/workspace.rs`; `crates/near-local-fs/src/lib.rs`; `crates/near-ops/src/lib.rs`; `.near/qualification/macos-mount-safety.json` | The expected mounted-root behavior is a visible denial, not deletion. The user-visible eject/disconnect workflow still needs current direct operator evidence on removable hardware or a capability-equivalent fixture. |
| Settings/menu choices were clunky, ambiguous, inert, and had no Back path | **Specific failures fixed.** Categories filter correctly, Enter toggles or edits, value dialogs show valid choices, disabled items explain denial, and Esc restores parent overlays. | `crates/near-ui/src/settings_surface.rs`; `crates/near-ui/src/workspace_settings_ui.rs`; `crates/near-ui/src/workspace.rs:20012`; fresh terminal PTY settings workflow | `FAR-CUSTOM-004` remains partial until the complete layer/provenance/reset/reload/rollback/restart operator scenario is recorded. Broader menu ergonomics still require direct review. |
| Command palette accepted no ordinary text | **Discovered during this reassessment and fixed.** Plain text, paste, Backspace, selection-search input, and activation now share one filtering path; the real PTY settings workflow enters through the palette. | `crates/near-ui/src/workspace.rs`; `tools/test_tmux_terminal_workflows.py`; `specs/abstraction-ownership.toml` `ABST-PALETTE-001` | Direct terminal-matrix evidence remains part of the general surface-navigation operator workflow. |
| Settings could trap the operator | **Implemented with a non-removable Escape path.** | `crates/near-ui/src/workspace.rs:1991`; `crates/near-ui/src/workspace.rs:20060` | A truly wedged process cannot process keyboard input; OS signal/kill remains the final recovery mechanism. |
| There was no hard quit | **Implemented and real-PTY verified.** `Ctrl+Alt+Q` bypasses keymaps, overlays, terminal routing, and unsaved-editor blocking, then exits through terminal restoration. | `crates/near-ui/src/workspace.rs:1978`; `crates/near-ui/src/workspace.rs:17293`; `tools/test_tmux_terminal_workflows.py`; `.near/qualification/tmux-terminal-workflows.json` | This is emergency data-loss behavior by design. `F10` remains the protected quit. |

## Current Unresolved Issue Register

### IF-001 — Operator evidence is incomplete and platform-bound

- **Code-backed finding:** exact-revision macOS evidence records the disposable mount and private
  public-consumer scenarios as passed with hashed artifacts.
- **Observed status:** `tools/operator_session.py status` reports 50 pending macOS
  scenario/terminal combinations, and no Linux operator evidence is available locally.
- **Impact:** No production declaration, and no promotion of terminal-sensitive parity records from
  partial to verified.
- **Required closure:** Record the complete macOS matrix at one candidate revision, repeat the
  required Linux matrix, and rerun production qualification against those exact artifacts.

### IF-002 — Fourteen Far parity records remain partial

The current partial records are `FAR-SHELL-001`, `FAR-SHELL-005`, `FAR-PANEL-001`,
`FAR-PANEL-007`, `FAR-SELECT-001`, `FAR-MENU-001`, `FAR-MENU-002`, `FAR-CUSTOM-004`,
`FAR-AUTO-002`, `FAR-EXT-004`, `FAR-PLAT-001`, `FAR-SHELL-006`, `FAR-VIEW-005`, and
`FAR-EDIT-005` (`project/far-parity.toml`). Several narrow complaints inside these records are
implemented, but the broader acceptance sets are not fully evidenced.

### IF-003 — Temporary Panel lacks direct operator proof

- **Code-backed finding:** model, rendering, bindings, workflow specification, and a current
  revision-bound real-PTY precheck now execute slot switching, copy-as-reference, F3 source action,
  F7 removal, and source reveal while checking the source files remain unchanged.
- **Current implementation:** UTF-8 list import/export, provider-qualified identity round trips,
  `tmp:` slot/safe/any/replace options, safe-mode denials, arbitrary command-text rows, exact
  asynchronous reveal focus, asynchronous command-output ingestion, labeled file-list menus, and
  full-panel mode, and retained stale metadata now have model, render, application, public
  collection-focus, and real tmux PTY regressions.
- **Missing evidence:** direct current-revision macOS/Linux operator recordings.
- **Closure:** record `OP-TEMP-PANEL-001` on macOS and Linux at the candidate revision before
  promoting `FAR-EXT-004`.

### IF-004 — Exact Far menu fidelity remains unproven

- **Code-backed finding:** hierarchy and inert-action regressions are substantially corrected; the
  current real-PTY precheck traverses all five top-level categories and switches directly between
  Left and Right panel menus with Tab and Shift+Tab.
- **Missing evidence:** current operator recordings for all top menus, accelerators, category
  traversal, nested Back behavior, dynamic applicability, and documented intentional differences.
- **Closure:** execute `OP-SURFACE-NAVIGATION-001` and the menu portion of
  `OP-FAR-PARITY-001` across required terminals.

### IF-005 — Full terminal claim exceeds current operator evidence

- **Code-backed finding:** persistent PTY, shared command line/user screen, scrollback, input,
  resize, alternate-screen parsing, and nested-app tests exist. The current real-PTY precheck also
  proves command-history Home/End/PageUp/PageDown, Help edge/page navigation, Tasks command
  routing, and Escape recovery in the built binary.
- **Missing evidence:** complete Terminal.app, iTerm2, Ghostty, tmux, GNOME Terminal, Konsole, and
  scheduled SSH combinations at one candidate revision.
- **Closure:** complete `OP-INPUT-RESTORATION` and `OP-SHELL-001`; retain explicit degradation
  records where protocols cannot report modifier release or keypad identity.

### IF-006 — Production gates intentionally remain closed

- **Code-backed finding:** wave qualification passes, but production requires clean source,
  parity-complete, interaction-complete, operator evidence, exact-revision `NearTuiProof`, release
  packages, SBOM, checksums, and provenance (`specs/qualification.toml`).
- **Closure:** finish IF-001 through IF-005, pin and validate the private consumer, then run clean
  macOS and Linux production qualification.

### IF-007 — Repeat dispatch required a safety-policy correction

- **Discovery:** the first repeat repair normalized every enhanced `Repeat` event to `Press`, so a
  held destructive or dialog-opening binding could dispatch repeatedly even though the reported
  panel-navigation symptom appeared fixed.
- **Correction:** reusable keymap resolution now defaults repeatability only for single-stroke
  navigation keys, supports explicit per-binding opt-in or opt-out, ignores releases, and leaves
  pending chords unchanged when unrelated repeats arrive.
- **Evidence:** protected-action, explicit-opt-in, pending-chord, panel integration, real tmux PTY,
  Clippy, and developer qualification checks pass.
- **Remaining gap:** direct terminal recordings are still required before closing the platform and
  menu parity records.

### IF-008 — Operator probes must share production terminal lifecycle

- **Discovery:** a probe abandoned after terminal-control experimentation remained alive and used
  one full core because it polled terminal input outside the production reactor.
- **Correction:** `near-input-probe` now uses `TerminalEventReactor`, including the same signal,
  deadline, PTY-disconnection, and restoration path as application runtimes.
- **Evidence:** `tools/test_idle_cpu.py` now covers `near-input-probe` idle CPU, SIGTERM, attached
  hangup, and detached-terminal closure in addition to `near-fm` and `near-view`.
- **Systemic rule:** every interactive binary and operator utility must use the shared reactor;
  standalone terminal polling is not an acceptable lifecycle implementation.

### IF-009 — Legacy Ctrl+Shift letter bindings need honest fallbacks

- **Discovery:** the real tmux editor corpus sends `Ctrl+Shift+B` for persistent blocks, but a
  legacy terminal encodes it identically to `Ctrl+B`; Near therefore runs the ordinary block-toggle
  command instead of the persistent-block command.
- **Current workaround:** the persistent-block command remains reachable through the command
  palette and is exercised there by the local PTY corpus. This does not validate the ambiguous
  binding.
- **Impact:** any `Ctrl+Shift+letter` binding can be misleading in a legacy session unless an
  enhanced keyboard protocol reports the Shift modifier distinctly.
- **Required closure:** terminal-adapter capability reporting must let keymap presentation select a
  declared legacy fallback, and conformance must reject or visibly degrade colliding bindings.

## Native Shell Dock Follow-up — 2026-07-16

- **Reassessment:** the former bottom command strip was a Near-owned buffer with synthetic prompt,
  history, and completion. Although Enter reused the persistent PTY, pre-submit interaction was not
  the native shell and did not meet the stated integration bar.
- **Implemented direction:** the account shell now starts with the workspace; a three-row dock
  projects that PTY under the panels; text, paste, line-editing keys, history, completion, reverse
  search, and shell widgets route to the shell; Enter promotes the same session to the user screen.
- **Remaining gap:** the scene contract currently flattens VT cells to text and therefore loses
  per-cell ANSI colors and modifiers. Styled-cell projection and direct operator qualification of
  real zsh/bash plugin configurations remain required before native-shell fidelity can be verified.

## Evidence Chain and Uncertainty

### Direct code-backed findings

- Current wave qualification: `.near/qualification/qualification.json`.
- Current real-PTY precheck: `.near/qualification/tmux-terminal-workflows.json`.
- Fresh disposable macOS mount fixture: `.near/qualification/macos-mount-safety.json`.
- Current parity state: `project/far-parity.toml`.
- Current discovery limitations: `project/evidence/interaction-discovery-inventory.md`.

### Analytical inferences

- The original cluster of missed navigation, selection, display, menu, and recovery behavior was
  caused by incomplete interaction-grammar coverage, not by one faulty key binding. This is the
  strongest explanation because the fixes required reusable navigation, selection, semantic
  rendering, menu applicability, overlay history, terminal lifecycle, and operator workflow
  changes across multiple owners.
- Fresh automated and tmux evidence substantially reduces regression risk but cannot substitute for
  direct terminal observations where escape sequences, modifier release, shell startup files,
  native associations, mount behavior, and nested applications vary by host.

### Alternate hypotheses considered

- **“The complaints were only caused by running an old binary.”** An old process explained the
  missing new footer during one settings report, but does not explain the earlier navigation,
  selection, CPU, PTY, association, or mount failures; each required source changes and regressions.
- **“The wave pass means everything is complete.”** Rejected because the wave profile permits
  partial parity and does not require current operator evidence or the private proof repository.
- **“Partial parity means the narrow complaints are still broken.”** Rejected where current direct
  code and PTY evidence proves the narrow behavior; partial applies to the broader acceptance set.

### What would falsify this reassessment

- Reproducing any row marked implemented on the freshly built candidate using its stated workflow
  would reopen that issue and require a minimal reproduction plus class-level regression.
- Current-revision operator recordings completing the required terminal matrices would allow the
  corresponding partial parity records to be reconsidered for verification.
- A production qualification result on clean macOS and Linux candidates with every mandatory gate
  and artifact current would supersede the unresolved register above.
