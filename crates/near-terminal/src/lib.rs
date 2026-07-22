//! Terminal lifecycle and normalized input for Near.

mod input;
mod reactor;
mod session;
mod signal;

pub use input::{
    Key, KeyKind, KeyStroke, ModifierKey, Modifiers, MouseButton, MouseEvent, MouseEventKind,
    TerminalEvent,
};
pub use reactor::{RuntimeWakeHandle, TerminalEventReactor, TerminalRuntimeEvent};
pub use session::{
    KeyboardMode, TerminalCapabilities, TerminalDiagnostics, TerminalSession, TerminalSessionError,
};
pub use signal::TerminationWatcher;

/// Maximum idle wait between signal/background-task checks in interactive runtimes.
pub const DEFAULT_IDLE_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Returns terminal dimensions without emitting a cursor-position query.
pub fn dimensions() -> (u16, u16) {
    terminal_size::terminal_size().map_or(
        (80, 24),
        |(terminal_size::Width(width), terminal_size::Height(height))| (width, height),
    )
}

/// Reads and normalizes the next supported terminal event.
///
/// # Errors
///
/// Returns an I/O error when the terminal event source cannot be read.
pub fn read_event() -> std::io::Result<TerminalEvent> {
    loop {
        #[cfg(unix)]
        ensure_terminal_input_connected()?;
        let event = crossterm::event::read()?;
        if let Ok(event) = TerminalEvent::try_from(event) {
            return Ok(event);
        }
    }
}

/// Reads the next supported terminal event until `timeout` elapses.
///
/// # Errors
///
/// Returns an I/O error when polling or reading the terminal event source fails.
pub fn read_event_timeout(timeout: std::time::Duration) -> std::io::Result<Option<TerminalEvent>> {
    let started = std::time::Instant::now();
    let mut remaining = timeout;
    loop {
        #[cfg(unix)]
        ensure_terminal_input_connected()?;
        if !crossterm::event::poll(remaining)? {
            return Ok(None);
        }
        let event = crossterm::event::read()?;
        if let Ok(event) = TerminalEvent::try_from(event) {
            return Ok(Some(event));
        }
        remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Ok(None);
        }
    }
}

#[cfg(unix)]
fn ensure_terminal_input_connected() -> std::io::Result<()> {
    use std::os::fd::AsRawFd as _;
    use std::sync::{Mutex, OnceLock};

    use mio::{Events, Interest, Poll, Token, unix::SourceFd};

    struct InputMonitor {
        poll: Poll,
        events: Events,
    }

    impl InputMonitor {
        fn new() -> std::io::Result<Self> {
            let poll = Poll::new()?;
            let descriptor = std::io::stdin().as_raw_fd();
            poll.registry()
                .register(&mut SourceFd(&descriptor), Token(0), Interest::READABLE)?;
            Ok(Self {
                poll,
                events: Events::with_capacity(4),
            })
        }

        fn connected(&mut self) -> std::io::Result<bool> {
            self.events.clear();
            self.poll
                .poll(&mut self.events, Some(std::time::Duration::ZERO))?;
            for event in &self.events {
                if event.is_read_closed() {
                    return Ok(false);
                }
                if event.is_readable() && rustix::io::ioctl_fionread(std::io::stdin())? == 0 {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }

    static MONITOR: OnceLock<Mutex<InputMonitor>> = OnceLock::new();
    let monitor = if let Some(monitor) = MONITOR.get() {
        monitor
    } else {
        let _ = MONITOR.set(Mutex::new(InputMonitor::new()?));
        MONITOR.get().expect("terminal input monitor initialized")
    };
    let connected = monitor
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .connected()?;
    if connected {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "terminal input is disconnected",
        ))
    }
}

#[cfg(test)]
mod idle_tests {
    use super::DEFAULT_IDLE_POLL;

    #[test]
    fn idle_runtime_wait_is_blocking_and_nonzero() {
        assert_eq!(DEFAULT_IDLE_POLL, std::time::Duration::from_millis(50));
        assert!(!DEFAULT_IDLE_POLL.is_zero());
    }
}
