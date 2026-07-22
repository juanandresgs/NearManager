# Near Embedded PTY

`near-pty` isolates native pseudo-terminal lifecycle and VT interpretation from the backend-independent UI model. It exposes the cloneable `PtySessionHandle`; `near-ui` aliases that public contract as `EmbeddedTerminalSession` and supplies `EmbeddedTerminalSurface` through its additive `embedded-pty` feature, which is disabled by default. `near-fm` enables that feature explicitly and can still disable sessions at runtime with `NEAR_EMBEDDED_PTY=0`.

Sessions use the versioned `ShellProfile` contract. An explicit program overrides platform
resolution; otherwise macOS resolves the account shell with `dscl`, Linux uses `getent`, and the
environment then supplies the fallback. `platform-default` launches a login shell on macOS and an
interactive shell elsewhere. Login, interactive, clean, explicit arguments, startup command,
environment inheritance, and close policy remain inspectable settings.

## Structure

- `PtySession` owns the native PTY master, child killer, writer, reader thread, and exit observer.
- `PtySessionHandle` is the reusable persistent shell controller shared by command-line submission and user-screen rendering. Every profile spawn captures the resolved program, mode, arguments, environment policy, and close policy for that session.
- `vt100::Parser` supplies bounded screen and scrollback state, cursor position, alternate-screen state, application-cursor mode, bracketed-paste mode, indexed/RGB colors, text attributes, combining content, and wide-cell geometry.
- An incremental OSC scanner records the latest valid OSC 7 `file://` URI without interpreting it as a local filesystem authority decision.
- `EmbeddedTerminalSurface` is a view/controller adapter over the shared session. It translates semantic surface events and typed commands into PTY bytes, resizes from the rendered inner rectangle, and projects a backend-independent scene.
- Close lifecycle is explicit: `warn` presents terminate, keep-running/hide, and cancel decisions for a running child and retains completed output, `keep-open` hides the screen while preserving the same child, and `close` terminates on explicit close and removes the user screen when the shell exits. PTY child exit wakes the blocking reactor immediately. Dropping the workspace still kills any retained child. Host terminal restoration remains owned by `near-terminal`, not by the embedded child.

## Far Workflow

- Near starts the persistent account shell with the workspace and docks a cropped viewport of that
  actual PTY under the panels. The shell owns its prompt, startup files, line editing, history,
  reverse search, completion, widgets, quoting, and bracketed paste; Near no longer renders a
  synthetic provider-URI prompt or performs competing completion while the dock is active.
- While panels are visible, an unbound `Alt+letter` starts filename lookup before the dock can
  consume it, even when the shell has a draft. Cancelling lookup restores that draft unchanged;
  bound chords and full-screen terminal applications retain their declared input ownership.
- The dock retains three rows around the native shell cursor so multiline prompts and bounded
  completion feedback remain visible. Enter promotes the same session to the full user screen;
  Ctrl+O returns to the panels without replacing or restarting it.
- Enter submits the line already owned by the persistent shell and activates the user screen. Output is terminal output; it is never converted to a viewer.
- Subsequent command-line submissions and interactive REPL input use the same shell process, preserving its working directory, environment, functions, jobs, prompt, and transcript.
- `Ctrl+O` creates the same shared session when needed, then toggles between that retained screen and the prior panels, viewer, or editor without restarting the shell.
- The retained user screen participates in F12 screen selection and Ctrl+Tab/Ctrl+Shift+Tab cycling. Its PTY, scrollback, working directory, and nested application remain live while hidden.
- Text, paste, Backspace, Enter, Tab, Escape, arrows, Home, End, and Delete are forwarded to the child.
- `Ctrl+C` sends the terminal interrupt character; `Ctrl+D` sends end-of-file; `Ctrl+L` clears.
- `PageUp` and `PageDown` move through parser scrollback. `Ctrl+Shift+C` enters copy mode.
- `Ctrl+Shift+Q` applies the captured close policy; `Ctrl+O` is the explicit keep-running/hide path. The shell border displays the resolved shell mode and close policy. Escape is intentionally delivered to nested programs such as Vim rather than closing the screen.

The Commands menu opens the terminal-tab workspace menu; `near.terminal.open` remains the classic
toggle command. See [Near Terminal Workspaces](near-terminal-workspaces.md) for tab, peer-pane,
focus, and reversible zoom behavior. The static `TerminalSurface` remains available as a
backend-independent catalog and test fixture. Backend-independent tests prove that output survives
redraw and hide/show cycles, suspended full-screen viewers restore intact, and retained terminals
appear independently in screen-list and cycle navigation.

## Optionality and Fallback

Applications that do not enable `near-ui/embedded-pty` compile without the native PTY adapters, `vt100`, or `near-pty`. Runtime disabling keeps the explicit non-interactive `CommandLineExecutor` fallback. Provider locations without a native working directory are denied rather than silently executing in Near's process directory. External typed handlers and the suspend-and-run F4 workflow remain unchanged and independently tested.

## Compatibility Evidence

The automated suite verifies PTY input, resize, bounded scrollback state, alternate-screen entry and restoration, bracketed paste, Ctrl-C recovery, OSC 7 updates, child exit observation, shared command-line/user-screen state, persistent working directory across submissions, SSH client execution under a PTY, nested Vim restoration, and all three close policies for running and completed shells. It does not yet prove the full operator terminal matrix. Those remain explicit release gates.

The scene projection preserves terminal text, cursor position, indexed and true-color cell styling,
bold/dim/italic/underline/inverse attributes, combining content, wide-cell geometry, and native
input semantics. The local tmux workflow verifies styled Unicode output; the broader named-terminal
operator matrix remains a release gate.

The implementation uses [`portable-pty`](https://docs.rs/portable-pty/latest/portable_pty/) on Unix, [`conpty`](https://docs.rs/conpty/latest/conpty/) on Windows, and the shared screen model from [`vt100`](https://docs.rs/vt100/latest/vt100/struct.Parser.html).
