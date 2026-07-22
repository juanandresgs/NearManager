use std::{
    env,
    io::{self, Stdout, stdout},
    process::{Command, ExitStatus},
};

use crossterm::{
    ExecutableCommand,
    cursor::{Hide, Show},
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use near_core::ExternalInvocation;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TerminalSessionError {
    #[error("terminal I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("external tool failed: {0}")]
    External(#[source] io::Error),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct TerminalCapabilities {
    pub bracketed_paste: bool,
    pub cursor_visibility: bool,
    pub keyboard_enhancement: bool,
    pub mouse_capture: bool,
}

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self {
            bracketed_paste: true,
            cursor_visibility: true,
            keyboard_enhancement: true,
            mouse_capture: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum KeyboardMode {
    #[default]
    Legacy,
    Enhanced,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct TerminalDiagnostics {
    pub raw_mode: bool,
    pub alternate_screen: bool,
    pub bracketed_paste: bool,
    pub cursor_hidden: bool,
    pub keyboard_mode: KeyboardMode,
    pub mouse_capture: bool,
    pub degraded_features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Transition {
    EnableRawMode,
    EnterAlternateScreen,
    PushKeyboardEnhancement,
    EnableMouseCapture,
    EnableBracketedPaste,
    HideCursor,
    ShowCursor,
    DisableBracketedPaste,
    DisableMouseCapture,
    PopKeyboardEnhancement,
    LeaveAlternateScreen,
    DisableRawMode,
}

trait TerminalControl: Send {
    fn apply(&mut self, transition: Transition) -> io::Result<()>;

    fn supports_keyboard_enhancement(&mut self) -> io::Result<bool> {
        Ok(true)
    }

    fn legacy_keyboard_degradations(&self) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Default)]
struct SystemTerminalControl;

impl TerminalControl for SystemTerminalControl {
    fn apply(&mut self, transition: Transition) -> io::Result<()> {
        match transition {
            Transition::EnableRawMode => enable_raw_mode(),
            Transition::EnterAlternateScreen => {
                stdout().execute(EnterAlternateScreen)?;
                Ok(())
            }
            Transition::PushKeyboardEnhancement => {
                stdout().execute(PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
                ))?;
                Ok(())
            }
            Transition::EnableMouseCapture => {
                stdout().execute(EnableMouseCapture)?;
                Ok(())
            }
            Transition::EnableBracketedPaste => {
                stdout().execute(EnableBracketedPaste)?;
                Ok(())
            }
            Transition::HideCursor => {
                stdout().execute(Hide)?;
                Ok(())
            }
            Transition::ShowCursor => {
                stdout().execute(Show)?;
                Ok(())
            }
            Transition::DisableBracketedPaste => {
                stdout().execute(DisableBracketedPaste)?;
                Ok(())
            }
            Transition::DisableMouseCapture => {
                stdout().execute(DisableMouseCapture)?;
                Ok(())
            }
            Transition::PopKeyboardEnhancement => {
                stdout().execute(PopKeyboardEnhancementFlags)?;
                Ok(())
            }
            Transition::LeaveAlternateScreen => {
                stdout().execute(LeaveAlternateScreen)?;
                Ok(())
            }
            Transition::DisableRawMode => disable_raw_mode(),
        }
    }

    fn supports_keyboard_enhancement(&mut self) -> io::Result<bool> {
        Ok(keyboard_enhancement_from_environment(|name| {
            env::var(name).ok()
        }))
    }

    fn legacy_keyboard_degradations(&self) -> Vec<String> {
        tmux_legacy_degradations(|name| env::var(name).ok())
    }
}

fn keyboard_enhancement_from_environment(value: impl Fn(&str) -> Option<String>) -> bool {
    match value("NEAR_KEYBOARD_PROTOCOL")
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("enhanced" | "kitty" | "1" | "true") => return true,
        Some("legacy" | "0" | "false") => return false,
        _ => {}
    }
    if value("TMUX").is_some() {
        return false;
    }
    let term_program = value("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term = value("TERM").unwrap_or_default().to_ascii_lowercase();
    matches!(
        term_program.as_str(),
        "ghostty" | "iterm.app" | "kitty" | "wezterm"
    ) || term.contains("kitty")
        || value("GHOSTTY_RESOURCES_DIR").is_some()
        || value("KITTY_WINDOW_ID").is_some()
        || value("WEZTERM_PANE").is_some()
}

fn tmux_legacy_degradations(value: impl Fn(&str) -> Option<String>) -> Vec<String> {
    if value("TMUX").is_some() {
        vec![
            "tmux legacy input requires `set -sg escape-time 10` for prompt standalone Escape"
                .to_owned(),
        ]
    } else {
        Vec::new()
    }
}

pub struct TerminalSession {
    output: Stdout,
    control: Box<dyn TerminalControl>,
    capabilities: TerminalCapabilities,
    diagnostics: TerminalDiagnostics,
}

impl TerminalSession {
    /// Enters Near's alternate-screen terminal mode.
    ///
    /// Optional terminal enhancements degrade independently. Raw mode and the alternate screen are
    /// required and roll back in reverse order when initialization fails.
    ///
    /// # Errors
    ///
    /// Returns an error if required terminal state cannot be established.
    pub fn enter() -> Result<Self, TerminalSessionError> {
        Self::enter_with_capabilities(TerminalCapabilities::default())
    }

    /// Enters terminal mode with explicit optional capability requests.
    ///
    /// # Errors
    ///
    /// Returns an error if required terminal state cannot be established.
    pub fn enter_with_capabilities(
        capabilities: TerminalCapabilities,
    ) -> Result<Self, TerminalSessionError> {
        Self::enter_with_control(capabilities, Box::<SystemTerminalControl>::default())
    }

    fn enter_with_control(
        capabilities: TerminalCapabilities,
        control: Box<dyn TerminalControl>,
    ) -> Result<Self, TerminalSessionError> {
        let mut session = Self {
            output: stdout(),
            control,
            capabilities,
            diagnostics: TerminalDiagnostics::default(),
        };
        session.activate()?;
        Ok(session)
    }

    fn activate(&mut self) -> Result<(), TerminalSessionError> {
        self.diagnostics.degraded_features.clear();
        if let Err(error) = self.control.apply(Transition::EnableRawMode) {
            return Err(error.into());
        }
        self.diagnostics.raw_mode = true;

        if let Err(error) = self.control.apply(Transition::EnterAlternateScreen) {
            let _ = self.restore();
            return Err(error.into());
        }
        self.diagnostics.alternate_screen = true;

        if self.capabilities.keyboard_enhancement {
            match self.control.supports_keyboard_enhancement() {
                Ok(true) => match self.control.apply(Transition::PushKeyboardEnhancement) {
                    Ok(()) => self.diagnostics.keyboard_mode = KeyboardMode::Enhanced,
                    Err(error) => self
                        .diagnostics
                        .degraded_features
                        .push(format!("keyboard enhancement unavailable: {error}")),
                },
                Ok(false) => self
                    .diagnostics
                    .degraded_features
                    .push("keyboard enhancement unsupported; using legacy mode".to_owned()),
                Err(error) => self
                    .diagnostics
                    .degraded_features
                    .push(format!("keyboard enhancement probe failed: {error}")),
            }
        }
        if self.diagnostics.keyboard_mode == KeyboardMode::Legacy {
            self.diagnostics
                .degraded_features
                .extend(self.control.legacy_keyboard_degradations());
        }

        if self.capabilities.bracketed_paste {
            match self.control.apply(Transition::EnableBracketedPaste) {
                Ok(()) => self.diagnostics.bracketed_paste = true,
                Err(error) => self
                    .diagnostics
                    .degraded_features
                    .push(format!("bracketed paste unavailable: {error}")),
            }
        }
        if self.capabilities.mouse_capture {
            match self.control.apply(Transition::EnableMouseCapture) {
                Ok(()) => self.diagnostics.mouse_capture = true,
                Err(error) => self
                    .diagnostics
                    .degraded_features
                    .push(format!("mouse capture unavailable: {error}")),
            }
        }
        if self.capabilities.cursor_visibility {
            match self.control.apply(Transition::HideCursor) {
                Ok(()) => self.diagnostics.cursor_hidden = true,
                Err(error) => self
                    .diagnostics
                    .degraded_features
                    .push(format!("cursor hiding unavailable: {error}")),
            }
        }
        Ok(())
    }

    pub fn output_mut(&mut self) -> &mut Stdout {
        &mut self.output
    }

    pub fn diagnostics(&self) -> &TerminalDiagnostics {
        &self.diagnostics
    }

    pub fn is_active(&self) -> bool {
        self.diagnostics.raw_mode
            || self.diagnostics.alternate_screen
            || self.diagnostics.bracketed_paste
            || self.diagnostics.mouse_capture
            || self.diagnostics.cursor_hidden
            || self.diagnostics.keyboard_mode == KeyboardMode::Enhanced
    }

    /// Restores every active terminal feature in reverse order.
    ///
    /// Restoration continues after individual failures so one unsupported or broken command cannot
    /// prevent later cleanup. Failed transitions remain active and are retried by `Drop`.
    ///
    /// # Errors
    ///
    /// Returns the first restoration error after attempting every active transition.
    pub fn restore(&mut self) -> Result<(), TerminalSessionError> {
        let mut first_error = None;
        self.restore_transition(
            Transition::ShowCursor,
            self.diagnostics.cursor_hidden,
            |diagnostics| &mut diagnostics.cursor_hidden,
            &mut first_error,
        );
        self.restore_transition(
            Transition::DisableMouseCapture,
            self.diagnostics.mouse_capture,
            |diagnostics| &mut diagnostics.mouse_capture,
            &mut first_error,
        );
        self.restore_transition(
            Transition::DisableBracketedPaste,
            self.diagnostics.bracketed_paste,
            |diagnostics| &mut diagnostics.bracketed_paste,
            &mut first_error,
        );
        self.restore_keyboard_enhancement(&mut first_error);
        self.restore_transition(
            Transition::LeaveAlternateScreen,
            self.diagnostics.alternate_screen,
            |diagnostics| &mut diagnostics.alternate_screen,
            &mut first_error,
        );
        self.restore_transition(
            Transition::DisableRawMode,
            self.diagnostics.raw_mode,
            |diagnostics| &mut diagnostics.raw_mode,
            &mut first_error,
        );
        first_error.map_or(Ok(()), |error| Err(error.into()))
    }

    fn restore_keyboard_enhancement(&mut self, first_error: &mut Option<io::Error>) {
        if self.diagnostics.keyboard_mode != KeyboardMode::Enhanced {
            return;
        }
        match self.control.apply(Transition::PopKeyboardEnhancement) {
            Ok(()) => self.diagnostics.keyboard_mode = KeyboardMode::Legacy,
            Err(error) if first_error.is_none() => *first_error = Some(error),
            Err(_) => {}
        }
    }

    fn restore_transition(
        &mut self,
        transition: Transition,
        active: bool,
        state: impl FnOnce(&mut TerminalDiagnostics) -> &mut bool,
        first_error: &mut Option<io::Error>,
    ) {
        if !active {
            return;
        }
        match self.control.apply(transition) {
            Ok(()) => *state(&mut self.diagnostics) = false,
            Err(error) if first_error.is_none() => *first_error = Some(error),
            Err(_) => {}
        }
    }

    /// Temporarily restores the host terminal, runs an external action, and re-enters Near mode.
    ///
    /// Re-entry is attempted even when the action fails. This is the primitive used by external
    /// editors, shells, and other interactive tools.
    ///
    /// # Errors
    ///
    /// Returns a restoration, external-action, or terminal re-entry error.
    pub fn suspend<T>(
        &mut self,
        action: impl FnOnce() -> io::Result<T>,
    ) -> Result<T, TerminalSessionError> {
        self.restore()?;
        let action_result = action();
        let resume_result = self.activate();
        match (action_result, resume_result) {
            (_, Err(error)) => Err(error),
            (Err(error), Ok(())) => Err(TerminalSessionError::External(error)),
            (Ok(value), Ok(())) => Ok(value),
        }
    }

    /// Runs a structured external invocation on the original terminal.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal suspension, process launch, waiting, or terminal resumption
    /// fails.
    pub fn run_external(
        &mut self,
        invocation: &ExternalInvocation,
    ) -> Result<ExitStatus, TerminalSessionError> {
        self.suspend(|| {
            let mut command = Command::new(&invocation.program);
            command.args(&invocation.arguments);
            if let Some(directory) = &invocation.current_directory {
                command.current_dir(directory);
            }
            if invocation.clear_environment {
                command.env_clear();
            }
            command.envs(&invocation.environment);
            let result = command.status();
            for path in &invocation.cleanup_paths {
                let _ = std::fs::remove_file(path);
            }
            result
        })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, VecDeque},
        io,
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{Arc, Mutex},
    };

    use super::{
        KeyboardMode, TerminalCapabilities, TerminalControl, TerminalSession, Transition,
        keyboard_enhancement_from_environment, tmux_legacy_degradations,
    };

    fn keyboard_environment(values: &[(&str, &str)]) -> bool {
        let values = values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>();
        keyboard_enhancement_from_environment(|name| values.get(name).cloned())
    }

    #[test]
    fn keyboard_enhancement_uses_environment_without_active_terminal_probe() {
        assert!(keyboard_environment(&[("TERM_PROGRAM", "iTerm.app")]));
        assert!(keyboard_environment(&[("TERM", "xterm-kitty")]));
        assert!(!keyboard_environment(&[("TERM_PROGRAM", "Apple_Terminal")]));
        assert!(!keyboard_environment(&[
            ("TERM_PROGRAM", "iTerm.app"),
            ("TMUX", "/tmp/tmux-501/default,1,0"),
        ]));
    }

    #[test]
    fn keyboard_protocol_override_is_authoritative() {
        assert!(!keyboard_environment(&[
            ("TERM_PROGRAM", "ghostty"),
            ("NEAR_KEYBOARD_PROTOCOL", "legacy"),
        ]));
        assert!(keyboard_environment(&[
            ("TERM_PROGRAM", "Apple_Terminal"),
            ("NEAR_KEYBOARD_PROTOCOL", "enhanced"),
        ]));
    }

    #[test]
    fn tmux_legacy_mode_declares_escape_timing_requirement() {
        let degradation = tmux_legacy_degradations(|name| {
            (name == "TMUX").then(|| "/tmp/tmux/default,1,0".to_owned())
        });
        assert_eq!(degradation.len(), 1);
        assert!(degradation[0].contains("escape-time 10"));
        assert!(tmux_legacy_degradations(|_| None).is_empty());
    }

    struct FakeControl {
        transitions: Arc<Mutex<Vec<Transition>>>,
        failures: VecDeque<Transition>,
        keyboard_enhancement: bool,
    }

    impl FakeControl {
        fn new(
            transitions: Arc<Mutex<Vec<Transition>>>,
            failures: impl IntoIterator<Item = Transition>,
        ) -> Self {
            Self {
                transitions,
                failures: failures.into_iter().collect(),
                keyboard_enhancement: true,
            }
        }

        fn with_keyboard_enhancement(mut self, supported: bool) -> Self {
            self.keyboard_enhancement = supported;
            self
        }
    }

    impl TerminalControl for FakeControl {
        fn apply(&mut self, transition: Transition) -> io::Result<()> {
            self.transitions.lock().unwrap().push(transition);
            if self.failures.front() == Some(&transition) {
                self.failures.pop_front();
                Err(io::Error::other(format!("injected {transition:?} failure")))
            } else {
                Ok(())
            }
        }

        fn supports_keyboard_enhancement(&mut self) -> io::Result<bool> {
            Ok(self.keyboard_enhancement)
        }
    }

    fn session(
        failures: impl IntoIterator<Item = Transition>,
    ) -> (TerminalSession, Arc<Mutex<Vec<Transition>>>) {
        let transitions = Arc::new(Mutex::new(Vec::new()));
        let control = FakeControl::new(Arc::clone(&transitions), failures);
        let session =
            TerminalSession::enter_with_control(TerminalCapabilities::default(), Box::new(control))
                .unwrap();
        (session, transitions)
    }

    #[test]
    fn normal_restore_uses_reverse_order() {
        let (mut session, transitions) = session([]);
        session.restore().unwrap();
        assert_eq!(
            *transitions.lock().unwrap(),
            [
                Transition::EnableRawMode,
                Transition::EnterAlternateScreen,
                Transition::PushKeyboardEnhancement,
                Transition::EnableBracketedPaste,
                Transition::EnableMouseCapture,
                Transition::HideCursor,
                Transition::ShowCursor,
                Transition::DisableMouseCapture,
                Transition::DisableBracketedPaste,
                Transition::PopKeyboardEnhancement,
                Transition::LeaveAlternateScreen,
                Transition::DisableRawMode,
            ]
        );
        assert!(!session.is_active());
    }

    #[test]
    fn required_initialization_failure_rolls_back_applied_state() {
        let transitions = Arc::new(Mutex::new(Vec::new()));
        let control =
            FakeControl::new(Arc::clone(&transitions), [Transition::EnterAlternateScreen]);
        assert!(
            TerminalSession::enter_with_control(
                TerminalCapabilities::default(),
                Box::new(control),
            )
            .is_err()
        );
        assert_eq!(
            *transitions.lock().unwrap(),
            [
                Transition::EnableRawMode,
                Transition::EnterAlternateScreen,
                Transition::DisableRawMode,
            ]
        );
    }

    #[test]
    fn optional_capability_failures_degrade_without_blocking_startup() {
        let transitions = Arc::new(Mutex::new(Vec::new()));
        let control = FakeControl::new(
            Arc::clone(&transitions),
            [
                Transition::EnableBracketedPaste,
                Transition::EnableMouseCapture,
                Transition::HideCursor,
            ],
        );
        let mut session =
            TerminalSession::enter_with_control(TerminalCapabilities::default(), Box::new(control))
                .unwrap();
        assert_eq!(session.diagnostics().degraded_features.len(), 3);
        assert_eq!(session.diagnostics().keyboard_mode, KeyboardMode::Enhanced);
        assert!(!session.diagnostics().bracketed_paste);
        assert!(!session.diagnostics().mouse_capture);
        assert!(!session.diagnostics().cursor_hidden);
        session.restore().unwrap();
    }

    #[test]
    fn unsupported_keyboard_enhancement_uses_deterministic_legacy_mode() {
        let transitions = Arc::new(Mutex::new(Vec::new()));
        let control =
            FakeControl::new(Arc::clone(&transitions), []).with_keyboard_enhancement(false);
        let mut session =
            TerminalSession::enter_with_control(TerminalCapabilities::default(), Box::new(control))
                .unwrap();
        assert_eq!(session.diagnostics().keyboard_mode, KeyboardMode::Legacy);
        assert!(
            session
                .diagnostics()
                .degraded_features
                .iter()
                .any(|feature| feature.contains("using legacy mode"))
        );
        assert!(
            !transitions
                .lock()
                .unwrap()
                .contains(&Transition::PushKeyboardEnhancement)
        );
        session.restore().unwrap();
    }

    #[test]
    fn restoration_continues_after_errors_and_drop_retries() {
        let (mut session, transitions) = session([Transition::ShowCursor]);
        assert!(session.restore().is_err());
        assert!(session.diagnostics().cursor_hidden);
        assert!(!session.diagnostics().raw_mode);
        drop(session);
        let transitions = transitions.lock().unwrap();
        assert_eq!(transitions.last(), Some(&Transition::ShowCursor));
    }

    #[test]
    fn panic_unwinding_restores_terminal_state() {
        let (session, transitions) = session([]);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _session = session;
            panic!("injected panic");
        }));
        assert!(result.is_err());
        assert_eq!(
            &transitions.lock().unwrap()[6..],
            [
                Transition::ShowCursor,
                Transition::DisableMouseCapture,
                Transition::DisableBracketedPaste,
                Transition::PopKeyboardEnhancement,
                Transition::LeaveAlternateScreen,
                Transition::DisableRawMode,
            ]
        );
    }

    #[test]
    fn suspend_restores_runs_action_and_reenters() {
        let (mut session, transitions) = session([]);
        let mut action_ran = false;
        session
            .suspend(|| {
                action_ran = true;
                Ok(())
            })
            .unwrap();
        assert!(action_ran);
        assert!(session.is_active());
        assert_eq!(
            *transitions.lock().unwrap(),
            [
                Transition::EnableRawMode,
                Transition::EnterAlternateScreen,
                Transition::PushKeyboardEnhancement,
                Transition::EnableBracketedPaste,
                Transition::EnableMouseCapture,
                Transition::HideCursor,
                Transition::ShowCursor,
                Transition::DisableMouseCapture,
                Transition::DisableBracketedPaste,
                Transition::PopKeyboardEnhancement,
                Transition::LeaveAlternateScreen,
                Transition::DisableRawMode,
                Transition::EnableRawMode,
                Transition::EnterAlternateScreen,
                Transition::PushKeyboardEnhancement,
                Transition::EnableBracketedPaste,
                Transition::EnableMouseCapture,
                Transition::HideCursor,
            ]
        );
    }

    #[test]
    fn suspend_reenters_after_external_failure() {
        let (mut session, _) = session([]);
        assert!(
            session
                .suspend(|| Err::<(), _>(io::Error::other("failed")))
                .is_err()
        );
        assert!(session.is_active());
    }

    #[test]
    fn external_process_status_is_returned_after_round_trip() {
        let (mut session, _) = session([]);
        let invocation = near_core::ExternalInvocation::new("true");
        let status = session.run_external(&invocation).unwrap();
        assert!(status.success());
        assert!(session.is_active());
    }

    #[test]
    fn external_process_removes_registered_cleanup_paths() {
        let (mut session, _) = session([]);
        let path =
            std::env::temp_dir().join(format!("near-terminal-cleanup-{}", std::process::id()));
        std::fs::write(&path, "temporary").unwrap();
        let invocation = near_core::ExternalInvocation::new("true").with_cleanup_path(&path);
        let status = session.run_external(&invocation).unwrap();
        assert!(status.success());
        assert!(!path.exists());
    }
}
