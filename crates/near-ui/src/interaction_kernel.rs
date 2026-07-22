use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionInteractionModel {
    item_count: usize,
    cursor: usize,
    selected: BTreeSet<usize>,
    non_selectable: BTreeSet<usize>,
    viewport_start: usize,
    visible_rows: usize,
}

impl CollectionInteractionModel {
    pub fn new(
        item_count: usize,
        cursor: usize,
        selected: impl IntoIterator<Item = usize>,
        selectable: impl IntoIterator<Item = bool>,
        viewport_start: usize,
        visible_rows: usize,
    ) -> Self {
        let non_selectable = selectable
            .into_iter()
            .take(item_count)
            .enumerate()
            .filter_map(|(index, selectable)| (!selectable).then_some(index))
            .collect::<BTreeSet<_>>();
        let selected = selected
            .into_iter()
            .filter(|index| *index < item_count && !non_selectable.contains(index))
            .collect();
        let mut model = Self {
            item_count,
            cursor: cursor.min(item_count.saturating_sub(1)),
            selected,
            non_selectable,
            viewport_start,
            visible_rows: visible_rows.max(1),
        };
        model.clamp_viewport();
        model
    }

    pub fn item_count(&self) -> usize {
        self.item_count
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selected(&self) -> &BTreeSet<usize> {
        &self.selected
    }

    pub fn viewport_start(&self) -> usize {
        self.viewport_start
    }

    pub fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    fn move_by(&mut self, rows: isize) {
        if self.item_count() == 0 {
            self.cursor = 0;
            self.viewport_start = 0;
            return;
        }
        self.cursor = self
            .cursor
            .saturating_add_signed(rows)
            .min(self.item_count() - 1);
        self.ensure_cursor_visible();
    }

    fn first(&mut self) {
        self.cursor = 0;
        self.ensure_cursor_visible();
    }

    fn last(&mut self) {
        self.cursor = self.item_count().saturating_sub(1);
        self.ensure_cursor_visible();
    }

    fn page(&mut self, pages: isize) {
        if self.item_count() == 0 {
            self.cursor = 0;
            self.viewport_start = 0;
            return;
        }
        let rows = isize::try_from(self.visible_rows).unwrap_or(isize::MAX);
        let movement = rows.saturating_mul(pages);
        self.cursor = self
            .cursor
            .saturating_add_signed(movement)
            .min(self.item_count() - 1);
        self.viewport_start = self
            .viewport_start
            .saturating_add_signed(movement)
            .min(self.maximum_viewport_start());
        self.ensure_cursor_visible();
    }

    fn toggle_current(&mut self) -> bool {
        if self.item_count() == 0 || self.non_selectable.contains(&self.cursor) {
            return false;
        }
        if !self.selected.remove(&self.cursor) {
            self.selected.insert(self.cursor);
        }
        true
    }

    pub(crate) fn navigation(
        item_count: usize,
        cursor: usize,
        viewport_start: usize,
        visible_rows: usize,
    ) -> Self {
        let mut model = Self {
            item_count,
            cursor: cursor.min(item_count.saturating_sub(1)),
            selected: BTreeSet::new(),
            non_selectable: BTreeSet::new(),
            viewport_start,
            visible_rows: visible_rows.max(1),
        };
        model.clamp_viewport();
        model
    }

    pub(crate) fn current_item(
        item_count: usize,
        cursor: usize,
        selected: bool,
        selectable: bool,
        viewport_start: usize,
        visible_rows: usize,
    ) -> Self {
        let mut model = Self {
            item_count,
            cursor: cursor.min(item_count.saturating_sub(1)),
            selected: selected.then_some(cursor).into_iter().collect(),
            non_selectable: (!selectable).then_some(cursor).into_iter().collect(),
            viewport_start,
            visible_rows: visible_rows.max(1),
        };
        model.clamp_viewport();
        model
    }

    fn maximum_viewport_start(&self) -> usize {
        self.item_count().saturating_sub(self.visible_rows)
    }

    fn clamp_viewport(&mut self) {
        self.viewport_start = self.viewport_start.min(self.maximum_viewport_start());
        self.ensure_cursor_visible();
    }

    fn ensure_cursor_visible(&mut self) {
        if self.item_count() == 0 {
            self.viewport_start = 0;
            return;
        }
        if self.cursor < self.viewport_start {
            self.viewport_start = self.cursor;
        } else if self.cursor >= self.viewport_start.saturating_add(self.visible_rows) {
            self.viewport_start = self
                .cursor
                .saturating_add(1)
                .saturating_sub(self.visible_rows);
        }
        self.viewport_start = self.viewport_start.min(self.maximum_viewport_start());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionInteractionMsg {
    Move(isize),
    First,
    Last,
    Page(isize),
    ToggleCurrent,
    ToggleCurrentAndMove(isize),
    SetCursor(usize),
    SetVisibleRows(usize),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CollectionInteractionEffect {
    pub changed: bool,
    pub selection_changed: bool,
    pub selection_denied: bool,
}

pub fn update_collection_interaction(
    model: &mut CollectionInteractionModel,
    message: CollectionInteractionMsg,
) -> CollectionInteractionEffect {
    let before = model.clone();
    let mut selection_changed = false;
    let mut selection_denied = false;
    match message {
        CollectionInteractionMsg::Move(rows) => model.move_by(rows),
        CollectionInteractionMsg::First => model.first(),
        CollectionInteractionMsg::Last => model.last(),
        CollectionInteractionMsg::Page(pages) => model.page(pages),
        CollectionInteractionMsg::ToggleCurrent => {
            selection_changed = model.toggle_current();
            selection_denied = !selection_changed && model.item_count() > 0;
        }
        CollectionInteractionMsg::ToggleCurrentAndMove(rows) => {
            selection_changed = model.toggle_current();
            selection_denied = !selection_changed && model.item_count() > 0;
            model.move_by(rows);
        }
        CollectionInteractionMsg::SetCursor(cursor) => {
            model.cursor = cursor.min(model.item_count().saturating_sub(1));
            model.ensure_cursor_visible();
        }
        CollectionInteractionMsg::SetVisibleRows(rows) => {
            model.visible_rows = rows.max(1);
            model.clamp_viewport();
        }
    }
    CollectionInteractionEffect {
        changed: *model != before,
        selection_changed,
        selection_denied,
    }
}
