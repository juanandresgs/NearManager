#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use near_ui::{
        CollectionInteractionModel, CollectionInteractionMsg, update_collection_interaction,
    };

    fn model() -> CollectionInteractionModel {
        CollectionInteractionModel::new(8, 2, [0, 5], [true; 8], 0, 3)
    }

    #[test]
    fn far_edge_page_and_plain_navigation_preserve_explicit_selection() {
        let mut state = model();
        update_collection_interaction(&mut state, CollectionInteractionMsg::First);
        assert_eq!(state.cursor(), 0);
        update_collection_interaction(&mut state, CollectionInteractionMsg::Last);
        assert_eq!(state.cursor(), 7);
        assert_eq!(state.viewport_start(), 5);
        update_collection_interaction(&mut state, CollectionInteractionMsg::Page(-1));
        assert_eq!(state.cursor(), 4);
        assert_eq!(state.selected().iter().copied().collect::<Vec<_>>(), [0, 5]);
    }

    #[test]
    fn shifted_navigation_toggles_each_visited_item_and_allows_gaps() {
        let mut state = CollectionInteractionModel::new(6, 0, [], [true; 6], 0, 4);
        update_collection_interaction(
            &mut state,
            CollectionInteractionMsg::ToggleCurrentAndMove(1),
        );
        update_collection_interaction(&mut state, CollectionInteractionMsg::Move(2));
        update_collection_interaction(
            &mut state,
            CollectionInteractionMsg::ToggleCurrentAndMove(1),
        );
        assert_eq!(state.cursor(), 4);
        assert_eq!(state.selected().iter().copied().collect::<Vec<_>>(), [0, 3]);
    }

    #[test]
    fn non_selectable_items_move_without_entering_selection() {
        let mut state = CollectionInteractionModel::new(3, 0, [], [false, true, true], 0, 2);
        let effect = update_collection_interaction(
            &mut state,
            CollectionInteractionMsg::ToggleCurrentAndMove(1),
        );
        assert!(effect.selection_denied);
        assert!(!effect.selection_changed);
        assert_eq!(state.cursor(), 1);
        assert!(state.selected().is_empty());
    }

    #[test]
    fn empty_collections_are_stable_for_every_navigation_message() {
        let mut state = CollectionInteractionModel::new(0, 4, [], [], 8, 0);
        for message in [
            CollectionInteractionMsg::Move(1),
            CollectionInteractionMsg::First,
            CollectionInteractionMsg::Last,
            CollectionInteractionMsg::Page(1),
            CollectionInteractionMsg::ToggleCurrent,
        ] {
            update_collection_interaction(&mut state, message);
        }
        assert_eq!(state.cursor(), 0);
        assert_eq!(state.viewport_start(), 0);
        assert!(state.selected().is_empty());
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct ReferenceState {
        count: usize,
        cursor: usize,
        selected: BTreeSet<usize>,
        selectable: Vec<bool>,
        start: usize,
        rows: usize,
    }

    impl ReferenceState {
        fn apply(&mut self, message: CollectionInteractionMsg) {
            match message {
                CollectionInteractionMsg::Move(rows) => self.move_by(rows),
                CollectionInteractionMsg::First => self.cursor = 0,
                CollectionInteractionMsg::Last => self.cursor = self.count.saturating_sub(1),
                CollectionInteractionMsg::Page(pages) => {
                    self.ensure_visible();
                    let movement = isize::try_from(self.rows)
                        .unwrap_or(isize::MAX)
                        .saturating_mul(pages);
                    self.cursor = self
                        .cursor
                        .saturating_add_signed(movement)
                        .min(self.count.saturating_sub(1));
                    self.start = self
                        .start
                        .saturating_add_signed(movement)
                        .min(self.count.saturating_sub(self.rows));
                }
                CollectionInteractionMsg::ToggleCurrent => self.toggle(),
                CollectionInteractionMsg::ToggleCurrentAndMove(rows) => {
                    self.toggle();
                    self.move_by(rows);
                }
                CollectionInteractionMsg::SetCursor(cursor) => {
                    self.cursor = cursor.min(self.count.saturating_sub(1));
                }
                CollectionInteractionMsg::SetVisibleRows(rows) => self.rows = rows.max(1),
            }
            self.ensure_visible();
        }

        fn move_by(&mut self, rows: isize) {
            self.cursor = self
                .cursor
                .saturating_add_signed(rows)
                .min(self.count.saturating_sub(1));
        }

        fn toggle(&mut self) {
            if self.count == 0 || !self.selectable[self.cursor] {
                return;
            }
            if !self.selected.remove(&self.cursor) {
                self.selected.insert(self.cursor);
            }
        }

        fn ensure_visible(&mut self) {
            if self.count == 0 {
                self.cursor = 0;
                self.start = 0;
                return;
            }
            self.rows = self.rows.max(1);
            self.start = self.start.min(self.count.saturating_sub(self.rows));
            if self.cursor < self.start {
                self.start = self.cursor;
            } else if self.cursor >= self.start.saturating_add(self.rows) {
                self.start = self.cursor.saturating_add(1).saturating_sub(self.rows);
            }
        }
    }

    #[test]
    fn extracted_kernel_matches_reference_transition_grammar_exhaustively() {
        let messages = [
            CollectionInteractionMsg::Move(-1),
            CollectionInteractionMsg::Move(1),
            CollectionInteractionMsg::First,
            CollectionInteractionMsg::Last,
            CollectionInteractionMsg::Page(-1),
            CollectionInteractionMsg::Page(1),
            CollectionInteractionMsg::ToggleCurrent,
            CollectionInteractionMsg::ToggleCurrentAndMove(-1),
            CollectionInteractionMsg::ToggleCurrentAndMove(1),
        ];
        for count in 0..=12 {
            for rows in 1..=5 {
                for cursor in 0..count.max(1) {
                    let selectable = (0..count).map(|index| index % 4 != 0).collect::<Vec<_>>();
                    let selected = (0..count)
                        .filter(|index| index % 3 == 1 && selectable[*index])
                        .collect::<BTreeSet<_>>();
                    for message in messages {
                        let mut reference = ReferenceState {
                            count,
                            cursor,
                            selected: selected.clone(),
                            selectable: selectable.clone(),
                            start: cursor.saturating_sub(rows / 2),
                            rows,
                        };
                        reference.ensure_visible();
                        let mut extracted = CollectionInteractionModel::new(
                            count,
                            cursor,
                            selected.iter().copied(),
                            selectable.iter().copied(),
                            reference.start,
                            rows,
                        );
                        reference.apply(message);
                        update_collection_interaction(&mut extracted, message);
                        assert_eq!(
                            (
                                extracted.cursor(),
                                extracted.selected().clone(),
                                extracted.viewport_start(),
                                extracted.visible_rows(),
                            ),
                            (
                                reference.cursor,
                                reference.selected,
                                reference.start,
                                reference.rows,
                            ),
                            "count={count} rows={rows} cursor={cursor} message={message:?}"
                        );
                    }
                }
            }
        }
    }
}
