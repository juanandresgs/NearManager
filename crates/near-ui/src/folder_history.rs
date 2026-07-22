#![allow(clippy::match_same_arms)]

use std::collections::BTreeMap;

use near_core::{
    CapabilitySet, CommandId, CommandInvocation, CommandValue, ContextId, FolderLocationEntry,
    FolderNavigationState, SurfaceId,
};

use crate::{
    ListNavigation, RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState,
    UpdateContext, UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

#[derive(Clone)]
struct FolderHistoryItem {
    shortcut: Option<usize>,
    entry: FolderLocationEntry,
}

pub struct FolderHistorySurface {
    id: SurfaceId,
    items: Vec<FolderHistoryItem>,
    navigation: ListNavigation,
    filter: SelectionSearch,
}

impl FolderHistorySurface {
    pub fn new(state: &FolderNavigationState) -> Self {
        let mut items = state
            .shortcuts
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                entry.clone().map(|entry| FolderHistoryItem {
                    shortcut: Some(slot),
                    entry,
                })
            })
            .collect::<Vec<_>>();
        items.extend(
            state
                .history
                .iter()
                .rev()
                .cloned()
                .map(|entry| FolderHistoryItem {
                    shortcut: None,
                    entry,
                }),
        );
        Self {
            id: SurfaceId::from("near-fm.folder-history"),
            items,
            navigation: ListNavigation::default(),
            filter: SelectionSearch::default(),
        }
    }

    pub fn update_error(&mut self, provider: &str, location: &str, error: Option<&str>) {
        for item in &mut self.items {
            if item.entry.provider.as_str() == provider && item.entry.location.as_str() == location
            {
                item.entry.last_error = error.map(str::to_owned);
            }
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                self.filter
                    .matches([item.entry.label.as_str(), item.entry.location.as_str()])
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn move_by(&mut self, rows: isize) {
        let visible = self.visible_indices();
        self.navigation.move_by(&visible, rows);
    }

    fn activate(&self) -> UpdateResult {
        self.items
            .get(self.navigation.cursor())
            .map_or_else(UpdateResult::ignored, |item| {
                UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from("near.location.history-open"),
                    arguments: BTreeMap::from([
                        (
                            "provider".to_owned(),
                            CommandValue::String(item.entry.provider.to_string()),
                        ),
                        (
                            "location".to_owned(),
                            CommandValue::String(item.entry.location.as_str().to_owned()),
                        ),
                    ]),
                })
            })
    }

    fn toggle_lock(&self) -> UpdateResult {
        self.items
            .get(self.navigation.cursor())
            .map_or_else(UpdateResult::ignored, |item| {
                UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from("near.location.history-toggle-lock"),
                    arguments: BTreeMap::from([
                        (
                            "provider".to_owned(),
                            CommandValue::String(item.entry.provider.to_string()),
                        ),
                        (
                            "location".to_owned(),
                            CommandValue::String(item.entry.location.as_str().to_owned()),
                        ),
                    ]),
                })
            })
    }
}

impl Surface for FolderHistorySurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.folder-history")]
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
                "near.folder-history.up" => {
                    self.move_by(-1);
                    UpdateResult::handled()
                }
                "near.folder-history.down" => {
                    self.move_by(1);
                    UpdateResult::handled()
                }
                "near.folder-history.first" => {
                    let visible = self.visible_indices();
                    self.navigation.first(&visible);
                    UpdateResult::handled()
                }
                "near.folder-history.last" => {
                    let visible = self.visible_indices();
                    self.navigation.last(&visible);
                    UpdateResult::handled()
                }
                "near.folder-history.page-up" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, -1);
                    UpdateResult::handled()
                }
                "near.folder-history.page-down" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, 1);
                    UpdateResult::handled()
                }
                "near.folder-history.activate" => self.activate(),
                "near.folder-history.toggle-lock" => self.toggle_lock(),
                "near.folder-history.clear" => UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from("near.location.history-clear"),
                    arguments: BTreeMap::new(),
                }),
                _ => UpdateResult::ignored(),
            },
            _ => UpdateResult::ignored(),
        }
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "dialog.background");
        scene.border(area, Some(" Folder History ".to_owned()), "dialog.border");
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
            let item = &self.items[index];
            let role = if index == self.navigation.cursor() {
                "control.focused"
            } else if item.entry.last_error.is_some() {
                "status.warning"
            } else {
                "dialog.background"
            };
            let prefix = item
                .shortcut
                .map_or_else(|| "   ".to_owned(), |slot| format!("[{slot}]"));
            let error = item
                .entry
                .last_error
                .as_deref()
                .map_or_else(String::new, |error| format!("  ! {error}"));
            let content = format!(
                "{} {prefix} {}  {}{error}",
                if item.entry.locked { "◆" } else { " " },
                item.entry.label,
                item.entry.location.as_str()
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
            "Enter Open   Space Lock   Delete Clear unlocked   Esc Close",
            "text",
        );
        scene
    }
}
