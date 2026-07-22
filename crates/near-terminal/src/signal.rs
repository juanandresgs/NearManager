use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use signal_hook::{SigId, consts::SIGTERM, flag, low_level};

#[cfg(windows)]
use signal_hook::consts::SIGINT;
#[cfg(unix)]
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT};

pub struct TerminationWatcher {
    signal: Arc<AtomicUsize>,
    registrations: Vec<SigId>,
}

impl TerminationWatcher {
    /// Registers non-blocking observation for common termination signals.
    ///
    /// # Errors
    ///
    /// Returns an error if the operating system signal handlers cannot be installed.
    pub fn register() -> std::io::Result<Self> {
        let signal = Arc::new(AtomicUsize::new(0));
        let mut registrations = Vec::new();
        for number in termination_signals() {
            let value = usize::try_from(number).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "POSIX signal number must be positive",
                )
            })?;
            match flag::register_usize(number, Arc::clone(&signal), value) {
                Ok(registration) => registrations.push(registration),
                Err(error) => {
                    for registration in registrations.drain(..) {
                        low_level::unregister(registration);
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            signal,
            registrations,
        })
    }

    pub fn pending(&self) -> Option<i32> {
        let signal = self.signal.load(Ordering::Relaxed);
        (signal != 0).then(|| i32::try_from(signal).unwrap_or(SIGTERM))
    }

    pub fn take(&self) -> Option<i32> {
        let signal = self.signal.swap(0, Ordering::Relaxed);
        (signal != 0).then(|| i32::try_from(signal).unwrap_or(SIGTERM))
    }

    #[cfg(test)]
    fn notify(&self, signal: i32) {
        self.signal.store(
            usize::try_from(signal).expect("test signal numbers are positive"),
            Ordering::Relaxed,
        );
    }
}

#[cfg(unix)]
pub(crate) fn termination_signals() -> [i32; 4] {
    [SIGHUP, SIGINT, SIGQUIT, SIGTERM]
}

#[cfg(windows)]
pub(crate) fn termination_signals() -> [i32; 2] {
    [SIGINT, SIGTERM]
}

impl Drop for TerminationWatcher {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            low_level::unregister(registration);
        }
    }
}

#[cfg(test)]
mod tests {
    use signal_hook::consts::SIGTERM;

    use super::TerminationWatcher;

    #[test]
    fn notification_is_observable_and_consumable() {
        let watcher = TerminationWatcher::register().unwrap();
        watcher.notify(SIGTERM);
        assert_eq!(watcher.pending(), Some(SIGTERM));
        assert_eq!(watcher.take(), Some(SIGTERM));
        assert_eq!(watcher.pending(), None);
    }
}
