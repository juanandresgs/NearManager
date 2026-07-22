# Near

Near is a Rust platform for cohesive keyboard-first terminal applications, with Near FM as its Far-inspired proving application. The platform keeps reusable commands, surfaces, terminal lifecycle, provider contracts, and interaction mechanics separate from file-manager policy.

Near is pre-release software. macOS is the primary proving platform; Linux and Windows have native build and adapter coverage, but platform-specific operator qualification may lag the current source revision. Do not infer production readiness from a successful build alone.

## Install

Download a verified archive from the [latest release](https://github.com/juanandresgs/NearManager/releases/latest), or build `near-fm` from a public checkout:

```sh
git clone https://github.com/juanandresgs/NearManager.git
cd NearManager
cargo install --path apps/near-fm --locked
near-fm
```

Rust 1.88 or newer is required. See [docs/INSTALL.md](docs/INSTALL.md) for platform prerequisites, checksum/provenance verification, companion binaries, and a clean-checkout validation path.

Near is dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) before contributing or reporting a vulnerability.

`F10` performs the normal protected quit. `Ctrl+Alt+Q` is a reserved emergency quit that
discards unsaved editor changes while still returning through Near's terminal-restoration path.

## Start Here

1. `project/README.md` — normative project-definition system and governance.
2. `docs/near-project-codification.md` — researched methodology for requirements, architecture, traceability, and completeness.
3. `docs/near-platform-blueprint.md` — product thesis, abstractions, architecture, technology decisions, macOS strategy, testing, risks, and phased roadmap.
4. `docs/near-implementation-plan.md` — dependency-ordered engineering epics and acceptance gates.
5. `docs/near-application-authoring.md` — how file managers and other applications should consume the platform.
6. `docs/near-surface-api.md` — public backend-independent scene, surface, and shell API.
7. `docs/near-testkit.md` — deterministic clocks, fake providers, and scripted semantic workflows.
8. `docs/near-local-filesystem.md` — exact-byte macOS filesystem identity, metadata, paging, and bounded reads.
9. `docs/near-operations.md` — immutable operation plans, execution, conflicts, recovery, and current runtime limits.
10. `docs/near-terminal-runtime.md` — fail-safe terminal lifecycle, signals, and structured external-tool handoff.
11. `docs/near-streaming-viewer.md` — bounded provider windows, text/hex modes, search, bookmarks, and quick view.
12. `docs/near-internal-editor.md` — provider-neutral editing, safe save, undo/redo, search, selections, and clipboard behavior.
13. `docs/near-reference-providers.md` — process, search-result, and plugin-catalog identity and capability contracts.
14. `docs/near-task-runtime.md` — bounded cancellable workers and workspace completion delivery.
15. `docs/near-api-compatibility.md` — published crate stability, MSRV, features, deprecation, and SemVer policy.
16. `docs/near-search.md` — versioned predicates, provider-neutral recursive execution, streaming, cancellation, and actionable result collections.
17. `docs/near-handlers.md` — predicate-selected external tools, exact argv templates, explicit shell mode, and diagnostics.
18. `docs/near-command-prefixes.md` — provider and extension `prefix:arguments` routing.
19. `docs/near-descriptions.md` — configurable file catalogs and folder-description files.
20. `docs/near-configuration.md` — six-layer precedence, trust, provenance, migrations, and atomic reload.
21. `docs/near-macros.md` — semantic command recording, contextual replay, trust, and safety.
22. `docs/near-embedded-pty.md` — optional native shell sessions, VT state, controls, fallback, and current compatibility limits.
23. `docs/near-interaction-settings-gap-audit.md` — evidence-based reassessment of settings, input, shell, viewer, and editor gaps.
24. `docs/near-settings-input-terminal-development-plan.md` — macOS-first work packages, acceptance gates, and platform matrix.
25. `docs/near-modal-selection-search.md` — uniform Alt+typing selection search for menus and selectable modals.
26. `docs/near-production-autonomy-development-plan.md` — fault-derived safety, workflow, autonomous qualification, and TUI abstraction proof plan.
27. `docs/near-plugins.md` — isolated Component Model hosting, grants, limits, discovery, and WIT evolution.
28. `docs/near-performance.md` — deterministic latency workload, p95 budget, idle contract, and scheduled gate.
29. `docs/near-diagnostics.md` — correlated event model, JSON export, and default redaction policy.
30. `docs/near-accessibility.md` — non-color state, high-contrast, and static-motion verification contract.
31. `specs/keymap.toml` — proposed contextual keymap and command-binding format.
32. `specs/handlers.toml` — shipped versioned macOS handler rules.
33. `specs/user-menu.toml` — typed global and local user-menu automation.
34. `specs/descriptions.toml` — sidecar names, encoding, visibility, and update policy.
35. `specs/macros.toml` — shipped versioned semantic macro records.
36. `specs/panel-modes.toml` — built-in defaults and user-defined panel column layouts.
37. `specs/editor.toml` — current minimal editor settings schema, including persistent blocks.
38. `specs/theme.toml` — proposed semantic theme-role format and Far-inspired preset.
39. `specs/theme-terminal-native.toml` and `specs/theme-high-contrast.toml` — low-assumption and accessible presets.
40. `specs/plugin.wit` — versioned WebAssembly Component Model boundary.

## Far Research

- `docs/farmanager-3-research.md` — product and interaction overview.
- `docs/farmanager-3-hotkeys.md` — default keyboard language.
- `docs/farmanager-3-file-operations-and-search.md` — operations, search, histories, and safety behavior.
- `docs/farmanager-3-advanced-workflows.md` — panels, filters, associations, menus, templates, and custom workflows.
- `docs/farmanager-3-settings.md` — settings and configuration model.
- `docs/farmanager-3-luamacro.md` — macro and automation model.
- `docs/farmanager-3-bundled-plugins.md` — bundled extension patterns.
- `docs/farmanager-3-startup-and-environment.md` — startup, profiles, and environment.
- `docs/farmanager-3-coverage-audit.md` — research completeness audit.
- `docs/farmanager-3-visual-coverage.md` — screenshot-to-workflow coverage.
- `assets/farmanager-ux/` — 49-item source/provenance index; third-party media is intentionally not redistributed.

## Core Recommendation

Build the command/keymap/theme/runtime abstraction first, prove it with generic collection surfaces and a fake provider, then add a read-only local filesystem provider. Ship external interactive tools through suspend-and-run before attempting an embedded PTY. Extract a public platform only after `near-fm` and at least two other applications demonstrate the abstractions are not file-manager-specific.

## Interaction Laboratory

The repository includes the M0 interaction platform plus broad M1–M3 implementation slices: exact-byte macOS paths, portable metadata, cancellable paged panels, streaming viewing and search, operation planning and recovery, structured handlers, layered configuration, semantic macros, fail-safe external handoff, and an optional embedded PTY with VT state, resize, scrollback, alternate-screen recovery, bracketed paste, signals, and OSC 7 tracking. Qualification status is revision- and platform-specific; consult the generated qualification record rather than treating this feature inventory as a release claim.

Requirements remain governed by `project/requirements.toml`. M0 and the implemented M1 terminal, filesystem, resource, operation, viewer, security, and external-tool slices are verified. See the M1 evidence records under `project/evidence/`.

### Build and Run

Near currently requires Rust 1.88 or newer:

```sh
cargo run -p near-fm --locked
```

Useful bindings include `Up`/`Down`, `Home`/`End`, `Space`, `Tab`, `Backspace`, `Ctrl+U`, `Ctrl+Q`, `Alt+F7`, `F1`, `F3`–`F10`, and `Esc` in overlays. Ctrl+O creates or toggles a retained user screen from panels, viewers, editors, or the terminal itself; its PTY output survives while hidden and it participates in F12 and Ctrl+Tab screen switching. Plain text enters the command line; Enter writes it to the persistent embedded shell in the focused native panel location and opens the shared user screen without converting output into a viewer, Ctrl+E/Ctrl+X browse command history, Ctrl+Enter and Ctrl+Shift+Enter insert current or peer names, and Ctrl+Alt+Enter inserts every selected name with shell-safe quoting. Ctrl+[ and Ctrl+] insert focused or peer panel paths; their Shift variants insert the corresponding current resource path and visibly reject providers without native paths. Alt+F1 and Alt+F2 open metadata-rich location menus for the exact left and right panels; entries combine provider roots with native filesystem roots, home, mounted volumes, and platform mount directories, and Enter navigates that side without changing focus. Alt+F8 opens persistent searchable command history where Space locks entries and Delete clears unlocked entries. Alt+F11 opens persistent viewed/edited-file histories with filtering, locking, clearing, and unavailable-provider diagnostics. Ctrl+Shift+0–9 assign ten persistent folder shortcuts, Ctrl+0–9 open them, and Alt+F12 opens searchable lockable folder history that retains unavailable destinations and their errors. `history.toml` configures independent retention limits. Alt+character starts incremental filename lookup only when the effective keymap has no binding for that chord; typing narrows the prefix, repeating the Alt chord cycles matches, Enter accepts, and Escape restores the original cursor. Ctrl+T and Ctrl+L toggle the focused side between file, live tree, and live information panels without overlays. Ctrl+Q turns the passive side into quick view, tracks the active file cursor, and restores the previous passive panel type when closed. Alt+1–Alt+4 assign compact, medium, full, or metadata view mode to only the focused panel, while Alt+0 opens every built-in and custom mode from layered `panel-modes.toml`. Ctrl+Shift+S opens selection commands for include/exclude masks, same-name or same-extension groups, inversion, saved selection sets, and two-panel comparison. Ctrl+Shift+C directly opens configurable non-recursive folder comparison by name, size, and modified time; unique entries and newer or both changed copies become selections without changing files. Ctrl+G applies a typed command template to selected resources or the current resource. `{resource}`, `{name}`, and `{panel}` are resolver-quoted for sequential execution; batch mode requires `{resources}` and runs one process with one structured argument per source. Results identify every source, command, exit code, stdout, stderr, cancellation, and skipped invocation. Ctrl+F3–Ctrl+F11 select Far-style sort modes, Ctrl+F12 opens the complete sort menu, Shift+F11 toggles rule-defined sort groups, and Shift+F12 toggles selected-first ordering; reverse, numeric, and directories-first options remain in the sort menu. `highlighting.toml` composes masks, kinds, sizes, dates, hidden state, permissions, priority, inheritance, semantic roles, and visible marks. Every non-root collection begins with a navigation-only `..` entry; Enter opens it and Backspace navigates directly to the parent. F3 opens the streamed viewer; inside it F2 toggles wrap, F4 toggles hex, F7 searches, Shift+F7 and Alt+F7 move between matches, F8 cycles encoding, Alt+F8 goes to absolute, relative, percentage, or line positions, Alt+Left/Right traverse viewer history, and Alt/Ctrl+0–9 set or jump to bookmarks. Alt+F7 starts provider-neutral recursive name/content search from the panel workspace. F4 opens the full-screen internal UTF-8 editor for writable provider resources; Ctrl+S saves through the provider, Ctrl+Z/Ctrl+Y undo and redo, F7 searches, Shift+cursor marks stream blocks, Alt+Shift+cursor marks rectangular blocks, and Ctrl+C/Ctrl+X/Ctrl+V preserve the matching clipboard semantics. `editor.toml` and Ctrl+Shift+B control persistent blocks. F7 searches, Ctrl+F7 replaces through staged fields, Ctrl+Shift+F7 replaces all, and Ctrl+Alt+F7 opens navigable Find All results; Alt+R enables regex captures and Alt+P preserves replacement style. Multiple documents remain live while panels are active; F12 lists panels and editor screens, Ctrl+Tab cycles them, and provider-scoped cursor positions restore after restart. Alt+F4 invokes the default external edit handler, while F9 → File associations lists every ordered view, edit, and execute alternative with structured-argv or explicit-shell labeling. `F5`, `F6`, `F7`, and Trash open operation plans before mutation; Shift+F6 opens single or template-driven selected-resource rename with explicit target, conflict, and backup-recovery preview. Alt+F6 creates an explicitly typed hard, symbolic, or junction-equivalent link with provider preflight validation. Ctrl+A edits portable read-only state, Unix mode and ownership, timestamps, and recursive attribute plans. Enter confirms a preview. The function-key bar is generated from the active keymap. Workspace projection for enhanced modifier events exists, but real hold behavior and the legacy-terminal fallback remain partial and are tracked in the interaction gap audit.

Confirmation policy loads from `NEAR_CONFIRMATIONS`, `~/Library/Application Support/near/confirmations.toml`, or `specs/confirmations.toml`. Reversible and confirmable previews are configurable; destructive and high-impact safeguards cannot be disabled.

External handler rules load from `NEAR_HANDLERS`, `~/Library/Application Support/near/handlers.toml`, or `specs/handlers.toml`. Structured argv is the default; explicit shell handlers are visibly marked before execution. F9 → File associations exposes ordered view, edit, and execute alternatives, and `near.handler.diagnostics` shows every match and rejection in configuration order.

User-menu rules load from `NEAR_USER_MENU`, layered `user-menu.toml`, or the shipped spec. F2 opens global entries and Shift+F2 opens local entries; focused, peer, selected, and temporary-list metasymbols remain typed, while explicit shell entries are visibly marked and quoted.

Registered provider and extension prefixes intercept `prefix:arguments` before shell execution. The built-in `file:` route navigates native paths or file URIs; unknown and one-letter drive-like prefixes remain ordinary shell input. F9 → Command prefixes shows effective ownership and descriptions.

File descriptions load from layered `descriptions.toml`, render in description panel columns, and are edited with Ctrl+Z for selected/current resources. Local copy, move, rename, trash, and delete operations maintain sidecar entries. F9 opens configurable folder-description files in the internal viewer or editor.

Reusable panel filters load from layered `filters.toml`. Ctrl+Shift+F or F9 opens named include, exclude, force-include, and force-exclude filters built from mask groups and provider-neutral metadata predicates. Filters toggle independently per panel, refresh asynchronously, and mark the affected panel border with `*`.

F9 opens the Far-style Left, Files, Commands, Options, and Right hierarchy. Nested entries use visible `[X]` accelerators, share the normal command registry, disable unavailable actions with reasons, and target the named panel explicitly. See `docs/near-menu-hierarchy.md`.

F8 moves resources to the platform trash, Shift+Delete plans permanent deletion, and Ctrl+Shift+Delete offers 1–7 overwrite passes for writable regular files. Permanent delete and wipe always require two explicit high-impact confirmations; wipe dialogs document SSD, snapshot, and copy-on-write limitations. See `docs/near-deletion.md`.

Ctrl+Q opens passive quick view in the peer panel. Ctrl+Shift+Q temporarily gives it the standard viewer navigation, search, hex, encoding, bookmark, and history controls; Esc returns to file navigation. Directories render asynchronous provider summaries rather than an unavailable placeholder.

All `near-fm` documents share the layered configuration engine. Use `--keymap`, `--theme`, `--confirmations`, `--handlers`, or `--macros` for CLI overrides and `--trust-workspace` to opt into `.near/*.toml`. The searchable Settings surface exposes typed descriptors, effective values and origins, platform availability, validation, persistence, apply scope, and rollback behavior through the runtime configuration coordinator.

Semantic macros use `Ctrl+.` to start/stop recording and `Ctrl+Shift+.` to replay. Records contain command IDs and typed arguments, remain stable across key rebinding, and can be inspected with `near.macro.show-last`.

### Validate

The resumable qualification entry point persists evidence under `.near/qualification/`:

```sh
python3 tools/qualify.py developer
python3 tools/qualify.py wave
python3 tools/qualify.py production --no-resume
```

Gate definitions live in `specs/qualification.toml`; production qualification requires a clean
macOS or Linux checkout and emits `.near/qualification/qualification.json`.

Before direct operator qualification, generate the revision-bound fixture and environment pack,
then initialize and inspect the evidence checklist:

```sh
python3 tools/operator_session.py prepare --platform macos
python3 tools/workflow_evidence.py init --platform macos --operator "$USER" \
  --output .near/qualification/operator/macos/evidence.json
python3 tools/operator_session.py status \
  --evidence .near/qualification/operator/macos/evidence.json
```

Use `--platform linux` on Linux and follow
`docs/near-production-workflow-testing-guide.md` for the non-waivable terminal and parity matrix.

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/validate_project.py
```

The backend-independent custom-surface example runs with:

```sh
cargo run -p near-demo --locked
```

Capture normalized terminal protocol evidence without depending on the file-manager workspace:

```sh
cargo run -p near-input-probe --locked -- input.json "Terminal.app"
```

Exercise modified navigation, modifier press/release, focus, paste, resize, mouse, and keypad input,
then press `Ctrl+Q`. The JSON uses the public serializable `near-terminal` event and diagnostics
contracts consumed by qualification tooling and independent applications.

Backend-neutral applications can also be driven deterministically through
`near_testkit::ApplicationWorkflowHarness`. It accepts a public `near_app::Application`, normalized
keys, paste, manual time advances, and named semantic captures without importing `FarWorkspace` or
selecting a terminal backend.

The extracted standalone applications run with:

```sh
cargo run -p near-view --locked -- README.md
cargo run -p near-proc --locked
```

`near-view` also composes in pipelines: `cat README.md | near-view -` and `near-view file:///path` emit exact plain bytes when stdout is not a terminal.

Deterministic provider and scripted Far workflow tests run with:

```sh
cargo test -p near-testkit --locked
```
