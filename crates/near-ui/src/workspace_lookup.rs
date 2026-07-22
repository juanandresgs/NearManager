use near_terminal::{Key, KeyStroke};
use ratatui::{
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::collection::name_matches_lookup;

use super::{
    CollectionLookupMode, FarWorkspace, Frame, Keymap, Rect, RoleBuffer, SemanticTheme,
    is_parent_entry, truncate,
};

impl FarWorkspace {
    pub(super) fn refresh_filename_lookup(&mut self) {
        let Some(lookup) = &self.filename_lookup else {
            return;
        };
        let (panel, original_cursor, query) =
            (lookup.panel, lookup.original_cursor, lookup.query.clone());
        let matches_for = |mode| {
            self.panel(panel)
                .entries()
                .iter()
                .enumerate()
                .filter(|(_, entry)| !is_parent_entry(entry))
                .filter(|(_, entry)| name_matches_lookup(&entry.metadata.name, &query, mode))
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        };
        let mut mode = CollectionLookupMode::Prefix;
        let mut matches = matches_for(mode);
        if matches.is_empty() {
            mode = CollectionLookupMode::Contains;
            matches = matches_for(mode);
        }
        let active_match = matches
            .iter()
            .position(|index| *index >= original_cursor)
            .unwrap_or_default();
        if let Some(lookup) = &mut self.filename_lookup {
            lookup.mode = mode;
            lookup.matches = matches;
            lookup.active_match = active_match;
        }
        self.panel_mut(panel).set_lookup(Some(query), mode);
        self.apply_filename_lookup_cursor();
    }

    pub(super) fn route_panel_lookup_before_shell(
        &mut self,
        stroke: &KeyStroke,
        keymap: &Keymap,
    ) -> bool {
        if self.overlay.is_some()
            || self.terminal_owns_focus()
            || self.quick_view_interactive
            || self.active_editor.is_some()
        {
            return false;
        }
        if self.filename_lookup.is_some() {
            return self.handle_filename_lookup_key(stroke);
        }
        if !stroke.modifiers.alt || stroke.modifiers.control || stroke.modifiers.super_key {
            return false;
        }
        let Key::Character(character) = stroke.key else {
            return false;
        };
        if keymap
            .bindings_for(&self.active_contexts())
            .iter()
            .any(|binding| binding.sequence.first() == Some(stroke))
        {
            return false;
        }
        self.start_filename_lookup(&character.to_string());
        true
    }

    pub(super) fn render_filename_lookup_status(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) -> bool {
        let Some(lookup) = &self.filename_lookup else {
            return false;
        };
        let position = if lookup.matches.is_empty() {
            "no matches".to_owned()
        } else {
            format!("{} of {}", lookup.active_match + 1, lookup.matches.len())
        };
        let movement = if lookup.matches.len() <= 1 {
            "↓ close"
        } else {
            "↑↓ next"
        };
        let mode = if lookup.mode == CollectionLookupMode::Contains {
            "contains "
        } else {
            ""
        };
        let text = format!(
            " Find: {}_   {mode}{position}   {movement}   Enter keep   Esc restore ",
            lookup.query
        );
        frame.render_widget(
            Paragraph::new(truncate(&text, area.width as usize)).style(theme.style("lookup.bar")),
            area,
        );
        roles.fill(area, "lookup.bar");
        true
    }

    pub(super) fn render_filename_lookup_keybar(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) -> bool {
        let Some(lookup) = &self.filename_lookup else {
            return false;
        };
        let movement = if lookup.matches.len() <= 1 {
            ("↓", " close  ")
        } else {
            ("↑↓", " next  ")
        };
        let mut spans = Vec::new();
        let mut column = area.x;
        for (key, label) in [
            movement,
            ("Enter", " keep  "),
            ("Esc", " restore  "),
            ("⌫", " edit"),
        ] {
            spans.push(Span::styled(
                key,
                theme.style("keybar.key").add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(label, theme.style("keybar.label")));
            let key_width = u16::try_from(key.chars().count()).unwrap_or(u16::MAX);
            let label_width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
            roles.fill(Rect::new(column, area.y, key_width, 1), "keybar.key");
            column = column.saturating_add(key_width);
            roles.fill(Rect::new(column, area.y, label_width, 1), "keybar.label");
            column = column.saturating_add(label_width);
        }
        if column < area.right() {
            roles.fill(
                Rect::new(column, area.y, area.right() - column, 1),
                "keybar.label",
            );
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        true
    }
}
