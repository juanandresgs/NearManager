use std::cell::Cell;

#[derive(Clone, Debug, Default)]
pub struct ListNavigation {
    cursor: usize,
    start: Cell<usize>,
    visible_rows: Cell<usize>,
}

impl ListNavigation {
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
    }

    pub fn visible_rows(&self) -> usize {
        self.visible_rows.get().max(1)
    }

    pub fn move_by(&mut self, visible: &[usize], rows: isize) {
        if visible.is_empty() {
            self.cursor = 0;
            self.start.set(0);
            return;
        }
        let position = visible
            .iter()
            .position(|index| *index == self.cursor)
            .unwrap_or_default();
        self.cursor = visible[position
            .saturating_add_signed(rows)
            .min(visible.len().saturating_sub(1))];
    }

    pub fn first(&mut self, visible: &[usize]) {
        if let Some(first) = visible.first() {
            self.cursor = *first;
        }
    }

    pub fn last(&mut self, visible: &[usize]) {
        if let Some(last) = visible.last() {
            self.cursor = *last;
        }
    }

    pub fn page(&mut self, visible: &[usize], pages: isize) {
        let rows = isize::try_from(self.visible_rows()).unwrap_or(isize::MAX);
        self.move_by(visible, rows.saturating_mul(pages));
    }

    pub fn window<'a>(&self, visible: &'a [usize], rows: usize) -> &'a [usize] {
        let rows = rows.max(1);
        self.visible_rows.set(rows);
        if visible.is_empty() {
            self.start.set(0);
            return visible;
        }
        let cursor_position = visible
            .iter()
            .position(|index| *index == self.cursor)
            .unwrap_or_default();
        let maximum = visible.len().saturating_sub(rows);
        let mut start = self.start.get().min(maximum);
        if cursor_position < start {
            start = cursor_position;
        } else if cursor_position >= start.saturating_add(rows) {
            start = cursor_position.saturating_add(1).saturating_sub(rows);
        }
        self.start.set(start);
        &visible[start..visible.len().min(start.saturating_add(rows))]
    }

    pub fn select_visible_row(&mut self, visible: &[usize], row: usize) -> bool {
        let Some(index) = self.start.get().checked_add(row) else {
            return false;
        };
        let Some(cursor) = visible.get(index) else {
            return false;
        };
        self.cursor = *cursor;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ListNavigation;

    #[test]
    fn paging_filtering_and_pointer_rows_share_one_visible_window() {
        let visible = [1, 3, 5, 7, 9, 11, 13];
        let mut navigation = ListNavigation::default();
        navigation.set_cursor(1);
        assert_eq!(navigation.window(&visible, 3), &[1, 3, 5]);
        navigation.page(&visible, 1);
        assert_eq!(navigation.cursor(), 7);
        assert_eq!(navigation.window(&visible, 3), &[3, 5, 7]);
        assert!(navigation.select_visible_row(&visible, 0));
        assert_eq!(navigation.cursor(), 3);
        navigation.last(&visible);
        assert_eq!(navigation.window(&visible, 3), &[9, 11, 13]);
        navigation.first(&visible);
        assert_eq!(navigation.cursor(), 1);
    }
}
