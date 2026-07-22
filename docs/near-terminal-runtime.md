# Near Terminal Lifecycle and External Handoff

Near reserves `Ctrl+Alt+Q` as a keymap-independent emergency quit. It bypasses modal routing and
unsaved-editor protection, but still exits through the runtime loop so raw mode, keyboard
enhancements, and the alternate screen are restored. `F10` remains the normal protected quit.

Near owns terminal state through `near-terminal::TerminalSession`. Applications use the workspace runtime rather than issuing Crossterm lifecycle commands directly.

## Lifecycle Contract

Raw mode and the alternate screen are required. Keyboard enhancement, bracketed paste, mouse capture, and cursor hiding are optional enhancements: if any is unsupported, startup continues and the degradation is recorded in `TerminalDiagnostics`. `TerminalDiagnostics::keyboard_mode` explicitly reports `Legacy` or `Enhanced`.

Initialization applies transitions in this order:

1. Enable raw mode.
2. Enter the alternate screen.
3. Probe and push progressive keyboard flags when supported.
4. Enable bracketed paste when requested.
5. Enable mouse capture when requested.
6. Hide the cursor when requested.

Restoration always attempts the reverse order. A failure in one transition does not prevent later cleanup, and failed transitions remain active so `Drop` can retry them. Required initialization failures roll back state already applied. Rust panic unwinding therefore restores the session through the same path as normal exit.

## Common Signals

`TerminationWatcher` installs handlers for `SIGHUP`, `SIGINT`, `SIGQUIT`, and `SIGTERM`. Handlers only publish the signal number through an atomic value. The workspace polls input in bounded 50-millisecond intervals, observes the signal on its normal thread, drops the renderer, restores terminal state, and then returns `RunWorkspaceError::Terminated`.

This design avoids performing terminal I/O from an asynchronous signal handler.

## Structured External Tools

`near-core::ExternalInvocation` represents a process as an executable, an exact argument vector, an optional working directory, and an explicit environment. No shell parsing or interpolation occurs.

`ExternalToolResolver` maps a semantic action and `ResourceRef` to that invocation. The macOS local resolver preserves exact path bytes as one `OsString` argument, including whitespace, newlines, shell metacharacters, and non-Unicode bytes.

The default Far editor resolver uses `$VISUAL`, then `$EDITOR`, as an executable path. Without either variable it falls back to `/usr/bin/open -W -t`.

## Suspend and Resume

F4 asks the configured resolver for an edit invocation. The workspace retains focus, cursor, selection, navigation, and history state while the runtime:

1. Drops the Ratatui terminal backend.
2. Restores the host terminal.
3. Runs the child attached to the original standard streams.
4. Captures its exit status.
5. Re-enters Near terminal mode.
6. Recreates the renderer and redraws the unchanged workspace.

Re-entry is attempted even when process launch or waiting fails. Child exit codes are reported in the workspace status line.

## Verification

Unit tests cover normal restoration, required initialization rollback, optional-capability degradation, cleanup after restoration failures, panic unwinding, successful and failed handoffs, child exit status, and signal notification.

macOS PTY integration tests use `/usr/bin/script` to drive the real `near-fm` binary through F4 and F10. They run Vim and, when installed, Neovim; assert repeated workspace content before and after the child; and verify alternate-screen leave/re-entry sequences. A live SIGTERM smoke test additionally verified cursor, paste, and alternate-screen restoration before `Terminated(15)` was returned.

Keyboard negotiation requests escape-code disambiguation, event types, and alternate-key reporting. Unsupported terminals retain deterministic legacy normalization, while enhanced events preserve press, repeat, and release kinds. Fixtures cover Escape, Alt, function keys, repeat/release, and distinct Tab versus Ctrl-I input.

Legacy terminals and tmux may split one escape-prefixed key across multiple reads. The shared Unix
reactor holds a plain Escape for a bounded 25-millisecond disambiguation window and coalesces known
CSI and SS3 function, navigation, modifier, focus, and Alt sequences before dispatch. Unknown or
incomplete suffixes are replayed in order, so adapter recovery cannot silently discard input. The
real tmux workflow deliberately sends F9 as separate `Escape` and `[20~` writes and requires the
menu to open without leaking suffix text into the command line.

tmux's legacy input parser also delays a standalone Escape according to its server `escape-time`.
Near reports a tmux legacy-input degradation requiring `set -sg escape-time 10`; the mandatory
tmux operator profile records that effective value. This is a transport capability requirement,
not an application key-binding workaround.

Mouse normalization preserves coordinates, buttons, modifiers, drag phases, movement, and horizontal or vertical wheel direction. See [Near Mouse Interaction](near-mouse-interaction.md) for workspace routing and cross-panel transfer policy.
