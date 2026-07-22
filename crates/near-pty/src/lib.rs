//! Safe embedded pseudo-terminal sessions with VT state and OSC 7 tracking.

use std::{
    ffi::OsString,
    fmt,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

pub const SHELL_PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellMode {
    #[default]
    PlatformDefault,
    Login,
    Interactive,
    Clean,
}

impl ShellMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlatformDefault => "platform-default",
            Self::Login => "login",
            Self::Interactive => "interactive",
            Self::Clean => "clean",
        }
    }
}

impl fmt::Display for ShellMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ShellMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "platform-default" | "default" => Ok(Self::PlatformDefault),
            "login" => Ok(Self::Login),
            "interactive" => Ok(Self::Interactive),
            "clean" => Ok(Self::Clean),
            _ => {
                Err("shell mode must be platform-default, login, interactive, or clean".to_owned())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellClosePolicy {
    #[default]
    Warn,
    KeepOpen,
    Close,
}

impl ShellClosePolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::KeepOpen => "keep-open",
            Self::Close => "close",
        }
    }

    pub const fn warns_before_terminating(self) -> bool {
        matches!(self, Self::Warn)
    }

    pub const fn keeps_process_on_close(self) -> bool {
        matches!(self, Self::KeepOpen)
    }

    pub const fn closes_on_exit(self) -> bool {
        matches!(self, Self::Close)
    }
}

impl fmt::Display for ShellClosePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ShellClosePolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "warn" => Ok(Self::Warn),
            "keep-open" | "keep" => Ok(Self::KeepOpen),
            "close" => Ok(Self::Close),
            _ => Err("shell close policy must be warn, keep-open, or close".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ShellProfile {
    pub schema: u32,
    #[serde(default)]
    pub program: Option<PathBuf>,
    #[serde(default)]
    pub mode: ShellMode,
    #[serde(default)]
    pub startup_command: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub close_policy: ShellClosePolicy,
    #[serde(default = "default_true")]
    pub inherit_environment: bool,
}

impl ShellProfile {
    /// Parses a versioned shell profile.
    ///
    /// # Errors
    ///
    /// Returns TOML decoding or unsupported-schema failures.
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let profile: Self = toml::from_str(source).map_err(|error| error.to_string())?;
        if profile.schema != SHELL_PROFILE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported shell profile schema {}",
                profile.schema
            ));
        }
        Ok(profile)
    }

    pub fn native_default() -> Self {
        Self {
            schema: SHELL_PROFILE_SCHEMA_VERSION,
            program: None,
            mode: ShellMode::PlatformDefault,
            startup_command: None,
            arguments: Vec::new(),
            close_policy: ShellClosePolicy::Warn,
            inherit_environment: true,
        }
    }

    pub fn resolve(&self) -> ResolvedShellProfile {
        let program = self.program.clone().unwrap_or_else(resolve_account_shell);
        let mode = match self.mode {
            ShellMode::PlatformDefault if cfg!(target_os = "macos") => ShellMode::Login,
            ShellMode::PlatformDefault => ShellMode::Interactive,
            mode => mode,
        };
        let mut arguments = shell_mode_arguments(&program, mode);
        arguments.extend(self.arguments.iter().map(OsString::from));
        if let Some(command) = &self.startup_command {
            arguments.push(OsString::from("-c"));
            arguments.push(OsString::from(command));
        }
        ResolvedShellProfile {
            program,
            mode,
            arguments,
            close_policy: self.close_policy,
            inherit_environment: self.inherit_environment,
        }
    }
}

impl Default for ShellProfile {
    fn default() -> Self {
        Self::native_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedShellProfile {
    pub program: PathBuf,
    pub mode: ShellMode,
    pub arguments: Vec<OsString>,
    pub close_policy: ShellClosePolicy,
    pub inherit_environment: bool,
}

impl ResolvedShellProfile {
    pub fn lifecycle_label(&self) -> String {
        format!("shell={} close={}", self.mode, self.close_policy)
    }
}

const fn default_true() -> bool {
    true
}

fn shell_mode_arguments(program: &Path, mode: ShellMode) -> Vec<OsString> {
    let name = program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match mode {
        ShellMode::Login => vec![OsString::from("-l")],
        ShellMode::Interactive => vec![OsString::from("-i")],
        ShellMode::Clean if name.contains("zsh") => vec![OsString::from("-f")],
        ShellMode::Clean if name.contains("bash") => {
            vec![OsString::from("--noprofile"), OsString::from("--norc")]
        }
        ShellMode::Clean | ShellMode::PlatformDefault => Vec::new(),
    }
}

fn resolve_account_shell() -> PathBuf {
    platform_account_shell()
        .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
        .or_else(|| std::env::var_os("COMSPEC").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }))
}

#[cfg(target_os = "macos")]
fn platform_account_shell() -> Option<PathBuf> {
    let user = std::env::var("USER").ok()?;
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{user}"), "UserShell"])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .last()
            .map(PathBuf::from)
    })?
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_account_shell() -> Option<PathBuf> {
    let user = std::env::var("USER").ok()?;
    let output = Command::new("getent")
        .args(["passwd", &user])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .split(':')
        .nth(6)
        .filter(|shell| !shell.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn platform_account_shell() -> Option<PathBuf> {
    std::env::var_os("NEAR_SHELL").map(PathBuf::from)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    pub rows: u16,
    pub columns: u16,
}

impl TerminalSize {
    pub fn new(rows: u16, columns: u16) -> Self {
        Self {
            rows: rows.max(1),
            columns: columns.max(1),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PtyColor {
    Default,
    Indexed(u8),
    Rgb { red: u8, green: u8, blue: u8 },
}

impl From<vt100::Color> for PtyColor {
    fn from(color: vt100::Color) -> Self {
        match color {
            vt100::Color::Default => Self::Default,
            vt100::Color::Idx(index) => Self::Indexed(index),
            vt100::Color::Rgb(red, green, blue) => Self::Rgb { red, green, blue },
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PtyCellStyle {
    pub foreground: PtyColor,
    pub background: PtyColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PtyCell {
    pub contents: String,
    pub width: u8,
    pub style: PtyCellStyle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PtySnapshot {
    pub lines: Vec<String>,
    pub cells: Vec<Vec<PtyCell>>,
    pub cursor: (u16, u16),
    pub alternate_screen: bool,
    pub application_cursor: bool,
    pub bracketed_paste: bool,
    pub current_directory_uri: Option<String>,
    pub exit_code: Option<u32>,
    pub shell_profile: Option<ResolvedShellProfile>,
    pub size: TerminalSize,
}

fn snapshot_cell(cell: &vt100::Cell) -> PtyCell {
    PtyCell {
        contents: cell.contents().to_owned(),
        width: if cell.is_wide_continuation() {
            0
        } else if cell.is_wide() {
            2
        } else {
            1
        },
        style: PtyCellStyle {
            foreground: cell.fgcolor().into(),
            background: cell.bgcolor().into(),
            bold: cell.bold(),
            dim: cell.dim(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
        },
    }
}

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("PTY operation failed: {0}")]
    Backend(String),
    #[error("PTY I/O failed: {0}")]
    Io(#[from] io::Error),
}

struct TerminalState {
    parser: vt100::Parser,
    current_directory_uri: Option<String>,
    osc_tail: Vec<u8>,
    exit_code: Option<u32>,
    exit_wake: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    output_wake: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    shell_profile: Option<ResolvedShellProfile>,
    size: TerminalSize,
}

pub struct PtySession {
    #[cfg(unix)]
    master: Mutex<Box<dyn MasterPty + Send>>,
    #[cfg(unix)]
    tty_name: Option<PathBuf>,
    writer: Mutex<Box<dyn Write + Send>>,
    #[cfg(unix)]
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    #[cfg(windows)]
    process: Arc<Mutex<conpty::Process>>,
    state: Arc<Mutex<TerminalState>>,
}

impl PtySession {
    /// Spawns a command attached to a native pseudo-terminal.
    ///
    /// # Errors
    ///
    /// Returns backend or I/O errors while opening the PTY, spawning, or cloning its reader.
    #[cfg(unix)]
    pub fn spawn(
        program: impl AsRef<Path>,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
    ) -> Result<Self, PtyError> {
        Self::spawn_with_environment(
            program,
            arguments,
            current_directory,
            size,
            scrollback,
            true,
        )
    }

    #[cfg(unix)]
    fn spawn_with_environment(
        program: impl AsRef<Path>,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
        inherit_environment: bool,
    ) -> Result<Self, PtyError> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: size.rows,
                cols: size.columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let mut command = CommandBuilder::new(program.as_ref());
        if !inherit_environment {
            command.env_clear();
        }
        for argument in arguments {
            command.arg(argument.as_ref());
        }
        if let Some(directory) = current_directory {
            command.cwd(directory);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let tty_name = pair.master.tty_name();
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let state = terminal_state(size, scrollback);
        spawn_reader(reader, Arc::clone(&state))?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let killer = child.clone_killer();
        let child_state = Arc::clone(&state);
        thread::Builder::new()
            .name("near-pty-child".to_owned())
            .spawn(move || {
                let code = child.wait().ok().map(|status| status.exit_code());
                let wake = {
                    let mut state = child_state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.exit_code = code;
                    state.exit_wake.clone()
                };
                if let Some(wake) = wake {
                    wake();
                }
            })
            .map_err(PtyError::Io)?;
        Ok(Self {
            master: Mutex::new(pair.master),
            tty_name,
            writer: Mutex::new(writer),
            killer: Mutex::new(killer),
            state,
        })
    }

    /// Spawns a command attached to a Windows pseudo-console.
    ///
    /// # Errors
    ///
    /// Returns backend or I/O errors while opening `ConPTY`, spawning, or cloning its pipes.
    #[cfg(windows)]
    pub fn spawn(
        program: impl AsRef<Path>,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
    ) -> Result<Self, PtyError> {
        Self::spawn_with_environment(
            program,
            arguments,
            current_directory,
            size,
            scrollback,
            true,
        )
    }

    #[cfg(windows)]
    fn spawn_with_environment(
        program: impl AsRef<Path>,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
        inherit_environment: bool,
    ) -> Result<Self, PtyError> {
        let mut command = std::process::Command::new(program.as_ref());
        command.args(arguments);
        if !inherit_environment {
            command.env_clear();
        }
        if let Some(directory) = current_directory {
            command.current_dir(directory);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let mut options = conpty::ProcessOptions::default();
        options.set_console_size(Some((
            i16::try_from(size.columns).unwrap_or(i16::MAX),
            i16::try_from(size.rows).unwrap_or(i16::MAX),
        )));
        let mut process = options
            .spawn(command)
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let reader = process
            .output()
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let writer = process
            .input()
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let state = terminal_state(size, scrollback);
        spawn_reader(reader, Arc::clone(&state))?;
        let process = Arc::new(Mutex::new(process));
        let child_process = Arc::clone(&process);
        let child_state = Arc::clone(&state);
        thread::Builder::new()
            .name("near-pty-child".to_owned())
            .spawn(move || {
                loop {
                    let code = {
                        let process = child_process
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        (!process.is_alive())
                            .then(|| process.wait(Some(0)).ok())
                            .flatten()
                    };
                    if let Some(code) = code {
                        let wake = {
                            let mut state = child_state
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            state.exit_code = Some(code);
                            state.exit_wake.clone()
                        };
                        if let Some(wake) = wake {
                            wake();
                        }
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(10));
                }
            })
            .map_err(PtyError::Io)?;
        Ok(Self {
            writer: Mutex::new(Box::new(writer)),
            process,
            state,
        })
    }

    /// Spawns interactive zsh without reading user startup files.
    ///
    /// # Errors
    ///
    /// Returns PTY creation or process-spawn errors.
    pub fn spawn_zsh(
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
    ) -> Result<Self, PtyError> {
        Self::spawn("/bin/zsh", ["-f"], current_directory, size, scrollback)
    }

    /// Spawns the native interactive shell through the platform PTY backend.
    ///
    /// # Errors
    ///
    /// Returns PTY creation or process-spawn errors.
    pub fn spawn_shell(
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
    ) -> Result<Self, PtyError> {
        Self::spawn_profile(
            &ShellProfile::native_default(),
            current_directory,
            size,
            scrollback,
        )
    }

    /// Spawns a resolved, versioned shell profile.
    ///
    /// # Errors
    ///
    /// Returns PTY creation or process-spawn errors.
    pub fn spawn_profile(
        profile: &ShellProfile,
        current_directory: Option<&Path>,
        size: TerminalSize,
        scrollback: usize,
    ) -> Result<Self, PtyError> {
        let resolved = profile.resolve();
        let session = Self::spawn_with_environment(
            &resolved.program,
            &resolved.arguments,
            current_directory,
            size,
            scrollback,
            resolved.inherit_environment,
        )?;
        session
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shell_profile = Some(resolved);
        Ok(session)
    }

    /// Writes raw terminal input.
    ///
    /// # Errors
    ///
    /// Returns writer failures from the PTY master.
    pub fn write(&self, bytes: &[u8]) -> Result<(), PtyError> {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    /// Writes paste content using bracketed paste when requested by the child.
    ///
    /// # Errors
    ///
    /// Returns writer failures from the PTY master.
    pub fn paste(&self, text: &str) -> Result<(), PtyError> {
        let bracketed = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .parser
            .screen()
            .bracketed_paste();
        if bracketed {
            self.write(b"\x1b[200~")?;
        }
        self.write(text.as_bytes())?;
        if bracketed {
            self.write(b"\x1b[201~")?;
        }
        Ok(())
    }

    /// Resizes both the operating-system PTY and VT state.
    ///
    /// # Errors
    ///
    /// Returns backend resize failures.
    pub fn resize(&self, size: TerminalSize) -> Result<(), PtyError> {
        #[cfg(unix)]
        self.master
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resize(PtySize {
                rows: size.rows,
                cols: size.columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        #[cfg(windows)]
        self.process
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resize(
                i16::try_from(size.columns).unwrap_or(i16::MAX),
                i16::try_from(size.rows).unwrap_or(i16::MAX),
            )
            .map_err(|error| PtyError::Backend(error.to_string()))?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.parser.screen_mut().set_size(size.rows, size.columns);
        state.size = size;
        Ok(())
    }

    /// Sends the terminal interrupt character to the foreground process group.
    ///
    /// # Errors
    ///
    /// Returns PTY writer failures.
    pub fn interrupt(&self) -> Result<(), PtyError> {
        self.write(&[0x03])
    }

    /// Sends the terminal end-of-file character.
    ///
    /// # Errors
    ///
    /// Returns PTY writer failures.
    pub fn end_of_file(&self) -> Result<(), PtyError> {
        self.write(&[0x04])
    }

    pub fn snapshot(&self) -> PtySnapshot {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let screen = state.parser.screen();
        let (rows, columns) = screen.size();
        let cells = (0..rows)
            .map(|row| {
                (0..columns)
                    .filter_map(|column| screen.cell(row, column).map(snapshot_cell))
                    .collect()
            })
            .collect();
        PtySnapshot {
            lines: screen.rows(0, columns).collect(),
            cells,
            cursor: screen.cursor_position(),
            alternate_screen: screen.alternate_screen(),
            application_cursor: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            current_directory_uri: state.current_directory_uri.clone(),
            exit_code: state.exit_code,
            shell_profile: state.shell_profile.clone(),
            size: TerminalSize::new(rows, columns),
        }
    }

    #[cfg(unix)]
    fn foreground_process_label(&self) -> Option<String> {
        let tty_name = self.tty_name.as_ref()?;
        let output = Command::new("ps")
            .args(["-t", tty_name.to_str()?, "-o", "pid=,stat=,comm="])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout))
            .and_then(|output| foreground_process_from_ps(&output))
    }

    #[cfg(windows)]
    fn foreground_process_label(&self) -> Option<String> {
        None
    }

    pub fn set_scrollback(&self, rows: usize) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .parser
            .screen_mut()
            .set_scrollback(rows);
    }

    /// Terminates the attached child process.
    ///
    /// # Errors
    ///
    /// Returns native child-termination failures.
    pub fn terminate(&self) -> Result<(), PtyError> {
        #[cfg(unix)]
        {
            self.killer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .kill()
                .map_err(|error| PtyError::Backend(error.to_string()))
        }
        #[cfg(windows)]
        {
            self.process
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .exit(1)
                .map_err(|error| PtyError::Backend(error.to_string()))
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = self
            .killer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .kill();
        #[cfg(windows)]
        let _ = self
            .process
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .exit(1);
    }
}

#[derive(Clone)]
pub struct PtySessionHandle {
    session: Arc<PtySession>,
}

impl PtySessionHandle {
    /// Spawns a shared PTY from a resolved shell profile.
    ///
    /// # Errors
    /// Returns PTY creation or child-process launch failures.
    pub fn spawn_profile(
        profile: &ShellProfile,
        current_directory: Option<&Path>,
    ) -> Result<Self, PtyError> {
        Ok(Self {
            session: Arc::new(PtySession::spawn_profile(
                profile,
                current_directory,
                TerminalSize::new(24, 80),
                10_000,
            )?),
        })
    }

    /// Spawns shared interactive zsh without startup files.
    ///
    /// # Errors
    /// Returns PTY creation or child-process launch failures.
    pub fn spawn_zsh(current_directory: Option<&Path>) -> Result<Self, PtyError> {
        Ok(Self {
            session: Arc::new(PtySession::spawn_zsh(
                current_directory,
                TerminalSize::new(24, 80),
                10_000,
            )?),
        })
    }

    /// Spawns the shared platform-default shell.
    ///
    /// # Errors
    /// Returns PTY creation or child-process launch failures.
    pub fn spawn_shell(current_directory: Option<&Path>) -> Result<Self, PtyError> {
        Ok(Self {
            session: Arc::new(PtySession::spawn_shell(
                current_directory,
                TerminalSize::new(24, 80),
                10_000,
            )?),
        })
    }

    /// Writes one command followed by Enter.
    ///
    /// # Errors
    /// Returns PTY writer failures.
    pub fn submit_line(&self, line: &str) -> Result<(), PtyError> {
        self.write(line.as_bytes())?;
        self.write(b"\r")
    }

    pub fn has_exited(&self) -> bool {
        self.snapshot().exit_code.is_some()
    }

    pub fn close_policy(&self) -> ShellClosePolicy {
        self.snapshot()
            .shell_profile
            .map_or(ShellClosePolicy::Warn, |profile| profile.close_policy)
    }

    /// Writes raw bytes to the PTY.
    ///
    /// # Errors
    /// Returns PTY writer failures.
    pub fn write(&self, bytes: &[u8]) -> Result<(), PtyError> {
        self.session.write(bytes)
    }

    /// Writes terminal bracketed-paste input.
    ///
    /// # Errors
    /// Returns PTY writer failures.
    pub fn paste(&self, text: &str) -> Result<(), PtyError> {
        self.session.paste(text)
    }

    /// Resizes the PTY and parser state.
    ///
    /// # Errors
    /// Returns platform PTY resize failures.
    pub fn resize(&self, size: TerminalSize) -> Result<(), PtyError> {
        self.session.resize(size)
    }

    /// Sends the interrupt character.
    ///
    /// # Errors
    /// Returns PTY writer failures.
    pub fn interrupt(&self) -> Result<(), PtyError> {
        self.session.interrupt()
    }

    /// Sends the end-of-file character.
    ///
    /// # Errors
    /// Returns PTY writer failures.
    pub fn end_of_file(&self) -> Result<(), PtyError> {
        self.session.end_of_file()
    }

    pub fn set_scrollback(&self, rows: usize) {
        self.session.set_scrollback(rows);
    }

    pub fn snapshot(&self) -> PtySnapshot {
        self.session.snapshot()
    }

    /// Returns the foreground process attached to this terminal when the platform exposes it.
    pub fn foreground_process_label(&self) -> Option<String> {
        self.session.foreground_process_label()
    }

    pub fn set_exit_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        let wake = Arc::new(wake);
        let exited = {
            let mut state = self
                .session
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.exit_wake = Some(wake.clone());
            state.exit_code.is_some()
        };
        if exited {
            wake();
        }
    }

    pub fn set_output_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.session
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .output_wake = Some(Arc::new(wake));
    }

    /// Terminates the attached child process.
    ///
    /// # Errors
    /// Returns platform child-termination failures.
    pub fn terminate(&self) -> Result<(), PtyError> {
        self.session.terminate()
    }
}

fn terminal_state(size: TerminalSize, scrollback: usize) -> Arc<Mutex<TerminalState>> {
    Arc::new(Mutex::new(TerminalState {
        parser: vt100::Parser::new(size.rows, size.columns, scrollback),
        current_directory_uri: None,
        osc_tail: Vec::new(),
        exit_code: None,
        exit_wake: None,
        output_wake: None,
        shell_profile: None,
        size,
    }))
}

fn spawn_reader(
    mut reader: impl Read + Send + 'static,
    state: Arc<Mutex<TerminalState>>,
) -> Result<(), PtyError> {
    thread::Builder::new()
        .name("near-pty-reader".to_owned())
        .spawn(move || {
            let mut buffer = [0_u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let wake = {
                            let mut state = state
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            state.parser.process(&buffer[..read]);
                            process_osc7(&mut state, &buffer[..read]);
                            state.output_wake.clone()
                        };
                        if let Some(wake) = wake {
                            wake();
                        }
                    }
                }
            }
        })
        .map(|_| ())
        .map_err(PtyError::Io)
}

fn process_osc7(state: &mut TerminalState, bytes: &[u8]) {
    state.osc_tail.extend_from_slice(bytes);
    if state.osc_tail.len() > 16 * 1024 {
        let start = state.osc_tail.len() - 16 * 1024;
        state.osc_tail.drain(..start);
    }
    loop {
        let Some(start) = find_bytes(&state.osc_tail, b"\x1b]7;") else {
            retain_possible_osc_prefix(&mut state.osc_tail);
            break;
        };
        let payload_start = start + 4;
        let Some((end, terminator_length)) = find_osc_end(&state.osc_tail[payload_start..]) else {
            if start > 0 {
                state.osc_tail.drain(..start);
            }
            break;
        };
        let payload_end = payload_start + end;
        if let Ok(uri) = std::str::from_utf8(&state.osc_tail[payload_start..payload_end])
            && uri.starts_with("file://")
        {
            state.current_directory_uri = Some(uri.to_owned());
        }
        state.osc_tail.drain(..payload_end + terminator_length);
    }
}

fn find_osc_end(bytes: &[u8]) -> Option<(usize, usize)> {
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == 0x07 {
            return Some((index, 1));
        }
        if *byte == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
            return Some((index, 2));
        }
    }
    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn retain_possible_osc_prefix(bytes: &mut Vec<u8>) {
    let keep = bytes.len().min(3);
    if bytes.len() > keep {
        bytes.drain(..bytes.len() - keep);
    }
}

#[cfg(unix)]
fn foreground_process_from_ps(output: &str) -> Option<String> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _pid = fields.next()?;
            let state = fields.next()?;
            let command = fields.next()?;
            state.contains('+').then_some(command)
        })
        .filter_map(|command| {
            let command = command.trim_start_matches('-');
            Path::new(command)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .next_back()
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn versioned_profile_resolves_login_clean_and_custom_command_modes() {
        let login = ShellProfile::from_toml(
            "schema = 1\nprogram = '/bin/zsh'\nmode = 'login'\nstartup_command = 'printf ready'\ninherit_environment = false\n",
        )
        .unwrap()
        .resolve();
        assert_eq!(login.program, PathBuf::from("/bin/zsh"));
        assert_eq!(login.mode, ShellMode::Login);
        assert_eq!(
            login.arguments,
            ["-l", "-c", "printf ready"].map(OsString::from)
        );
        assert!(!login.inherit_environment);
        assert_eq!(login.close_policy, ShellClosePolicy::Warn);

        let clean = ShellProfile::from_toml("schema = 1\nprogram = '/bin/bash'\nmode = 'clean'\n")
            .unwrap()
            .resolve();
        assert_eq!(
            clean.arguments,
            ["--noprofile", "--norc"].map(OsString::from)
        );
        assert_eq!("default".parse(), Ok(ShellMode::PlatformDefault));
        assert!(ShellProfile::from_toml("schema = 2\n").is_err());
    }

    #[test]
    fn platform_default_uses_login_mode_on_macos_and_interactive_elsewhere() {
        let mut profile = ShellProfile::native_default();
        profile.program = Some(PathBuf::from("/bin/zsh"));
        let resolved = profile.resolve();
        if cfg!(target_os = "macos") {
            assert_eq!(resolved.arguments, [OsString::from("-l")]);
        } else {
            assert_eq!(resolved.arguments, [OsString::from("-i")]);
        }
    }

    #[test]
    fn shell_close_policy_contract_is_explicit() {
        assert_eq!("warn".parse(), Ok(ShellClosePolicy::Warn));
        assert_eq!("keep".parse(), Ok(ShellClosePolicy::KeepOpen));
        assert_eq!("close".parse(), Ok(ShellClosePolicy::Close));
        assert!(ShellClosePolicy::Warn.warns_before_terminating());
        assert!(ShellClosePolicy::KeepOpen.keeps_process_on_close());
        assert!(ShellClosePolicy::Close.closes_on_exit());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn foreground_process_parser_prefers_the_latest_foreground_command() {
        let output =
            " 10 Ss /usr/bin/login\n 11 S+ -zsh\n 12 S /tmp/helper\n 20 S+ /usr/local/bin/codex\n";
        assert_eq!(foreground_process_from_ps(output).as_deref(), Some("codex"));
    }

    fn wait_for(session: &PtySession, needle: &str) -> PtySnapshot {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = session.snapshot();
            if snapshot.lines.join("\n").contains(needle) || Instant::now() >= deadline {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_until(session: &PtySession, predicate: impl Fn(&PtySnapshot) -> bool) -> PtySnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = session.snapshot();
            if predicate(&snapshot) || Instant::now() >= deadline {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn send_line(session: &PtySession, line: &str) {
        session.write(line.as_bytes()).unwrap();
        session.write(b"\r").unwrap();
    }

    #[test]
    fn command_output_resize_and_exit_are_observable() {
        let session = PtySession::spawn(
            "/bin/sh",
            ["-c", "printf near-pty-ready"],
            None,
            TerminalSize::new(10, 40),
            100,
        )
        .unwrap();
        let snapshot = wait_for(&session, "near-pty-ready");
        assert!(snapshot.lines.join("\n").contains("near-pty-ready"));
        session.resize(TerminalSize::new(20, 60)).unwrap();
        assert_eq!(session.snapshot().size, TerminalSize::new(20, 60));
    }

    #[test]
    fn snapshot_preserves_ansi_styles_and_wide_cell_geometry() {
        let session = PtySession::spawn(
            "/bin/sh",
            ["-c", "printf '\\033[1;4;31;44mR\\033[0m界'"],
            None,
            TerminalSize::new(4, 20),
            10,
        )
        .unwrap();
        let snapshot = wait_for(&session, "R界");
        let cells = snapshot.cells.iter().flatten().collect::<Vec<_>>();
        let red = cells.iter().find(|cell| cell.contents == "R").unwrap();
        assert_eq!(red.style.foreground, PtyColor::Indexed(1));
        assert_eq!(red.style.background, PtyColor::Indexed(4));
        assert!(red.style.bold);
        assert!(red.style.underline);
        let wide = cells.iter().position(|cell| cell.contents == "界").unwrap();
        assert_eq!(cells[wide].width, 2);
        assert_eq!(cells[wide + 1].width, 0);
    }

    #[test]
    fn shared_session_wakes_even_when_exit_precedes_callback_installation() {
        let profile = ShellProfile::from_toml(
            "schema = 1\nprogram = '/bin/sh'\nmode = 'clean'\nstartup_command = 'exit 0'\n",
        )
        .unwrap();
        let session = PtySessionHandle::spawn_profile(&profile, None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !session.has_exited() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(session.has_exited());
        let notified = Arc::new(AtomicBool::new(false));
        let wake_state = Arc::clone(&notified);
        session.set_exit_wake(move || wake_state.store(true, Ordering::Release));
        assert!(notified.load(Ordering::Acquire));
    }

    #[test]
    fn alternate_screen_and_osc7_are_parsed() {
        let session = PtySession::spawn(
            "/bin/sh",
            [
                "-c",
                "printf '\\033]7;file://localhost/tmp\\007\\033[?1049hALT'; sleep 0.2; printf '\\033[?1049l'",
            ],
            None,
            TerminalSize::new(10, 40),
            100,
        )
        .unwrap();
        let snapshot = wait_for(&session, "ALT");
        assert_eq!(
            snapshot.current_directory_uri.as_deref(),
            Some("file://localhost/tmp")
        );
    }

    #[test]
    fn interactive_native_shell_supports_paste_interrupt_and_osc7() {
        let session = PtySession::spawn_shell(None, TerminalSize::new(16, 80), 1_000).unwrap();
        session.paste("printf near-pasted").unwrap();
        session.write(b"\r").unwrap();
        assert!(
            wait_for(&session, "near-pasted")
                .lines
                .join("\n")
                .contains("near-pasted")
        );

        send_line(
            &session,
            "printf '\\033]7;file://localhost/private/tmp\\007'",
        );
        let osc = wait_until(&session, |snapshot| {
            snapshot.current_directory_uri.as_deref() == Some("file://localhost/private/tmp")
        });
        assert_eq!(
            osc.current_directory_uri.as_deref(),
            Some("file://localhost/private/tmp")
        );

        send_line(&session, "sleep 30");
        std::thread::sleep(Duration::from_millis(100));
        session.interrupt().unwrap();
        send_line(&session, "printf near-after-interrupt");
        assert!(
            wait_for(&session, "near-after-interrupt")
                .lines
                .join("\n")
                .contains("near-after-interrupt")
        );
    }

    #[test]
    fn nested_vim_and_ssh_client_return_to_the_shell() {
        let session = PtySession::spawn_shell(None, TerminalSize::new(24, 100), 2_000).unwrap();
        send_line(
            &session,
            "if command -v vim >/dev/null; then vim -Nu NONE -n -c 'set shortmess+=I'; else printf near-no-vim; fi; printf near-after-vim",
        );
        let vim = wait_until(&session, |snapshot| {
            snapshot.alternate_screen || snapshot.lines.join("\n").contains("near-no-vim")
        });
        if vim.alternate_screen {
            session.write(b":qa!\r").unwrap();
            let restored = wait_until(&session, |snapshot| !snapshot.alternate_screen);
            assert!(!restored.alternate_screen);
        }
        assert!(
            wait_for(&session, "near-after-vim")
                .lines
                .join("\n")
                .contains("near-after-vim")
        );

        send_line(
            &session,
            "if command -v ssh >/dev/null; then ssh -V; fi; printf near-after-ssh",
        );
        assert!(
            wait_for(&session, "near-after-ssh")
                .lines
                .join("\n")
                .contains("near-after-ssh")
        );
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn conpty_command_output_and_exit_are_observable() {
        let shell = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
        let session = PtySession::spawn(
            shell,
            std::iter::empty::<&str>(),
            None,
            TerminalSize::new(10, 40),
            100,
        )
        .unwrap();
        session
            .write(b"echo near-conpty-ready\r\nexit /B 0\r\n")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = session.snapshot();
            if snapshot.lines.join("\n").contains("near-conpty-ready")
                && snapshot.exit_code == Some(0)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "ConPTY output or exit status did not arrive: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
