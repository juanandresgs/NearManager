#![allow(clippy::match_same_arms, clippy::missing_errors_doc)]

use std::collections::BTreeMap;

use near_core::{
    CapabilitySet, CommandId, CommandInvocation, CommandValue, ContextId, ResourceHistoryEntry,
    SurfaceId,
};

use crate::{
    ListNavigation, RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState,
    UpdateContext, UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct HistorySettings {
    pub schema: u32,
    #[serde(default = "default_command_limit")]
    pub command_max_unlocked: usize,
    #[serde(default = "default_folder_limit")]
    pub folder_max_unlocked: usize,
    #[serde(default = "default_resource_limit")]
    pub resource_max_unlocked: usize,
}

const fn default_command_limit() -> usize {
    200
}

const fn default_folder_limit() -> usize {
    200
}

const fn default_resource_limit() -> usize {
    100
}

impl HistorySettings {
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let settings: Self = toml::from_str(source).map_err(|error| error.to_string())?;
        if settings.schema != 1 {
            return Err(format!(
                "unsupported history settings schema {}",
                settings.schema
            ));
        }
        if settings.command_max_unlocked == 0
            || settings.folder_max_unlocked == 0
            || settings.resource_max_unlocked == 0
        {
            return Err("history retention limits must be greater than zero".to_owned());
        }
        Ok(settings)
    }
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            schema: 1,
            command_max_unlocked: default_command_limit(),
            folder_max_unlocked: default_folder_limit(),
            resource_max_unlocked: default_resource_limit(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceHistoryKind {
    Viewed,
    Edited,
}

impl ResourceHistoryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewed => "viewed",
            Self::Edited => "edited",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Viewed => "View History",
            Self::Edited => "Edit History",
        }
    }
}

pub struct ResourceHistorySurface {
    id: SurfaceId,
    kind: ResourceHistoryKind,
    entries: Vec<ResourceHistoryEntry>,
    navigation: ListNavigation,
    filter: SelectionSearch,
}

impl ResourceHistorySurface {
    pub fn new(kind: ResourceHistoryKind, entries: &[ResourceHistoryEntry]) -> Self {
        Self {
            id: SurfaceId::from(format!("near-fm.{}-history", kind.as_str())),
            kind,
            entries: entries.iter().rev().cloned().collect(),
            navigation: ListNavigation::default(),
            filter: SelectionSearch::default(),
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                self.filter.matches([
                    entry.label.as_str(),
                    entry.resource.location.as_str(),
                    entry.resource.provider.as_str(),
                ])
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn move_by(&mut self, rows: isize) {
        let visible = self.visible_indices();
        self.navigation.move_by(&visible, rows);
    }

    fn selected_command(&self, id: &str) -> UpdateResult {
        self.entries
            .get(self.navigation.cursor())
            .map_or_else(UpdateResult::ignored, |entry| {
                UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from(id),
                    arguments: BTreeMap::from([
                        (
                            "kind".to_owned(),
                            CommandValue::String(self.kind.as_str().to_owned()),
                        ),
                        (
                            "provider".to_owned(),
                            CommandValue::String(entry.resource.provider.to_string()),
                        ),
                        (
                            "location".to_owned(),
                            CommandValue::String(entry.resource.location.as_str().to_owned()),
                        ),
                        (
                            "label".to_owned(),
                            CommandValue::String(entry.label.clone()),
                        ),
                    ]),
                })
            })
    }

    fn clear_command(&self) -> UpdateResult {
        UpdateResult::dispatch(CommandInvocation {
            id: CommandId::from("near.resource-history.clear"),
            arguments: BTreeMap::from([(
                "kind".to_owned(),
                CommandValue::String(self.kind.as_str().to_owned()),
            )]),
        })
    }
}

impl Surface for ResourceHistorySurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.resource-history")]
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
                "near.resource-history.up" => {
                    self.move_by(-1);
                    UpdateResult::handled()
                }
                "near.resource-history.down" => {
                    self.move_by(1);
                    UpdateResult::handled()
                }
                "near.resource-history.first" => {
                    let visible = self.visible_indices();
                    self.navigation.first(&visible);
                    UpdateResult::handled()
                }
                "near.resource-history.last" => {
                    let visible = self.visible_indices();
                    self.navigation.last(&visible);
                    UpdateResult::handled()
                }
                "near.resource-history.page-up" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, -1);
                    UpdateResult::handled()
                }
                "near.resource-history.page-down" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, 1);
                    UpdateResult::handled()
                }
                "near.resource-history.open" => {
                    self.selected_command("near.resource-history.open-selected")
                }
                "near.resource-history.toggle-lock" => {
                    self.selected_command("near.resource-history.toggle-lock-selected")
                }
                "near.resource-history.clear-unlocked" => self.clear_command(),
                _ => UpdateResult::ignored(),
            },
            _ => UpdateResult::ignored(),
        }
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "dialog.background");
        scene.border(
            area,
            Some(format!(" {} ", self.kind.title())),
            "dialog.border",
        );
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
            } else if entry.last_error.is_some() {
                "status.warning"
            } else {
                "dialog.background"
            };
            let error = entry
                .last_error
                .as_deref()
                .map_or_else(String::new, |error| format!("  ! {error}"));
            let content = format!(
                "{} {:>4} {}  {}{error}",
                if entry.locked { "◆" } else { " " },
                entry.use_count,
                entry.label,
                entry.resource.location.as_str()
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

#[cfg(test)]
mod tests {
    use super::HistorySettings;

    #[test]
    fn history_settings_validate_independent_retention_limits() {
        let settings = HistorySettings::from_toml(
            "schema = 1\ncommand_max_unlocked = 7\nfolder_max_unlocked = 8\nresource_max_unlocked = 9\n",
        )
        .unwrap();
        assert_eq!(settings.command_max_unlocked, 7);
        assert_eq!(settings.folder_max_unlocked, 8);
        assert_eq!(settings.resource_max_unlocked, 9);
        assert!(HistorySettings::from_toml("schema = 1\ncommand_max_unlocked = 0\n").is_err());
        assert!(HistorySettings::from_toml("schema = 2\n").is_err());
    }
}
