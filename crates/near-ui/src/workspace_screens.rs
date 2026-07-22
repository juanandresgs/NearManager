use std::collections::BTreeMap;

use near_core::{CommandInvocation, CommandValue};

use super::{
    FarWorkspace, FocusedPanel, MenuItem, MenuSurface, Overlay, PanelType, ZoomablePanePresentation,
};

impl FarWorkspace {
    pub(super) fn focus_peer(&mut self) {
        if self.terminal_is_full_screen() {
            self.status =
                "No peer pane is visible; place the terminal left or right before switching focus"
                    .to_owned();
            return;
        }
        let target = match self.focused {
            FocusedPanel::Left => FocusedPanel::Right,
            FocusedPanel::Right => FocusedPanel::Left,
        };
        if self.panel_type(target) == PanelType::QuickView {
            "Quick view remains the passive panel".clone_into(&mut self.status);
        } else {
            self.focused = target;
            self.status = format!("Focused {} panel", self.focus_name());
            self.refresh_quick_view();
        }
    }

    pub(super) fn show_screen_list(&mut self) {
        let mut items = vec![MenuItem {
            label: numbered_screen_label(1, "Panels"),
            description: if self.active_editor.is_none() && !self.terminal_is_full_screen() {
                "active screen".to_owned()
            } else {
                "dual file workspace".to_owned()
            },
            command: CommandInvocation {
                id: "near.screen.panels".into(),
                arguments: BTreeMap::new(),
            },
            enabled: true,
        }];
        items.extend(
            self.editors
                .iter()
                .enumerate()
                .map(|(index, editor)| MenuItem {
                    label: numbered_screen_label(index + 2, &format!("Editor: {}", editor.title())),
                    description: format!(
                        "{}{}",
                        editor.resource().location.as_str(),
                        if Some(index) == self.active_editor {
                            " • active"
                        } else {
                            ""
                        }
                    ),
                    command: CommandInvocation {
                        id: "near.screen.editor".into(),
                        arguments: BTreeMap::from([(
                            "index".to_owned(),
                            CommandValue::Integer(i64::try_from(index).unwrap_or(i64::MAX)),
                        )]),
                    },
                    enabled: true,
                }),
        );
        let terminal_start = self.editors.len() + 2;
        items.extend(
            self.terminals
                .entries()
                .iter()
                .enumerate()
                .map(|(index, terminal)| MenuItem {
                    label: numbered_screen_label(
                        terminal_start + index,
                        &format!("Terminal: {}", terminal.title()),
                    ),
                    description: format!(
                        "{}{}",
                        self.terminal_process_description(terminal.id()),
                        if self.terminals.active_id() == Some(terminal.id())
                            && self.terminal_is_full_screen()
                        {
                            " • active"
                        } else {
                            ""
                        }
                    ),
                    command: CommandInvocation {
                        id: "near.screen.terminal".into(),
                        arguments: BTreeMap::from([(
                            "tab".to_owned(),
                            CommandValue::Integer(
                                i64::try_from(terminal.id().get()).unwrap_or(i64::MAX),
                            ),
                        )]),
                    },
                    enabled: true,
                }),
        );
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.screens",
            "Screens",
            items,
        )));
    }

    pub(super) fn activate_editor_screen(&mut self, invocation: &CommandInvocation) {
        let Some(index) = invocation
            .arguments
            .get("index")
            .and_then(CommandValue::as_i64)
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < self.editors.len())
        else {
            "Editor screen is unavailable".clone_into(&mut self.status);
            return;
        };
        self.persist_active_editor_position();
        self.terminal_presentation.hide();
        self.suspended_overlay = None;
        self.active_editor = Some(index);
        self.overlay = None;
        self.status = format!("Editor: {}", self.editors[index].title());
    }

    pub(super) fn cycle_editor_screen(&mut self, direction: isize) {
        if self.editors.is_empty() && self.terminals.is_empty() {
            self.active_editor = None;
            "No additional screens".clone_into(&mut self.status);
            return;
        }
        self.persist_active_editor_position();
        let terminal_index = self.editors.len() + 1;
        let screen_count = terminal_index + self.terminals.len();
        let current = if self.terminal_is_full_screen() {
            terminal_index + self.terminals.active_index().unwrap_or_default()
        } else {
            self.active_editor.map_or(0, |index| index + 1)
        };
        let next = if direction < 0 {
            (current + screen_count - 1) % screen_count
        } else {
            (current + 1) % screen_count
        };
        self.overlay = None;
        if next == 0 {
            self.active_editor = None;
            self.terminal_presentation.hide();
            self.suspended_overlay = None;
            "Panels screen".clone_into(&mut self.status);
        } else if next >= terminal_index {
            if let Some(terminal) = self.terminals.entries().get(next - terminal_index) {
                let id = terminal.id();
                self.terminals.select(id);
                self.activate_terminal_screen();
            }
        } else {
            let index = next - 1;
            self.terminal_presentation.hide();
            self.suspended_overlay = None;
            self.active_editor = Some(index);
            self.status = format!("Editor: {}", self.editors[index].title());
        }
    }

    pub(super) fn activate_terminal_screen(&mut self) {
        if self.terminals.is_empty() {
            "User screen is unavailable".clone_into(&mut self.status);
            return;
        }
        if self.terminal_is_full_screen() {
            self.overlay = None;
            "User screen".clone_into(&mut self.status);
            return;
        }
        self.persist_active_editor_position();
        self.overlay = None;
        self.suspended_overlay = None;
        self.quick_view_interactive = false;
        self.terminal_presentation = ZoomablePanePresentation::FullScreen { restore: None };
        let title = self
            .terminals
            .active()
            .map_or("Terminal", |terminal| terminal.title());
        self.status = format!("Terminal: {title}");
    }

    pub(super) fn activate_terminal_tab(&mut self, invocation: &CommandInvocation) {
        if let Some(id) = invocation
            .arguments
            .get("tab")
            .and_then(CommandValue::as_i64)
            .and_then(|id| u64::try_from(id).ok())
            .map(crate::TabId::from_raw)
        {
            self.terminals.select(id);
        }
        self.activate_terminal_screen();
    }
}

fn numbered_screen_label(number: usize, title: &str) -> String {
    if number < 10 {
        format!("&{number} {title}")
    } else {
        format!("{number} {title}")
    }
}
