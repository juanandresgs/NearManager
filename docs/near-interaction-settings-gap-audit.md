# Settings, Input, Shell, Viewer, and Editor Gap Audit

## Scope

This audit covers the reported empty settings experience, Alt+typing filename lookup failure, modifier-sensitive key bar failure, native zsh behavior, and configurable viewer/editor behavior. It also records adjacent functionality that the same earlier coverage method was likely to miss.

## Executive finding

Near has useful underlying mechanisms, but several were marked complete after unit-level or headless verification without proving the user-facing workflow on the primary macOS terminal matrix. The most important category error is that a searchable help/provenance catalog was treated as a settings implementation. Similar substitutions occurred elsewhere:

- a configuration merge engine was treated as live application configuration;
- a synthetic modifier event test was treated as proof that real terminals expose modifier holds;
- a PTY that starts `$SHELL` was treated as a native Terminal-compatible zsh profile;
- viewer/editor commands were treated as configurable viewer/editor defaults;
- internal and external view/edit mechanisms were tested separately without a user-configurable selection policy.

The current implementation therefore has strong primitives but incomplete product composition.

## Authoritative current-state evidence

### Settings

`FarWorkspace::effective_settings_surface` builds `HelpSurface` topics from static categories, commands, and resolver diagnostics. Most categories have no editable form, typed schema, validation, persistence, apply/rollback lifecycle, or restart verification. `System Settings` has no configuration documents. The only general settings dialog is the editor dialog, and `EditorSettings` contains only `persistent_blocks`.

The application resolves configuration documents once during startup. `near-config::ConfigManager` supports atomic reload in isolation, but `near-fm` does not own managers or file watchers and does not reapply changed settings to the running workspace.

### Alt+typing filename lookup

The lookup algorithm itself is deterministic and covered by headless tests. Starting lookup depends on the incoming character event carrying `stroke.modifiers.alt`; an independently tracked held Alt state is not consulted. On enhanced protocols a terminal may emit a modifier-only event separately from the character. On legacy terminals, Alt may be encoded as an Escape prefix and standalone press/release cannot be observed. The current tests construct the ideal `Alt+C` event directly and therefore cannot reproduce protocol-specific failures.

The implementation also conflates two different actions: repeated Alt+character cycles a one-character query, while subsequent unmodified typing extends it. This needs an explicit state machine and terminal-mode fixtures rather than incidental routing through unmatched-key handling.

### Modifier-sensitive key bar

The key bar correctly derives labels from the resolved keymap. Its alternate layer depends entirely on `held_modifiers`, which changes only for modifier-only press/release events. Terminal.app and other legacy keyboard modes cannot report standalone modifier holds. Even in enhanced mode, support must be negotiated and observed at runtime. The current test injects modifier-only events directly and proves only workspace projection, not terminal delivery.

A capability-aware UX is required:

- exact hold behavior when press/release events exist;
- a documented, non-invented fallback when they do not;
- visible terminal diagnostics explaining the active mode;
- real PTY/terminal fixtures for Escape-prefixed Alt and enhanced key events.

### Native zsh and terminal behavior

The embedded PTY now consumes a versioned `ShellProfile`, resolves the platform account shell, distinguishes platform-default, login, interactive, and clean modes, and captures environment, startup, and close policy per session. The user screen displays the resolved mode and policy. Warn, keep-open, and close behavior is enforced for running and completed children, so a process is never silently abandoned.

Near sets `TERM=xterm-256color` and `COLORTERM=truecolor`, tracks OSC 7, and supports nested full-screen children. Remaining gaps are direct startup-file evidence, shell-integration injection, title policy, terminfo validation, and the full compatibility matrix.

For macOS, the default must follow the user's login shell and preserve normal zsh startup files. A clean/no-rc mode should be explicit rather than the hidden basis of testing. Cross-platform code should consume a `ShellProfile` abstraction, with macOS, Unix, and Windows profile resolvers.

### Viewer configuration

`ViewerSurface` hard-codes initial wrap, hex, encoding, selection, and display behavior. Persisted per-resource state covers positions, bookmarks, mode, wrap, and encoding after use, but there is no `viewer.toml`, default policy, binary detection policy, tab width, line-width limit, scrollbar preference, visible control-character policy, or internal/external viewer preference.

### Editor configuration

`editor.toml` has one field. Missing defaults include tab policy and width, indentation, whitespace/line-ending display, cursor-beyond-EOL, match selection/cursor policy, line numbers, scrollbar, read-only warnings/locking, external-change policy, default encoding/BOM/EOL, code-page detection, position/bookmark retention, and internal/external editor preference.

### Native external application integration

The macOS default handlers use `/usr/bin/open -W`, which blocks until the launched application exits and treats native apps as command-line children. The platform should support native application selection and launch policy separately from terminal suspend-and-run. AppKit `NSWorkspace` exposes activation, recent-items, prompting, arguments, environment, and instance behavior; Near needs a platform service abstraction rather than embedding all behavior in generic argv handlers.

## Why the previous audit missed these gaps

1. **Acceptance criteria described exposure, not operation.** “Settings are exposed” allowed a help topic to satisfy a settings requirement.
2. **Evidence was file-presence based.** A source file or test name was accepted without checking whether it covered the full acceptance scope.
3. **Synthetic input replaced real protocol testing.** Directly constructed `TerminalEvent` values bypassed terminal negotiation and byte decoding.
4. **Mechanism and integration were conflated.** `ConfigManager`, PTY, handlers, and state stores existed, but the application did not compose their full lifecycle.
5. **The primary platform was not a release gate.** There is no recorded Terminal.app/iTerm2/Ghostty/tmux matrix for the exact workflows.
6. **Defaults were mistaken for settings.** Hard-coded viewer/editor behavior and runtime toggles were counted as configurable policy.
7. **Parity inventory stopped at feature verbs.** It captured “view,” “edit,” and “shell,” but not the preference, persistence, platform launch, startup, and failure-recovery systems around them.

The focused tests `incremental_filename_lookup_filters_cycles_and_restores_cursor`, `held_modifiers_project_alternate_function_key_hints`, and `settings_catalog_exposes_categories_search_provenance_and_live_editor_values` all pass in the current tree. That result is evidence of the coverage flaw, not evidence against the reports: they inject already-normalized ideal events or assert catalog text and one session-only field. None exercises terminal negotiation, raw legacy bytes, real modifier delivery, typed category editors, persistence, or restart behavior.

## Adjacent likely misses

The corrected coverage method must also inspect:

- runtime configuration watching, manual reload, validation errors, rollback, and persistence;
- keymap editing and conflict resolution, not only keymap loading;
- platform-native file/application association discovery and overrides;
- terminal diagnostics, terminfo presence, multiplexers, remote sessions, and Option-key modes;
- shell environment provenance, locale, PATH, Homebrew paths, SSH agent, and login-shell changes;
- viewer/editor font-independent tab and wide-character cell behavior;
- huge-line, mixed-EOL, binary, malformed-encoding, read-only, locked, and externally modified files;
- quick-view defaults versus full viewer defaults;
- settings import/export, schema migration, reset-to-layer, and restart durability;
- accessibility of every settings control without color or mouse;
- security boundaries for startup commands, workspace settings, external applications, and inherited environment.

## Platform adaptation principles

Near should preserve Far's workflow intent without copying Windows-specific implementation details.

- **macOS first:** account login shell, zsh startup semantics, Terminal.app legacy input, iTerm2/Ghostty enhanced input, AppKit application launching, native clipboard, Application Support paths, sandbox/bookmark constraints when applicable.
- **Platform-neutral core:** typed settings descriptors, transactional apply/rollback, semantic commands, terminal capability model, shell profiles, viewer/editor policy objects, external application service.
- **Adapters:** macOS AppKit/Unix PTY first; Linux XDG/portal and Windows ShellExecute/ConPTY follow the same contracts.
- **Degrade honestly:** unavailable modifier holds or native APIs must be reported as capabilities. Near must not display bindings that are not actually active.
