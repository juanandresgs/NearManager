# M1 Keyboard Input Evidence

Date: 2026-06-23

## Implemented Slice

- `TerminalSession` probes Crossterm keyboard-enhancement support after entering required terminal mode.
- Supported terminals receive Kitty progressive flags for escape-code disambiguation, event types, and alternate keys.
- Enhancement push/pop participates in the same reverse-order, panic-safe, suspend/resume lifecycle as raw mode, alternate screen, paste, and cursor state.
- Unsupported terminals continue in deterministic `KeyboardMode::Legacy`; probe and push failures are recorded as optional degradation rather than blocking startup.
- `TerminalDiagnostics::keyboard_mode` reports the active mode explicitly.
- Normalized input retains press, repeat, and release kinds plus modifiers and disambiguated key codes.

## Automated Evidence

- Lifecycle tests prove enhanced mode is pushed after required setup and popped before leaving the alternate screen on normal exit, panic unwinding, and external-tool suspension.
- The unsupported-capability fixture proves startup succeeds, no enhancement push occurs, diagnostics report `Legacy`, and degradation names the fallback.
- Legacy input fixtures cover Escape, Alt-modified characters, and function keys.
- Enhanced fixtures cover repeat, release, and distinct Tab versus Ctrl-I events. The shared keymap runtime canonicalizes repeat events to their press binding for dispatch, so held arrows, paging keys, Home, and End navigate continuously while release remains non-dispatching.
- The full `near-fm` PTY handoff suite exercises terminal leave/re-entry around Vim and Neovim with keyboard negotiation enabled.

## Requirement Status

- `REQ-TERM-002` is verified. Capability diagnostics, deterministic legacy fixtures, and enhanced event fixtures directly cover every acceptance clause.
