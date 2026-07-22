use crate::{TerminalEvent, TerminationWatcher};

#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use crate::{Key, KeyKind, KeyStroke, Modifiers};

#[cfg(unix)]
const LEGACY_ESCAPE_GRACE: Duration = Duration::from_millis(25);
#[cfg(unix)]
const LEGACY_ESCAPE_LIMIT: usize = 32;

#[cfg(unix)]
#[derive(Debug, PartialEq)]
enum FragmentedEscape {
    Complete(TerminalEvent),
    Invalid,
    Prefix,
}

#[cfg(unix)]
fn is_plain_escape(event: &TerminalEvent) -> bool {
    matches!(
        event,
        TerminalEvent::Key(KeyStroke {
            key: Key::Escape,
            modifiers: Modifiers {
                shift: false,
                control: false,
                alt: false,
                super_key: false,
            },
            kind: KeyKind::Press,
        })
    )
}

#[cfg(unix)]
fn plain_character(event: &TerminalEvent) -> Option<char> {
    let TerminalEvent::Key(KeyStroke {
        key: Key::Character(character),
        modifiers,
        kind: KeyKind::Press,
    }) = event
    else {
        return None;
    };
    (*modifiers == Modifiers::default()).then_some(*character)
}

#[cfg(unix)]
fn key_event(key: Key, modifiers: Modifiers) -> TerminalEvent {
    TerminalEvent::Key(KeyStroke {
        key,
        modifiers,
        kind: KeyKind::Press,
    })
}

#[cfg(unix)]
fn xterm_modifiers(parameter: Option<&str>) -> Option<Modifiers> {
    let encoded = match parameter {
        Some(value) => value.parse::<u8>().ok()?,
        None => 1,
    };
    let bits = encoded.checked_sub(1)?;
    Some(Modifiers {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        control: bits & 4 != 0,
        super_key: bits & 8 != 0,
    })
}

#[cfg(unix)]
fn decode_csi(body: &str) -> FragmentedEscape {
    let Some(final_character) = body.chars().last() else {
        return FragmentedEscape::Prefix;
    };
    if !('@'..='~').contains(&final_character) {
        return FragmentedEscape::Prefix;
    }
    let parameters = &body[..body.len() - final_character.len_utf8()];
    match (parameters, final_character) {
        ("", 'I') => return FragmentedEscape::Complete(TerminalEvent::FocusGained),
        ("", 'O') => return FragmentedEscape::Complete(TerminalEvent::FocusLost),
        ("", 'Z') => {
            return FragmentedEscape::Complete(key_event(
                Key::BackTab,
                Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            ));
        }
        _ => {}
    }
    let mut parts = parameters.split(';');
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    if parts.next().is_some() {
        return FragmentedEscape::Invalid;
    }
    let Some(modifiers) = xterm_modifiers(second) else {
        return FragmentedEscape::Invalid;
    };
    let key = match final_character {
        'A' if first.is_empty() || first == "1" => Key::Up,
        'B' if first.is_empty() || first == "1" => Key::Down,
        'C' if first.is_empty() || first == "1" => Key::Right,
        'D' if first.is_empty() || first == "1" => Key::Left,
        'H' if first.is_empty() || first == "1" => Key::Home,
        'F' if first.is_empty() || first == "1" => Key::End,
        '~' => match first.parse::<u8>() {
            Ok(1 | 7) => Key::Home,
            Ok(2) => Key::Insert,
            Ok(3) => Key::Delete,
            Ok(4 | 8) => Key::End,
            Ok(5) => Key::PageUp,
            Ok(6) => Key::PageDown,
            Ok(11) => Key::Function(1),
            Ok(12) => Key::Function(2),
            Ok(13) => Key::Function(3),
            Ok(14) => Key::Function(4),
            Ok(15) => Key::Function(5),
            Ok(17) => Key::Function(6),
            Ok(18) => Key::Function(7),
            Ok(19) => Key::Function(8),
            Ok(20) => Key::Function(9),
            Ok(21) => Key::Function(10),
            Ok(23) => Key::Function(11),
            Ok(24) => Key::Function(12),
            _ => return FragmentedEscape::Invalid,
        },
        _ => return FragmentedEscape::Invalid,
    };
    FragmentedEscape::Complete(key_event(key, modifiers))
}

#[cfg(unix)]
fn decode_ss3(body: &str) -> FragmentedEscape {
    let mut characters = body.chars();
    let Some(character) = characters.next() else {
        return FragmentedEscape::Prefix;
    };
    if characters.next().is_some() {
        return FragmentedEscape::Invalid;
    }
    let key = match character {
        'A' => Key::Up,
        'B' => Key::Down,
        'C' => Key::Right,
        'D' => Key::Left,
        'H' => Key::Home,
        'F' => Key::End,
        'P' => Key::Function(1),
        'Q' => Key::Function(2),
        'R' => Key::Function(3),
        'S' => Key::Function(4),
        _ => return FragmentedEscape::Invalid,
    };
    FragmentedEscape::Complete(key_event(key, Modifiers::default()))
}

#[cfg(unix)]
fn decode_fragmented_escape(events: &[TerminalEvent]) -> FragmentedEscape {
    let Some(first) = events.first() else {
        return FragmentedEscape::Prefix;
    };
    match plain_character(first) {
        Some('[') => {
            let mut body = String::new();
            for event in &events[1..] {
                let Some(character) = plain_character(event) else {
                    return FragmentedEscape::Invalid;
                };
                body.push(character);
            }
            decode_csi(&body)
        }
        Some('O') => {
            let mut body = String::new();
            for event in &events[1..] {
                let Some(character) = plain_character(event) else {
                    return FragmentedEscape::Invalid;
                };
                body.push(character);
            }
            decode_ss3(&body)
        }
        _ if events.len() == 1 => {
            let TerminalEvent::Key(mut stroke) = first.clone() else {
                return FragmentedEscape::Invalid;
            };
            if stroke.kind != KeyKind::Press || matches!(stroke.key, Key::Modifier(_)) {
                return FragmentedEscape::Invalid;
            }
            stroke.modifiers.alt = true;
            FragmentedEscape::Complete(TerminalEvent::Key(stroke))
        }
        _ => FragmentedEscape::Invalid,
    }
}

#[derive(Debug)]
pub enum TerminalRuntimeEvent {
    Terminal(TerminalEvent),
    Wake,
    Timeout,
    Terminate(i32),
}

#[cfg(unix)]
mod platform {
    use std::{
        collections::VecDeque,
        io::Read as _,
        os::{fd::AsRawFd as _, unix::net::UnixStream},
        sync::Arc,
        time::Duration,
    };

    use mio::{Events, Interest, Poll, Token, Waker, unix::SourceFd};
    use signal_hook::{SigId, low_level};

    use super::{
        FragmentedEscape, Instant, LEGACY_ESCAPE_GRACE, LEGACY_ESCAPE_LIMIT, TerminalRuntimeEvent,
        TerminationWatcher, decode_fragmented_escape, is_plain_escape,
    };
    use crate::{read_event, read_event_timeout, signal::termination_signals};

    const TERMINAL: Token = Token(0);
    const SIGNAL: Token = Token(1);
    const WAKE: Token = Token(2);

    #[derive(Clone)]
    pub struct RuntimeWakeHandle(Arc<Waker>);

    impl RuntimeWakeHandle {
        /// Wakes the event reactor from another thread.
        ///
        /// # Errors
        ///
        /// Returns an operating-system error when the registered reactor is unavailable.
        pub fn wake(&self) -> std::io::Result<()> {
            self.0.wake()
        }
    }

    pub struct TerminalEventReactor {
        poll: Poll,
        events: Events,
        wake: RuntimeWakeHandle,
        signal_read: UnixStream,
        termination: TerminationWatcher,
        signal_registrations: Vec<SigId>,
        pending_terminal: VecDeque<crate::TerminalEvent>,
    }

    impl TerminalEventReactor {
        /// Creates a reactor for terminal input, signals, explicit wakes, and deadlines.
        ///
        /// # Errors
        ///
        /// Returns an operating-system error when polling, signal pipes, or stdin registration
        /// cannot be initialized.
        pub fn new() -> std::io::Result<Self> {
            let poll = Poll::new()?;
            let descriptor = std::io::stdin().as_raw_fd();
            poll.registry()
                .register(&mut SourceFd(&descriptor), TERMINAL, Interest::READABLE)?;

            let (signal_read, signal_write) = UnixStream::pair()?;
            signal_read.set_nonblocking(true)?;
            let signal_descriptor = signal_read.as_raw_fd();
            poll.registry().register(
                &mut SourceFd(&signal_descriptor),
                SIGNAL,
                Interest::READABLE,
            )?;
            let termination = TerminationWatcher::register()?;
            let mut signal_registrations = Vec::new();
            for signal in termination_signals() {
                signal_registrations.push(signal_hook::low_level::pipe::register(
                    signal,
                    signal_write.try_clone()?,
                )?);
            }
            drop(signal_write);

            let wake = RuntimeWakeHandle(Arc::new(Waker::new(poll.registry(), WAKE)?));
            Ok(Self {
                poll,
                events: Events::with_capacity(8),
                wake,
                signal_read,
                termination,
                signal_registrations,
                pending_terminal: VecDeque::new(),
            })
        }

        pub fn wake_handle(&self) -> RuntimeWakeHandle {
            self.wake.clone()
        }

        /// Blocks until one runtime event or the optional deadline is reached.
        ///
        /// # Errors
        ///
        /// Returns an I/O error for polling, terminal reads, signal-pipe reads, or terminal
        /// disconnection.
        pub fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<TerminalRuntimeEvent> {
            if let Some(event) = self.pending_terminal.pop_front() {
                return Ok(TerminalRuntimeEvent::Terminal(event));
            }
            self.events.clear();
            self.poll.poll(&mut self.events, Some(Duration::ZERO))?;
            if let Some(event) = self.process_ready_events()? {
                return Ok(event);
            }
            if let Some(event) = read_event_timeout(Duration::ZERO)? {
                return self
                    .coalesce_fragmented_escape(event)
                    .map(TerminalRuntimeEvent::Terminal);
            }
            self.events.clear();
            self.poll.poll(&mut self.events, timeout)?;
            Ok(self
                .process_ready_events()?
                .unwrap_or(TerminalRuntimeEvent::Timeout))
        }

        fn process_ready_events(&mut self) -> std::io::Result<Option<TerminalRuntimeEvent>> {
            let event_count = self.events.iter().count();
            for index in 0..event_count {
                let event = self.events.iter().nth(index).expect("event index is valid");
                let token = event.token();
                let read_closed = event.is_read_closed();
                let readable = event.is_readable();
                match token {
                    TERMINAL if read_closed => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "terminal input is disconnected",
                        ));
                    }
                    TERMINAL if readable => {
                        if rustix::io::ioctl_fionread(std::io::stdin())? == 0 {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::BrokenPipe,
                                "terminal input is disconnected",
                            ));
                        }
                        let event = read_event()?;
                        return self
                            .coalesce_fragmented_escape(event)
                            .map(TerminalRuntimeEvent::Terminal)
                            .map(Some);
                    }
                    SIGNAL => {
                        let mut buffer = [0_u8; 64];
                        loop {
                            match self.signal_read.read(&mut buffer) {
                                Ok(0) => break,
                                Ok(_) => {}
                                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                    break;
                                }
                                Err(error) => return Err(error),
                            }
                        }
                        if let Some(signal) = self.termination.take() {
                            return Ok(Some(TerminalRuntimeEvent::Terminate(signal)));
                        }
                    }
                    WAKE => return Ok(Some(TerminalRuntimeEvent::Wake)),
                    _ => {}
                }
            }
            Ok(None)
        }

        fn coalesce_fragmented_escape(
            &mut self,
            escape: crate::TerminalEvent,
        ) -> std::io::Result<crate::TerminalEvent> {
            if !is_plain_escape(&escape) {
                return Ok(escape);
            }
            let started = Instant::now();
            let mut suffix = Vec::new();
            loop {
                match decode_fragmented_escape(&suffix) {
                    FragmentedEscape::Complete(event) => return Ok(event),
                    FragmentedEscape::Invalid => {
                        self.pending_terminal.extend(suffix);
                        return Ok(escape);
                    }
                    FragmentedEscape::Prefix => {}
                }
                if suffix.len() >= LEGACY_ESCAPE_LIMIT {
                    self.pending_terminal.extend(suffix);
                    return Ok(escape);
                }
                let remaining = LEGACY_ESCAPE_GRACE.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    self.pending_terminal.extend(suffix);
                    return Ok(escape);
                }
                let Some(event) = read_event_timeout(remaining)? else {
                    self.pending_terminal.extend(suffix);
                    return Ok(escape);
                };
                suffix.push(event);
            }
        }
    }

    impl Drop for TerminalEventReactor {
        fn drop(&mut self) {
            for registration in self.signal_registrations.drain(..) {
                low_level::unregister(registration);
            }
        }
    }
}

#[cfg(not(unix))]
mod platform {
    use std::time::Duration;

    use super::{TerminalRuntimeEvent, TerminationWatcher};
    use crate::{DEFAULT_IDLE_POLL, read_event_timeout};

    #[derive(Clone, Copy, Debug, Default)]
    pub struct RuntimeWakeHandle;

    impl RuntimeWakeHandle {
        /// Signals the fallback reactor.
        ///
        /// # Errors
        ///
        /// The fallback implementation currently cannot fail.
        pub const fn wake(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    pub struct TerminalEventReactor {
        termination: TerminationWatcher,
    }

    impl TerminalEventReactor {
        /// Creates the fallback terminal event reactor.
        ///
        /// # Errors
        ///
        /// Returns an I/O error when termination monitoring cannot be initialized.
        pub fn new() -> std::io::Result<Self> {
            Ok(Self {
                termination: TerminationWatcher::register()?,
            })
        }

        pub const fn wake_handle(&self) -> RuntimeWakeHandle {
            RuntimeWakeHandle
        }

        /// Waits for terminal input, termination, or the optional deadline.
        ///
        /// # Errors
        ///
        /// Returns an I/O error when terminal polling or input reading fails.
        pub fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<TerminalRuntimeEvent> {
            let event = read_event_timeout(timeout.unwrap_or(DEFAULT_IDLE_POLL))?;
            if let Some(signal) = self.termination.take() {
                return Ok(TerminalRuntimeEvent::Terminate(signal));
            }
            Ok(event.map_or(
                TerminalRuntimeEvent::Timeout,
                TerminalRuntimeEvent::Terminal,
            ))
        }
    }
}

pub use platform::{RuntimeWakeHandle, TerminalEventReactor};

#[cfg(test)]
mod tests {
    use super::TerminalRuntimeEvent;

    #[cfg(unix)]
    use super::{FragmentedEscape, decode_fragmented_escape};

    #[cfg(unix)]
    use crate::{Key, KeyKind, KeyStroke, Modifiers, TerminalEvent};

    #[cfg(unix)]
    fn character(character: char) -> TerminalEvent {
        TerminalEvent::Key(KeyStroke {
            key: Key::Character(character),
            modifiers: Modifiers::default(),
            kind: KeyKind::Press,
        })
    }

    #[test]
    fn runtime_events_separate_terminal_wake_timeout_and_termination() {
        assert!(matches!(
            TerminalRuntimeEvent::Wake,
            TerminalRuntimeEvent::Wake
        ));
        assert!(matches!(
            TerminalRuntimeEvent::Timeout,
            TerminalRuntimeEvent::Timeout
        ));
        assert!(matches!(
            TerminalRuntimeEvent::Terminate(15),
            TerminalRuntimeEvent::Terminate(15)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn terminal_interaction_conformance_fragmented_legacy_sequences_remain_atomic() {
        let f9 = ['[', '2', '0', '~'].map(character);
        assert_eq!(
            decode_fragmented_escape(&f9),
            FragmentedEscape::Complete(TerminalEvent::Key(KeyStroke {
                key: Key::Function(9),
                modifiers: Modifiers::default(),
                kind: KeyKind::Press,
            }))
        );

        let modified_page_down = ['[', '6', ';', '4', '~'].map(character);
        assert_eq!(
            decode_fragmented_escape(&modified_page_down),
            FragmentedEscape::Complete(TerminalEvent::Key(KeyStroke {
                key: Key::PageDown,
                modifiers: Modifiers {
                    shift: true,
                    alt: true,
                    ..Modifiers::default()
                },
                kind: KeyKind::Press,
            }))
        );

        assert_eq!(
            decode_fragmented_escape(&[character('c')]),
            FragmentedEscape::Complete(TerminalEvent::Key(KeyStroke {
                key: Key::Character('c'),
                modifiers: Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
                kind: KeyKind::Press,
            }))
        );
    }
}
