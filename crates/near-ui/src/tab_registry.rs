#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TabId(u64);

impl TabId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

pub struct TabEntry<T> {
    id: TabId,
    title: String,
    value: T,
}

impl<T> TabEntry<T> {
    pub const fn id(&self) -> TabId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub const fn value(&self) -> &T {
        &self.value
    }

    pub const fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

pub struct TabRegistry<T> {
    entries: Vec<TabEntry<T>>,
    active: Option<usize>,
    next_id: u64,
}

impl<T> Default for TabRegistry<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            active: None,
            next_id: 1,
        }
    }
}

impl<T> TabRegistry<T> {
    pub fn insert(&mut self, title: impl Into<String>, value: T) -> TabId {
        let id = TabId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.entries.push(TabEntry {
            id,
            title: title.into(),
            value,
        });
        self.active = Some(self.entries.len() - 1);
        id
    }

    pub fn entries(&self) -> &[TabEntry<T>] {
        &self.entries
    }

    pub fn active(&self) -> Option<&TabEntry<T>> {
        self.active.and_then(|index| self.entries.get(index))
    }

    pub fn active_mut(&mut self) -> Option<&mut TabEntry<T>> {
        self.active.and_then(|index| self.entries.get_mut(index))
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active().map(TabEntry::id)
    }

    pub const fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn select(&mut self, id: TabId) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        self.active = Some(index);
        true
    }

    pub fn cycle(&mut self, direction: isize) -> Option<TabId> {
        if self.entries.is_empty() {
            self.active = None;
            return None;
        }
        let current = self.active.unwrap_or_default();
        let next = if direction < 0 {
            (current + self.entries.len() - 1) % self.entries.len()
        } else {
            (current + 1) % self.entries.len()
        };
        self.active = Some(next);
        Some(self.entries[next].id)
    }

    pub fn remove_active(&mut self) -> Option<TabEntry<T>> {
        let index = self.active?;
        let removed = self.entries.remove(index);
        self.active = if self.entries.is_empty() {
            None
        } else {
            Some(index.min(self.entries.len() - 1))
        };
        Some(removed)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaneSlot {
    #[default]
    First,
    Second,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ZoomablePanePresentation {
    #[default]
    Base,
    Pane(PaneSlot),
    FullScreen {
        restore: Option<PaneSlot>,
    },
}

impl ZoomablePanePresentation {
    pub const fn pane(self) -> Option<PaneSlot> {
        match self {
            Self::Pane(slot) => Some(slot),
            Self::Base | Self::FullScreen { .. } => None,
        }
    }

    pub const fn is_full_screen(self) -> bool {
        matches!(self, Self::FullScreen { .. })
    }

    pub fn place(&mut self, slot: PaneSlot) {
        *self = Self::Pane(slot);
    }

    pub fn hide(&mut self) {
        *self = Self::Base;
    }

    pub fn toggle_zoom(&mut self) {
        *self = match *self {
            Self::Base => Self::FullScreen { restore: None },
            Self::Pane(slot) => Self::FullScreen {
                restore: Some(slot),
            },
            Self::FullScreen { restore } => restore.map_or(Self::Base, Self::Pane),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{PaneSlot, TabRegistry, ZoomablePanePresentation};

    #[test]
    fn tabs_cycle_close_and_retain_stable_identity() {
        let mut tabs = TabRegistry::default();
        let first = tabs.insert("one", 1);
        let second = tabs.insert("two", 2);
        assert_eq!(tabs.active_id(), Some(second));
        assert_eq!(tabs.cycle(1), Some(first));
        assert_eq!(*tabs.remove_active().unwrap().value(), 1);
        assert_eq!(tabs.active_id(), Some(second));
    }

    #[test]
    fn pane_zoom_restores_the_exact_previous_composition() {
        let mut presentation = ZoomablePanePresentation::default();
        presentation.place(PaneSlot::Second);
        presentation.toggle_zoom();
        assert!(presentation.is_full_screen());
        presentation.toggle_zoom();
        assert_eq!(presentation.pane(), Some(PaneSlot::Second));
        presentation.hide();
        presentation.toggle_zoom();
        presentation.toggle_zoom();
        assert_eq!(presentation, ZoomablePanePresentation::Base);
    }
}
