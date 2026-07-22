# M3 Embedded PTY Evidence

Date: 2026-06-23

## Implemented Slice

- `near-pty` provides a safe native PTY session with zsh spawn, raw input, bracketed paste, resize, scrollback, Ctrl-C, Ctrl-D, child termination, exit observation, VT snapshots, alternate-screen state, application-cursor state, and OSC 7 tracking.
- `near-ui/embedded-pty` provides a workspace-owned `EmbeddedTerminalSession` shared by panel command submission and `EmbeddedTerminalSurface`; the feature is disabled by default and enabled explicitly by `near-fm`.
- Enter on the panel command line writes into the same retained PTY shown by the user screen. Output remains VT terminal state rather than opening a captured-output viewer, and repeated submissions preserve shell working-directory and REPL state.
- `near-fm` creates or toggles a retained user screen with `Ctrl+O` or the main menu and supports nested-program key forwarding. `Ctrl+Shift+Q` explicitly closes the user screen without stealing Escape from nested TUIs.
- The user screen participates in F12 and Ctrl+Tab screen switching alongside panels and editors; hiding it preserves the PTY, output, scrollback, and working-directory state, while the prior viewer or editor screen is restored intact.
- `NEAR_EMBEDDED_PTY=0` disables spawning at runtime and reports that suspend-and-run handlers remain available.

## Automated Evidence

- `near-pty` tests verify output, resize, exit observation, alternate-screen parsing, OSC 7, bracketed paste, interrupt recovery, interactive zsh, SSH client execution, and nested Vim exit followed by continued shell use.
- `near-ui` all-feature tests verify separate panel command submissions share one shell and working directory, never create a viewer overlay, and render through the same user screen. They also verify the Far `Ctrl+O` binding opens, hides, and restores the embedded terminal and that the dedicated close binding removes it.
- A no-default-features build verifies `near-ui` remains usable without native PTY dependencies.
- The external-edit workspace test disables embedded PTY, verifies the fallback message, then proves the typed suspend-and-run invocation still preserves focus and selection.

## Requirement Status

`REQ-PTY-001` is verified for the macOS support matrix. Automated SSH coverage exercises the client under the interactive PTY without contacting a remote host; remote authentication remains an environment-specific smoke test rather than a deterministic repository test.
