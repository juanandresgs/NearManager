use near_core::DiagnosticPhase;
use near_terminal::{
    TerminalEvent, TerminalEventReactor, TerminalRuntimeEvent, TerminalSession,
    TerminalSessionError, dimensions,
};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};

use crate::{
    FarWorkspace, Keymap, RunWorkspaceError, SemanticTheme, TerminalColorDepth,
    render_loop::RenderInvalidation, semantic::RoleBuffer,
};

/// Runs a workspace until it dispatches the quit command.
///
/// # Errors
///
/// Returns an error if terminal initialization, rendering, input, external handoff, signal
/// handling, or restoration fails.
pub fn run_workspace(
    mut workspace: FarWorkspace,
    theme: &SemanticTheme,
    mut keymap: Keymap,
) -> Result<(), RunWorkspaceError> {
    let runtime_theme = theme
        .clone()
        .with_depth(TerminalColorDepth::detect_from_environment());
    workspace.set_theme_depth(runtime_theme.terminal_depth());
    run_workspace_at_depth(&mut workspace, &runtime_theme, &mut keymap)
}

/// Runs a workspace with an explicitly configured theme color depth.
///
/// # Errors
///
/// Returns an error if terminal initialization, rendering, input, external handoff, signal
/// handling, or restoration fails.
pub fn run_workspace_at_depth(
    workspace: &mut FarWorkspace,
    theme: &SemanticTheme,
    keymap: &mut Keymap,
) -> Result<(), RunWorkspaceError> {
    workspace.set_theme_depth(theme.terminal_depth());
    workspace.record_terminal_session(DiagnosticPhase::Started);
    let mut session = TerminalSession::enter()?;
    workspace.set_keyboard_mode(session.diagnostics().keyboard_mode);
    let mut reactor = TerminalEventReactor::new()?;
    workspace.install_runtime_wake(reactor.wake_handle());
    let mut terminated = None;
    'runtime: while !workspace.should_quit() {
        let backend = CrosstermBackend::new(session.output_mut());
        let (columns, rows) = dimensions();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, columns, rows)),
            },
        )?;
        let mut roles = RoleBuffer::new(columns, rows, "workspace.background");
        let mut handoff = None;
        let mut redraw = RenderInvalidation::initial();
        while !workspace.should_quit() {
            redraw.request_if(workspace.poll_background_tasks());
            if redraw.take() {
                let effective_theme = workspace.effective_theme(theme);
                terminal
                    .draw(|frame| workspace.render(frame, &effective_theme, keymap, &mut roles))?;
            }
            let runtime_event = reactor.wait(keymap.time_until_pending_timeout())?;
            redraw.request_if(workspace.poll_background_tasks());
            let event = match runtime_event {
                TerminalRuntimeEvent::Terminal(event) => event,
                TerminalRuntimeEvent::Terminate(signal) => {
                    terminated = Some(signal);
                    break 'runtime;
                }
                TerminalRuntimeEvent::Wake => continue,
                TerminalRuntimeEvent::Timeout => {
                    if keymap
                        .time_until_pending_timeout()
                        .is_some_and(|timeout| timeout.is_zero())
                    {
                        workspace.handle_keymap_timeout(keymap);
                        redraw.request();
                    }
                    continue;
                }
            };
            if let TerminalEvent::Resize { columns, rows } = event {
                terminal.resize(Rect::new(0, 0, columns, rows))?;
                roles = RoleBuffer::new(columns, rows, "workspace.background");
            }
            workspace.handle_terminal_event(keymap, event);
            if let Some(source) = workspace.take_keymap_reload() {
                let result = keymap
                    .reload_from_toml_named("runtime keymap.toml", &source)
                    .map_err(|error| error.to_string());
                workspace.report_keymap_reload(result);
            }
            redraw.request();
            if let Some(invocation) = workspace.take_external_invocation() {
                handoff = Some(invocation);
                break;
            }
        }
        drop(terminal);
        if let Some(invocation) = handoff {
            match session.run_external(&invocation) {
                Ok(status) => workspace.report_external_exit(status),
                Err(TerminalSessionError::External(error)) => {
                    workspace.report_external_error(&error);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    workspace.clear_runtime_wake();
    session.restore()?;
    workspace.record_terminal_session(DiagnosticPhase::Completed);
    terminated.map_or(Ok(()), |signal| Err(RunWorkspaceError::Terminated(signal)))
}
