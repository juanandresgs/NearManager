use std::collections::BTreeMap;

use near_config::{
    SettingApplyScope, SettingCandidate, SettingDescriptor, SettingProvenance, SettingState,
    SettingValue,
};
use near_core::{CapabilitySet, CommandValue, ContextId, SurfaceId};

use crate::{
    ListNavigation, RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfacePresentation,
    SurfaceState, UpdateContext, UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingSurfaceEntry {
    pub descriptor: SettingDescriptor,
    pub state: SettingState,
}
#[allow(clippy::missing_errors_doc)]
pub trait SettingsDocumentStore: Send + Sync {
    fn load(&self, document: &str) -> Result<Option<String>, String>;
    fn provenance(&self, _document: &str) -> Option<SettingProvenance> {
        None
    }
    /// Atomically persists one complete versioned settings document.
    ///
    /// # Errors
    ///
    /// Returns a storage failure without changing the previously persisted document.
    fn persist(&self, document: &str, contents: &str) -> Result<(), String>;
}
pub struct SettingsSurface {
    id: SurfaceId,
    title: String,
    entries: Vec<SettingSurfaceEntry>,
    navigation: ListNavigation,
    filter: SelectionSearch,
    candidates: BTreeMap<String, SettingValue>,
    message: String,
    show_advanced: bool,
}

impl SettingsSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        mut entries: Vec<SettingSurfaceEntry>,
    ) -> Self {
        entries.sort_by(|left, right| {
            left.descriptor
                .category
                .cmp(&right.descriptor.category)
                .then_with(|| left.descriptor.title.cmp(&right.descriptor.title))
        });
        let mut surface = Self {
            id: id.into(),
            title: title.into(),
            entries,
            navigation: ListNavigation::default(),
            filter: SelectionSearch::default(),
            candidates: BTreeMap::new(),
            message: String::new(),
            show_advanced: false,
        };
        surface.move_by(0);
        surface
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.entries
            .get(self.navigation.cursor())
            .map(|entry| entry.descriptor.id.as_str())
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Stages a typed value for later coordinator validation and application.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is unknown or the value kind does not match.
    pub fn set_value(&mut self, id: &str, value: SettingValue) -> Result<(), String> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.descriptor.id == id)
            .ok_or_else(|| format!("unknown setting: {id}"))?;
        if value.kind() != entry.descriptor.value_kind {
            return Err(format!("setting {id} has the wrong value type"));
        }
        self.candidates.insert(id.to_owned(), value);
        scope_message(entry.descriptor.apply_scope).clone_into(&mut self.message);
        Ok(())
    }

    pub fn reset_selected(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.navigation.cursor()) else {
            return false;
        };
        self.candidates.insert(
            entry.descriptor.id.clone(),
            entry.descriptor.default_value.clone(),
        );
        "Reset to the declared default".clone_into(&mut self.message);
        true
    }

    pub fn candidates(&self) -> Vec<SettingCandidate> {
        self.candidates
            .iter()
            .map(|(id, value)| SettingCandidate {
                id: id.clone(),
                value: value.clone(),
            })
            .collect()
    }

    fn value<'a>(&'a self, entry: &'a SettingSurfaceEntry) -> &'a SettingValue {
        self.candidates
            .get(&entry.descriptor.id)
            .unwrap_or(&entry.state.value)
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (!entry.descriptor.advanced || self.show_advanced || self.filter.is_active())
                    && self.filter.matches([
                        entry.descriptor.title.clone(),
                        entry.descriptor.description.clone(),
                        entry.descriptor.category.clone(),
                        entry.descriptor.id.clone(),
                        entry.state.provenance.source.clone(),
                    ])
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn move_by(&mut self, rows: isize) {
        let visible = self.visible_indices();
        self.navigation.move_by(&visible, rows);
    }

    fn toggle_selected(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.navigation.cursor()) else {
            return false;
        };
        let SettingValue::Boolean(value) = self.value(entry) else {
            return false;
        };
        let id = entry.descriptor.id.clone();
        let scope = entry.descriptor.apply_scope;
        self.candidates.insert(id, SettingValue::Boolean(!value));
        scope_message(scope).clone_into(&mut self.message);
        true
    }

    fn selected_candidate_command(&self) -> Option<near_core::CommandInvocation> {
        let entry = self.entries.get(self.navigation.cursor())?;
        let value = self.candidates.get(&entry.descriptor.id)?;
        Some(near_core::CommandInvocation {
            id: "near.settings.apply-candidate".into(),
            arguments: BTreeMap::from([
                (
                    "id".to_owned(),
                    CommandValue::String(entry.descriptor.id.clone()),
                ),
                (
                    "value".to_owned(),
                    CommandValue::String(format_value(value)),
                ),
            ]),
        })
    }

    fn selected_edit_command(&self) -> Option<near_core::CommandInvocation> {
        let id = self.selected_id()?;
        let value = self
            .entries
            .get(self.navigation.cursor())
            .map(|entry| format_value(self.value(entry)))
            .unwrap_or_default();
        Some(near_core::CommandInvocation {
            id: "near.settings.edit-value".into(),
            arguments: BTreeMap::from([
                ("id".to_owned(), CommandValue::String(id.to_owned())),
                ("value".to_owned(), CommandValue::String(value)),
            ]),
        })
    }
}

impl Surface for SettingsSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![
            ContextId::from("surface.settings"),
            ContextId::from("overlay.menu"),
        ]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn presentation(&self) -> SurfacePresentation {
        SurfacePresentation::FullScreen
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
            SurfaceEvent::Backspace | SurfaceEvent::SelectionSearchBackspace => {
                self.filter.pop();
                self.move_by(0);
                UpdateResult::handled()
            }
            SurfaceEvent::Command(invocation) => match invocation.id.as_str() {
                "near.settings.up" | "near.menu.up" => {
                    self.move_by(-1);
                    UpdateResult::handled()
                }
                "near.settings.down" | "near.menu.down" => {
                    self.move_by(1);
                    UpdateResult::handled()
                }
                "near.settings.first" | "near.menu.first" => {
                    let visible = self.visible_indices();
                    self.navigation.first(&visible);
                    UpdateResult::handled()
                }
                "near.settings.last" | "near.menu.last" => {
                    let visible = self.visible_indices();
                    self.navigation.last(&visible);
                    UpdateResult::handled()
                }
                "near.settings.page-up" | "near.menu.page-up" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, -1);
                    UpdateResult::handled()
                }
                "near.settings.page-down" | "near.menu.page-down" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, 1);
                    UpdateResult::handled()
                }
                "near.settings.toggle" | "near.menu.activate" => {
                    if self.toggle_selected() {
                        self.selected_candidate_command()
                            .map_or_else(UpdateResult::handled, UpdateResult::dispatch)
                    } else {
                        self.selected_edit_command()
                            .map_or_else(UpdateResult::ignored, UpdateResult::dispatch)
                    }
                }
                "near.settings.reset" => {
                    if self.reset_selected() {
                        self.selected_candidate_command()
                            .map_or_else(UpdateResult::handled, UpdateResult::dispatch)
                    } else {
                        UpdateResult::ignored()
                    }
                }
                "near.settings.edit" => self
                    .selected_edit_command()
                    .map_or_else(UpdateResult::ignored, UpdateResult::dispatch),
                "near.settings.toggle-advanced" => {
                    self.show_advanced = !self.show_advanced;
                    self.move_by(0);
                    if self.show_advanced {
                        "Advanced settings are visible"
                    } else {
                        "Advanced settings are hidden; search still includes them"
                    }
                    .clone_into(&mut self.message);
                    UpdateResult::handled()
                }
                _ => UpdateResult::ignored(),
            },
            _ => UpdateResult::ignored(),
        }
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "dialog.background");
        scene.border(area, Some(format!(" {} ", self.title)), "dialog.border");
        let inner = area.inset(1);
        let visible = self.visible_indices();
        let rows = usize::from(inner.height.saturating_sub(2));
        for (row, index) in self
            .navigation
            .window(&visible, rows)
            .iter()
            .copied()
            .enumerate()
        {
            let Ok(row) = u16::try_from(row) else { break };
            if row >= inner.height.saturating_sub(2) {
                break;
            }
            let entry = &self.entries[index];
            let marker = if self.candidates.contains_key(&entry.descriptor.id) {
                "*"
            } else {
                " "
            };
            let content = format!(
                "{marker} ({:?}: {}) {:<22} {:<14} {}",
                entry.state.provenance.layer,
                entry.state.provenance.source,
                entry.descriptor.title,
                format_value(self.value(entry)),
                entry.descriptor.description
            );
            searchable_text(
                &mut scene,
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                &content,
                if index == self.navigation.cursor() {
                    "control.focused"
                } else {
                    "dialog.background"
                },
                &self.filter,
            );
        }
        if let Some(entry) = self.entries.get(self.navigation.cursor()) {
            let metadata = format!(
                "{}{} • type={:?} platform={:?} scope={:?} source={:?}:{}",
                if entry.descriptor.advanced {
                    "advanced "
                } else {
                    ""
                },
                entry.descriptor.description,
                entry.descriptor.value_kind,
                entry.descriptor.availability,
                entry.descriptor.apply_scope,
                entry.state.provenance.layer,
                entry.state.provenance.source
            );
            scene.text(
                SceneRect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
                metadata,
                "text.dim",
            );
        }
        let controls = format!(
            "Enter toggle/edit • F4 edit • F5 reload • F6 {} advanced • Del reset • Esc back",
            if self.show_advanced { "hide" } else { "show" }
        );
        let footer = if self.filter.is_active() {
            format!("{} • Esc back", self.filter.prompt())
        } else if self.message.is_empty() {
            controls
        } else {
            format!("{} • {controls}", self.message)
        };
        scene.text(
            SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            footer,
            "text.dim",
        );
        scene
    }
}

fn format_value(value: &SettingValue) -> String {
    match value {
        SettingValue::Boolean(value) => value.to_string(),
        SettingValue::Integer(value) => value.to_string(),
        SettingValue::String(value) => value.clone(),
        SettingValue::Strings(values) => values.join(", "),
    }
}

const fn scope_message(scope: SettingApplyScope) -> &'static str {
    match scope {
        SettingApplyScope::Live => "Applies live after validation",
        SettingApplyScope::NewSurface => "Applies to newly opened surfaces",
        SettingApplyScope::Restart => "Requires restart after persistence",
    }
}

#[cfg(test)]
mod tests {
    use near_config::{
        ConfigLayerKind, SettingApplyScope, SettingDescriptor, SettingPlatformAvailability,
        SettingProvenance, SettingState, SettingValue,
    };
    use near_core::{ActionContext, CommandId, CommandInvocation};

    use super::*;

    fn entry(id: &str, title: &str, value: SettingValue) -> SettingSurfaceEntry {
        SettingSurfaceEntry {
            descriptor: SettingDescriptor {
                id: id.to_owned(),
                document: "ui.toml".to_owned(),
                path: id.to_owned(),
                category: "Interface".to_owned(),
                title: title.to_owned(),
                description: "Configures the interface".to_owned(),
                advanced: false,
                value_kind: value.kind(),
                default_value: value.clone(),
                apply_scope: SettingApplyScope::Live,
                apply_order: 0,
                availability: SettingPlatformAvailability::All,
            },
            state: SettingState {
                value,
                provenance: SettingProvenance {
                    layer: ConfigLayerKind::User,
                    source: "user ui.toml".to_owned(),
                },
            },
        }
    }

    fn advanced_entry(id: &str, title: &str, value: SettingValue) -> SettingSurfaceEntry {
        let mut entry = entry(id, title, value);
        entry.descriptor.advanced = true;
        entry
    }

    #[test]
    fn search_includes_identifier_category_and_provenance() {
        let mut surface = SettingsSurface::new(
            "settings",
            "Settings",
            vec![
                entry("ui.clock", "Show clock", SettingValue::Boolean(true)),
                entry("ui.menu", "Menu bar", SettingValue::Boolean(true)),
            ],
        );
        surface.filter.push("ui.menu");
        assert_eq!(surface.visible_indices(), vec![0]);
        surface.filter = SelectionSearch::default();
        surface.filter.push("user ui.toml");
        assert_eq!(surface.visible_indices(), vec![0, 1]);
    }

    #[test]
    fn surface_interaction_conformance_settings_pages_and_keeps_focus_visible() {
        let mut surface = SettingsSurface::new(
            "settings",
            "Settings",
            (0..12)
                .map(|index| {
                    entry(
                        &format!("ui.setting.{index:02}"),
                        &format!("Setting {index:02}"),
                        SettingValue::Boolean(false),
                    )
                })
                .collect(),
        );
        let action = ActionContext::default();
        surface.scene(
            SceneRect::new(0, 0, 60, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        for command in ["near.settings.page-down", "near.settings.last"] {
            surface.update(
                &SurfaceEvent::Command(CommandInvocation {
                    id: CommandId::from(command),
                    arguments: BTreeMap::new(),
                }),
                &mut UpdateContext { action: &action },
            );
        }
        assert_eq!(surface.selected_id(), Some("ui.setting.11"));
        let scene = surface.scene(
            SceneRect::new(0, 0, 60, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| {
            matches!(primitive, crate::ScenePrimitive::Text { content, role, .. } if content.contains("Setting 11") && role.as_str() == "control.focused")
        }));
    }

    #[test]
    fn activating_non_boolean_setting_opens_typed_editor() {
        let mut surface = SettingsSurface::new(
            "settings",
            "Settings",
            vec![entry(
                "viewer.open_policy",
                "Open policy",
                SettingValue::String("internal".to_owned()),
            )],
        );
        let action = ActionContext::default();
        let result = surface.update(
            &SurfaceEvent::Command(CommandInvocation {
                id: CommandId::from("near.menu.activate"),
                arguments: BTreeMap::new(),
            }),
            &mut UpdateContext { action: &action },
        );
        let command = result.command.expect("edit command");
        assert_eq!(command.id.as_str(), "near.settings.edit-value");
        assert_eq!(
            command.arguments.get("id").and_then(CommandValue::as_str),
            Some("viewer.open_policy")
        );
        assert_eq!(
            command
                .arguments
                .get("value")
                .and_then(CommandValue::as_str),
            Some("internal")
        );
    }

    #[test]
    fn advanced_settings_are_hidden_by_default_but_searchable_and_explicitly_toggleable() {
        let mut surface = SettingsSurface::new(
            "settings",
            "Settings",
            vec![
                entry("ui.normal", "Normal option", SettingValue::Boolean(true)),
                advanced_entry(
                    "ui.physical_keys",
                    "Physical key identity",
                    SettingValue::Boolean(false),
                ),
            ],
        );
        let action = ActionContext::default();
        assert_eq!(surface.visible_indices(), vec![0]);

        surface.update(
            &SurfaceEvent::Text("physical".to_owned()),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(surface.visible_indices(), vec![1]);

        surface.filter = SelectionSearch::default();
        surface.update(
            &SurfaceEvent::Command(CommandInvocation {
                id: CommandId::from("near.settings.toggle-advanced"),
                arguments: BTreeMap::new(),
            }),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(surface.visible_indices(), vec![0, 1]);
        let scene = surface.scene(
            SceneRect::new(0, 0, 100, 10),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| {
            matches!(primitive, crate::ScenePrimitive::Text { content, .. } if content.contains("F6 hide advanced"))
        }));
    }

    #[test]
    fn hidden_advanced_entry_cannot_remain_the_selected_action_target() {
        let mut surface = SettingsSurface::new(
            "settings",
            "Settings",
            vec![
                advanced_entry(
                    "ui.advanced",
                    "A hidden advanced option",
                    SettingValue::Boolean(false),
                ),
                entry(
                    "ui.normal",
                    "Z visible normal option",
                    SettingValue::Boolean(true),
                ),
            ],
        );
        assert_eq!(surface.visible_indices(), vec![1]);
        assert_eq!(surface.selected_id(), Some("ui.normal"));
        assert!(surface.reset_selected());
        assert_eq!(surface.candidates()[0].id, "ui.normal");
    }
}
