# Settings, Input, Terminal, Viewer, and Editor Development Plan

## Outcome

Deliver a real, persistent, platform-aware settings system and correct the affected interaction workflows. macOS is the first fully verified platform; abstractions must remain suitable for Linux and Windows adapters.

## Definition of done

The work is complete only when all of the following are true:

1. Every settings category opens a typed, non-empty editor or an explicitly read-only platform report.
2. Changes validate, preview where relevant, apply transactionally, persist to the selected layer, survive restart, and expose provenance plus reset-to-layer behavior.
3. Alt+typing filename lookup works in recorded Terminal.app legacy input and enhanced keyboard input, including start, extend, cycle, accept, cancel, Unicode, and no-match behavior.
4. The key bar changes while modifiers are physically held when the terminal can report holds, and exposes a tested honest fallback when it cannot.
5. Embedded zsh follows a configurable native shell profile and loads the expected macOS startup files by default.
6. Viewer and editor defaults are fully configurable, while per-resource state remains distinct from global defaults.
7. Internal versus external view/edit policy is configurable and native macOS applications launch through a platform service.
8. The macOS terminal/application matrix and headless model suite both pass; Linux and Windows contracts have adapter tests even if their full matrices follow later.

## Architecture

### Typed settings registry

Add a settings registry independent of the help system. Each setting descriptor must define:

- stable ID and category;
- value type, constraints, default, and documentation;
- owning versioned document and field path;
- effective value and full layer provenance;
- mutability and restart requirement;
- preview/apply/rollback hooks;
- platform availability and degradation reason;
- security sensitivity and workspace-trust policy.

Settings surfaces render descriptors; help links to them but does not substitute for them. Writes target a selected writable layer, preserve unrelated TOML, validate the entire candidate, write atomically, and update `ConfigManager`. Reset removes the selected-layer override and reveals the next effective value.

### Runtime configuration coordinator

`near-fm` should own a coordinator for every resolved document. It provides initial resolve, watcher/manual reload, candidate validation, cross-document transaction ordering, last-valid rollback, diagnostics, and workspace application. Keymap, theme, shell, viewer, editor, panel, history, handler, and safety updates must declare whether they apply live, to newly opened surfaces, or after restart.

### Input capability model

Extend terminal diagnostics with explicit keyboard capabilities:

- legacy Escape-prefixed Alt;
- enhanced disambiguation;
- press/repeat/release;
- modifier-only events;
- alternate-key reporting;
- physical-key reporting.

Create byte/event fixture tests for Terminal.app legacy sequences and enhanced protocol events. Workspace input state combines protocol events through a dedicated modifier tracker and lookup state machine rather than consulting incidental fields in unrelated routing branches.

### Shell profiles

Add `shell.toml` and a provider-neutral `ShellProfile`:

- executable source: account login shell, environment shell, or explicit path;
- invocation: login interactive, interactive, clean interactive, or explicit argv;
- startup command and whether it runs inside the shell;
- environment inheritance plus explicit overrides/removals;
- working-directory policy;
- TERM/COLORTERM/terminfo policy;
- OSC 7/title integration mode;
- exit/close policy and process-warning policy;
- scrollback and paste behavior.

The macOS resolver should default to the account login shell and login-interactive semantics, matching the normal Terminal workflow. It must not silently use `-f`. Clean mode remains available for diagnostics and deterministic tests.

### Viewer and editor policy

Add `viewer.toml` and expand `editor.toml`. Separate three layers of state:

1. global defaults;
2. session overrides for currently open surfaces;
3. provider/resource state such as position, bookmarks, and last mode.

A setting declares which layer it affects. Opening a surface resolves defaults first, then permitted resource state. Resetting resource state must not change global defaults.

### Native application service

Introduce a platform application-opening service. On macOS it should use AppKit `NSWorkspace` semantics for default app, chosen app, activation, prompting, recent-items policy, arguments, and asynchronous completion. Generic structured argv handlers remain available for CLI/TUI tools. Internal/external viewer/editor preference and association precedence become typed settings.

## Work packages

### WP1 — Correct the specification and tests

- Downgrade unsupported parity claims to partial.
- Add explicit viewer-settings, editor-settings, native-shell-profile, and runtime-settings items.
- Replace file-presence evidence with workflow evidence.
- Add a requirement-to-test matrix covering terminal mode and platform.

Exit: validators report the honest partial state and every new requirement has a workflow.

### WP2 — Fix Alt+typing lookup

- Extract `FilenameLookupController` with explicit `Idle`, `Searching`, and accepted/cancelled transitions.
- Normalize legacy Escape-prefixed Alt and enhanced Alt-character events to the same semantic start action.
- Combine modifier-only state only when capability evidence permits it.
- Define Unicode case folding, repeated chord cycling, timeout policy, paste, no-match, panel switch, and overlay behavior.
- Add parser fixtures, workspace model tests, and macOS PTY interaction tests.

Exit: recorded Terminal.app and enhanced fixtures pass the same golden workflow.

### WP3 — Fix modifier key-bar layers

- Track modifier presses/releases with focus-loss and timeout recovery.
- Render exact held layers from the effective keymap.
- For legacy terminals, provide an explicit key-bar layer command/latch or keep the base layer with a visible diagnostic; do not infer a physical hold that cannot be observed.
- Add terminal capability reporting to About/Diagnostics.
- Test Shift, Control, Alt/Option, combinations, repeats, releases, focus loss, tmux, and terminals without enhancement.

Exit: real enhanced terminals change on hold; legacy behavior is deterministic and documented.

### WP4 — Build the settings platform

- Implement registry, descriptors, category navigation, form controls, validation, provenance, writable-layer selection, reset, apply, rollback, and atomic persistence.
- Wire `ConfigManager` into the running application.
- Add watcher plus manual reload and conflict handling for externally edited settings.
- Implement System, Panel, Tree, Interface, Dialog, Menu, Command-line, Completion, Confirmation, Viewer, and Editor category owners.
- Mark platform reports read-only only when no meaningful writable setting exists.

Exit: no category is empty; every editable value survives restart and every read-only value explains its source.

### WP5 — Native macOS zsh profile

- Resolve the account login shell, with explicit override support.
- Implement login-interactive zsh launch and verify `.zprofile`, `.zshrc`, locale, PATH, Homebrew paths, SSH agent, and working directory.
- Add optional Near shell integration for OSC 7/title without replacing user startup files.
- Add startup command, exit policy, child-process warning, and clean diagnostic profile.
- Verify resize, paste, signals, scrollback, alternate screen, nested Vim/Neovim/SSH, and return to panels in Terminal.app, iTerm2, and Ghostty; record tmux behavior.

Exit: the default embedded shell behaves like the user's native macOS shell configuration.

### WP6 — Viewer settings

Implement typed defaults for internal/external preference, association precedence, wrap and word-wrap, text/hex/dump selection, binary detection, encoding detection/default, tab width, maximum line width, control/NUL glyphs, scrollbar, overflow markers, persistent selection, saved positions, saved bookmarks, saved encoding, and saved view mode.

Exit: new viewers and quick view obey documented defaults; resource state overrides only fields configured to persist.

### WP7 — Editor settings

Implement typed defaults for internal/external preference, association precedence, tab preservation/expansion, tab width, automatic indentation, persistent blocks, delete-block behavior, whitespace/EOL display, cursor beyond EOL, search selection/cursor placement, scrollbar, line numbers, position/bookmark persistence, read-only warning/lock behavior, external-change policy, encoding detection/default, new-file encoding/BOM/EOL, and lossy-save policy.

Exit: new editors use the resolved policy, open editors receive only settings declared live, and restart persistence is verified.

### WP8 — Native application integration

- Add macOS application chooser/default application discovery.
- Separate GUI app launch from terminal child execution.
- Define wait/non-wait, activation, error, and workspace restoration policies.
- Preserve generic handler ordering and explain why a native app, internal tool, or structured command won.

Exit: external view/edit can use TextEdit, Preview, a chosen app, or a configured CLI/TUI without terminal corruption.

### WP9 — Cross-platform adapters

- Linux: login shell/passwd resolution, XDG config, desktop portals/default applications, common enhanced terminals.
- Windows: PowerShell/cmd profiles, ConPTY, ShellExecute/default applications, Windows Terminal input behavior.
- Keep settings schemas and semantic commands shared; expose platform-only fields through availability metadata.

Exit: adapter contract tests pass and unsupported features degrade with reasons.

## Verification matrix

### macOS terminals

- Terminal.app in default Option-key mode and Escape-meta mode;
- iTerm2 with legacy and CSI-u/kitty-style reporting where available;
- Ghostty enhanced input;
- tmux inside each supported host where practical;
- SSH session into a remote host for shell and key encoding checks.

### Input workflows

For each mode: Alt+typing lookup, repeated lookup chord, modifier key-bar hold/release, key sequences, Option-generated Unicode, paste, focus loss, resize, and external tool round trip.

### Settings workflows

For every descriptor: built-in value, platform override, user override, trusted workspace override, CLI override, invalid candidate rollback, external edit reload, reset-to-layer, restart persistence, and provenance display.

### Viewer/editor corpus

Empty, ASCII, UTF-8, UTF-16, Latin-1, invalid UTF-8, binary/NUL, huge line, huge file, mixed EOL, tabs, read-only, externally changed, provider-backed, and unavailable-resource fixtures.

## Sequencing and release gates

1. Specification correction and failing integration tests.
2. Input fixes, because they affect every settings surface.
3. Settings registry and persistence foundation.
4. Native shell profile.
5. Viewer and editor policy expansion.
6. Native application service.
7. macOS matrix completion.
8. Linux and Windows adapters.

No item returns to `verified` until its platform workflow, persistence behavior, failure behavior, and restart behavior are evidenced. Unit tests alone are insufficient for terminal protocol or native application claims.

## External references

- Apple Terminal default shell and profile behavior: https://support.apple.com/guide/terminal/trml113/mac
- Apple Terminal shell startup/exit profile settings: https://support.apple.com/en-lamr/guide/terminal/trmlshll/mac
- Apple `NSWorkspace.OpenConfiguration`: https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration
- Apple URL/application opening API: https://developer.apple.com/documentation/appkit/nsworkspace/open%28_%3Awithapplicationat%3Aconfiguration%3Acompletionhandler%3A%29
- zsh invocation and startup semantics: https://zsh.sourceforge.io/Doc/Release/Invocation.html
