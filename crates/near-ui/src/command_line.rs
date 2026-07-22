#![allow(clippy::map_unwrap_or, clippy::single_match_else)]

use near_core::CommandHistoryEntry;

const MAX_UNLOCKED_HISTORY: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandLineState {
    buffer: String,
    history: Vec<CommandHistoryEntry>,
    history_index: Option<usize>,
    draft: String,
    max_unlocked_history: usize,
}

impl Default for CommandLineState {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            history: Vec::new(),
            history_index: None,
            draft: String::new(),
            max_unlocked_history: MAX_UNLOCKED_HISTORY,
        }
    }
}

impl CommandLineState {
    pub fn buffer(&self) -> &str {
        &self.buffer
    }
    pub fn entries(&self) -> &[CommandHistoryEntry] {
        &self.history
    }
    pub fn is_active(&self) -> bool {
        !self.buffer.is_empty() || self.history_index.is_some()
    }
    pub fn insert(&mut self, text: &str) {
        self.buffer.push_str(text);
        self.history_index = None;
    }
    pub fn set_buffer(&mut self, text: impl Into<String>) {
        self.buffer = text.into();
        self.history_index = None;
        self.draft.clear();
    }
    pub fn load_history(&mut self, entries: Vec<CommandHistoryEntry>) {
        self.history = entries;
        self.trim();
        self.history_index = None;
    }
    pub fn set_max_unlocked_history(&mut self, limit: usize) {
        self.max_unlocked_history = limit.max(1);
        self.trim();
    }
    pub fn backspace(&mut self) -> bool {
        let changed = self.buffer.pop().is_some();
        if changed {
            self.history_index = None;
        }
        changed
    }
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.history_index = None;
        self.draft.clear();
    }
    pub fn commit(&mut self) -> Option<String> {
        let command = self.buffer.trim().to_owned();
        if command.is_empty() {
            return None;
        }
        self.record(&command);
        self.clear();
        Some(command)
    }
    pub fn record(&mut self, command: &str) {
        let entry = self
            .history
            .iter()
            .position(|entry| entry.command == command)
            .map(|index| {
                let mut entry = self.history.remove(index);
                entry.use_count = entry.use_count.saturating_add(1);
                entry
            })
            .unwrap_or_else(|| CommandHistoryEntry::new(command));
        self.history.push(entry);
        self.trim();
        self.history_index = None;
    }
    pub fn toggle_lock(&mut self, command: &str) -> Option<bool> {
        let entry = self
            .history
            .iter_mut()
            .find(|entry| entry.command == command)?;
        entry.locked = !entry.locked;
        Some(entry.locked)
    }
    pub fn clear_unlocked_history(&mut self) -> usize {
        let before = self.history.len();
        self.history.retain(|entry| entry.locked);
        self.history_index = None;
        before.saturating_sub(self.history.len())
    }
    pub fn previous(&mut self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let index = match self.history_index {
            Some(index) => index.saturating_sub(1),
            None => {
                self.draft.clone_from(&self.buffer);
                self.history.len().saturating_sub(1)
            }
        };
        self.history_index = Some(index);
        self.buffer.clone_from(&self.history[index].command);
        true
    }
    pub fn next(&mut self) -> bool {
        let Some(index) = self.history_index else {
            return false;
        };
        if index + 1 < self.history.len() {
            self.history_index = Some(index + 1);
            self.buffer.clone_from(&self.history[index + 1].command);
        } else {
            self.history_index = None;
            self.buffer.clone_from(&self.draft);
        }
        true
    }
    fn trim(&mut self) {
        while self.history.iter().filter(|entry| !entry.locked).count() > self.max_unlocked_history
        {
            let Some(index) = self.history.iter().position(|entry| !entry.locked) else {
                break;
            };
            self.history.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use near_core::CommandHistoryEntry;

    use super::{CommandLineState, MAX_UNLOCKED_HISTORY};

    #[test]
    fn editing_commit_and_history_preserve_the_draft() {
        let mut state = CommandLineState::default();
        state.insert("printf one");
        assert_eq!(state.commit().as_deref(), Some("printf one"));
        state.insert("draft");
        assert!(state.previous());
        assert_eq!(state.buffer(), "printf one");
        assert!(state.next());
        assert_eq!(state.buffer(), "draft");
        assert!(state.backspace());
        assert_eq!(state.buffer(), "draf");
    }

    #[test]
    fn locked_entries_survive_history_trimming() {
        let mut locked = CommandHistoryEntry::new("locked");
        locked.locked = true;
        let mut state = CommandLineState::default();
        state.load_history(vec![locked]);
        for index in 0..=MAX_UNLOCKED_HISTORY {
            state.record(&format!("command-{index}"));
        }
        assert!(
            state
                .entries()
                .iter()
                .any(|entry| entry.command == "locked")
        );
        assert_eq!(
            state.entries().iter().filter(|entry| !entry.locked).count(),
            MAX_UNLOCKED_HISTORY
        );
    }
}
