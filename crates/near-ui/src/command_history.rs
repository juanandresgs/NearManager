#![allow(clippy::match_same_arms)]

use std::collections::BTreeMap;

use near_core::{
    CapabilitySet, CommandHistoryEntry, CommandId, CommandInvocation, CommandValue, ContextId,
    SurfaceId,
};

use crate::{
    ListNavigation, RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState,
    UpdateContext, UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

pub struct CommandHistorySurface {
    id: SurfaceId,
    entries: Vec<CommandHistoryEntry>,
    navigation: ListNavigation,
    filter: SelectionSearch,
}

impl CommandHistorySurface {
    pub fn new(entries: impl IntoIterator<Item = CommandHistoryEntry>) -> Self {
        Self {
            id: SurfaceId::from("near-fm.command-history"),
            entries: entries.into_iter().collect(),
            navigation: ListNavigation::default(),
            filter: SelectionSearch::default(),
        }
    }

    pub fn set_locked(&mut self, command: &str, locked: bool) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.command == command)
        {
            entry.locked = locked;
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.filter.matches([entry.command.as_str()]))
            .map(|(index, _)| index)
            .collect()
    }

    fn move_by(&mut self, rows: isize) {
        let visible = self.visible_indices();
        self.navigation.move_by(&visible, rows);
    }

    fn selected_entry(&self) -> Option<&CommandHistoryEntry> {
        self.entries.get(self.navigation.cursor())
    }

    fn dispatch_selected(&self, command: &str) -> UpdateResult {
        self.selected_entry()
            .map_or_else(UpdateResult::ignored, |entry| {
                UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from(command),
                    arguments: BTreeMap::from([(
                        "command".to_owned(),
                        CommandValue::String(entry.command.clone()),
                    )]),
                })
            })
    }
}

impl Surface for CommandHistorySurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.command-history")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        match event {
            SurfaceEvent::Text(text)
            | SurfaceEvent::Paste(text)
            | SurfaceEvent::SelectionSearchText(text) => {
                self.filter.push(text);
                self.move_by(0);
                UpdateResult::handled()
            }
            SurfaceEvent::Backspace => {
                self.filter.pop();
                self.move_by(0);
                UpdateResult::handled()
            }
            SurfaceEvent::SelectionSearchBackspace => {
                self.filter.pop();
                self.move_by(0);
                UpdateResult::handled()
            }
            SurfaceEvent::Command(invocation) => match invocation.id.as_str() {
                "near.command-history.up" => {
                    self.move_by(-1);
                    UpdateResult::handled()
                }
                "near.command-history.down" => {
                    self.move_by(1);
                    UpdateResult::handled()
                }
                "near.command-history.first" => {
                    let visible = self.visible_indices();
                    self.navigation.first(&visible);
                    UpdateResult::handled()
                }
                "near.command-history.last" => {
                    let visible = self.visible_indices();
                    self.navigation.last(&visible);
                    UpdateResult::handled()
                }
                "near.command-history.page-up" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, -1);
                    UpdateResult::handled()
                }
                "near.command-history.page-down" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, 1);
                    UpdateResult::handled()
                }
                "near.command-history.use" => {
                    self.dispatch_selected("near.command-line.history-use")
                }
                "near.command-history.toggle-lock" => {
                    self.dispatch_selected("near.command-line.history-toggle-lock")
                }
                "near.command-history.clear-unlocked" => {
                    UpdateResult::dispatch(CommandInvocation {
                        id: CommandId::from("near.command-line.history-clear-unlocked"),
                        arguments: BTreeMap::new(),
                    })
                }
                _ => UpdateResult::ignored(),
            },
            _ => UpdateResult::ignored(),
        }
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "dialog.background");
        scene.border(area, Some(" Command History ".to_owned()), "dialog.border");
        let inner = area.inset(1);
        let visible = self.visible_indices();
        let rows = usize::from(
            inner
                .height
                .saturating_sub(1 + u16::from(self.filter.is_active())),
        );
        for (row, index) in self
            .navigation
            .window(&visible, rows)
            .iter()
            .copied()
            .enumerate()
        {
            let Ok(row) = u16::try_from(row) else { break };
            let footer_rows = 1 + u16::from(self.filter.is_active());
            if row >= inner.height.saturating_sub(footer_rows) {
                break;
            }
            let entry = &self.entries[index];
            let role = if index == self.navigation.cursor() {
                "control.focused"
            } else {
                "dialog.background"
            };
            let content = format!(
                "{} {:>4} {}",
                if entry.locked { "◆" } else { " " },
                entry.use_count,
                entry.command
            );
            searchable_text(
                &mut scene,
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                &content,
                role,
                &self.filter,
            );
        }
        if self.filter.is_active() {
            scene.text(
                SceneRect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
                self.filter.prompt(),
                "text",
            );
        }
        scene.text(
            SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            "Enter Use   Space Lock   Delete Clear unlocked   Esc Close",
            "text",
        );
        scene
    }
}

#[cfg(test)]
mod tests {
    use near_core::ActionContext;

    use super::*;

    #[test]
    fn surface_interaction_conformance_histories_page_and_keep_focus_visible() {
        let mut surface = CommandHistorySurface::new(
            (0..12).map(|index| CommandHistoryEntry::new(format!("command-{index:02}"))),
        );
        let action = ActionContext::default();
        surface.scene(
            SceneRect::new(0, 0, 50, 6),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        for command in [
            "near.command-history.page-down",
            "near.command-history.last",
        ] {
            surface.update(
                &SurfaceEvent::Command(CommandInvocation {
                    id: CommandId::from(command),
                    arguments: BTreeMap::new(),
                }),
                &mut UpdateContext { action: &action },
            );
        }
        assert_eq!(surface.navigation.cursor(), 11);
        let scene = surface.scene(
            SceneRect::new(0, 0, 50, 6),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| {
            matches!(primitive, crate::ScenePrimitive::Text { content, role, .. } if content.contains("command-11") && role.as_str() == "control.focused")
        }));
    }
}
