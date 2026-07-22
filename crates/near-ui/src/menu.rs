#![allow(clippy::match_same_arms)]

use near_core::{CapabilitySet, CommandInvocation, ContextId, SurfaceId};

use crate::{
    ListNavigation, RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState,
    UpdateContext, UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub description: String,
    pub command: CommandInvocation,
    pub enabled: bool,
}

pub struct MenuSurface {
    id: SurfaceId,
    title: String,
    items: Vec<MenuItem>,
    navigation: ListNavigation,
    filter: SelectionSearch,
    wrap_navigation: bool,
    main_menu: Option<MainMenuPresentation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MainMenuPresentation {
    labels: Vec<String>,
    selected: usize,
}

impl MenuSurface {
    pub fn new(id: impl Into<SurfaceId>, title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            items,
            navigation: ListNavigation::default(),
            filter: SelectionSearch::default(),
            wrap_navigation: false,
            main_menu: None,
        }
    }

    #[must_use]
    pub fn with_main_menu(mut self, labels: Vec<String>, selected: usize) -> Self {
        self.main_menu = Some(MainMenuPresentation { labels, selected });
        self
    }

    pub fn main_menu_index(&self) -> Option<usize> {
        self.main_menu.as_ref().map(|menu| menu.selected)
    }

    pub fn is_main_menu(&self) -> bool {
        self.main_menu.is_some()
    }

    pub fn main_menu_column(&self, origin: u16) -> Option<u16> {
        let menu = self.main_menu.as_ref()?;
        Some(
            menu.labels
                .iter()
                .take(menu.selected)
                .fold(origin.saturating_add(1), |column, label| {
                    column.saturating_add(main_menu_label_width(label))
                }),
        )
    }

    pub fn main_menu_index_at(&self, origin: u16, column: u16) -> Option<usize> {
        let menu = self.main_menu.as_ref()?;
        let mut start = origin.saturating_add(1);
        for (index, label) in menu.labels.iter().enumerate() {
            let end = start.saturating_add(main_menu_label_width(label));
            if (start..end).contains(&column) {
                return Some(index);
            }
            start = end;
        }
        None
    }

    pub fn main_menu_scene(&self, area: SceneRect) -> Option<Scene> {
        let menu = self.main_menu.as_ref()?;
        let mut scene = Scene::new();
        scene.fill(area, "dialog.background");
        let mut column = area.x.saturating_add(1);
        for (index, label) in menu.labels.iter().enumerate() {
            let label = format!(" {label} ");
            let available = area.right().saturating_sub(column);
            if available == 0 {
                break;
            }
            let width = main_menu_label_width(label.trim()).min(available);
            scene.text(
                SceneRect::new(column, area.y, width, 1),
                label,
                if index == menu.selected {
                    "control.focused"
                } else {
                    "dialog.background"
                },
            );
            column = column.saturating_add(width);
        }
        Some(scene)
    }

    pub fn main_menu_popup_size(
        &self,
        maximum_width: u16,
        maximum_height: u16,
    ) -> Option<(u16, u16)> {
        self.main_menu.as_ref()?;
        let content_width = self
            .items
            .iter()
            .map(|item| {
                display_label(&item.label)
                    .chars()
                    .count()
                    .saturating_add(1)
                    .saturating_add(item.description.chars().count())
            })
            .chain(std::iter::once(
                self.title.chars().count().saturating_add(2),
            ))
            .max()
            .unwrap_or(1)
            .saturating_add(2);
        let width = u16::try_from(content_width)
            .unwrap_or(u16::MAX)
            .min(60)
            .min(maximum_width)
            .max(1);
        let content_height = self
            .visible_indices()
            .len()
            .saturating_add(2)
            .saturating_add(usize::from(self.filter.is_active()));
        let height = u16::try_from(content_height)
            .unwrap_or(u16::MAX)
            .min(maximum_height)
            .max(1);
        Some((width, height))
    }

    pub fn selected(&self) -> usize {
        self.navigation.cursor()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    pub fn select_visible_row(&mut self, row: usize) -> bool {
        let visible = self.visible_indices();
        self.navigation.select_visible_row(&visible, row)
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                self.filter
                    .matches([plain_label(&item.label), item.description.clone()])
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn move_by(&mut self, rows: isize) {
        let visible = self.visible_indices();
        let position = visible
            .iter()
            .position(|index| *index == self.navigation.cursor())
            .unwrap_or_default();
        if self.wrap_navigation && rows < 0 && position == 0 {
            self.navigation.last(&visible);
        } else if self.wrap_navigation && rows > 0 && position + 1 == visible.len() {
            self.navigation.first(&visible);
        } else {
            self.navigation.move_by(&visible, rows);
        }
    }
}

impl Surface for MenuSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }
    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.menu")]
    }
    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }
    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }
    fn configure_interaction(&mut self, menu_wrap: bool, _dialog_wrap: bool) {
        self.wrap_navigation = menu_wrap;
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        match event {
            SurfaceEvent::Text(text) => {
                if let Some(character) = single_character(text)
                    && let Some((index, item)) = self.items.iter().enumerate().find(|(_, item)| {
                        accelerator(&item.label)
                            .is_some_and(|accelerator| accelerator.eq_ignore_ascii_case(&character))
                    })
                {
                    self.navigation.set_cursor(index);
                    return UpdateResult::dispatch(item.command.clone());
                }
                self.filter.push(text);
                self.move_by(0);
                UpdateResult::handled()
            }
            SurfaceEvent::SelectionSearchText(text) => {
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
                "near.menu.up" => {
                    self.move_by(-1);
                    UpdateResult::handled()
                }
                "near.menu.down" => {
                    self.move_by(1);
                    UpdateResult::handled()
                }
                "near.menu.first" => {
                    let visible = self.visible_indices();
                    self.navigation.first(&visible);
                    UpdateResult::handled()
                }
                "near.menu.last" => {
                    let visible = self.visible_indices();
                    self.navigation.last(&visible);
                    UpdateResult::handled()
                }
                "near.menu.page-up" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, -1);
                    UpdateResult::handled()
                }
                "near.menu.page-down" => {
                    let visible = self.visible_indices();
                    self.navigation.page(&visible, 1);
                    UpdateResult::handled()
                }
                "near.menu.activate" => self
                    .items
                    .get(self.navigation.cursor())
                    .map_or_else(UpdateResult::ignored, |item| {
                        UpdateResult::dispatch(item.command.clone())
                    }),
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
        let rows = usize::from(
            inner
                .height
                .saturating_sub(u16::from(self.filter.is_active())),
        );
        for (row, index) in self
            .navigation
            .window(&visible, rows)
            .iter()
            .copied()
            .enumerate()
        {
            let Ok(row) = u16::try_from(row) else { break };
            if row
                >= inner
                    .height
                    .saturating_sub(u16::from(self.filter.is_active()))
            {
                break;
            }
            let item = &self.items[index];
            let role = if !item.enabled {
                "control.disabled"
            } else if index == self.navigation.cursor() {
                "control.focused"
            } else {
                "dialog.background"
            };
            let content = format!("{:<24} {}", display_label(&item.label), item.description);
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
                SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
                self.filter.prompt(),
                "text",
            );
        }
        scene
    }
}

fn accelerator(label: &str) -> Option<char> {
    let mut characters = label.chars();
    while let Some(character) = characters.next() {
        if character == '&' {
            return characters.next();
        }
    }
    None
}

fn main_menu_label_width(label: &str) -> u16 {
    u16::try_from(label.chars().count().saturating_add(2)).unwrap_or(u16::MAX)
}

fn plain_label(label: &str) -> String {
    label
        .chars()
        .filter(|character| *character != '&')
        .collect()
}

fn display_label(label: &str) -> String {
    let mut display = String::new();
    let mut characters = label.chars();
    while let Some(character) = characters.next() {
        if character == '&' {
            if let Some(accelerator) = characters.next() {
                display.push('[');
                display.push(accelerator.to_ascii_uppercase());
                display.push(']');
            }
        } else {
            display.push(character);
        }
    }
    display
}

fn single_character(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_core::{ActionContext, CommandId, CommandInvocation};

    use crate::ScenePrimitive;

    use super::*;

    #[test]
    fn menu_catalog_is_publicly_inspectable() {
        let menu = MenuSurface::new(
            "menu",
            "Inspectable Menu",
            vec![MenuItem {
                label: "&Open".to_owned(),
                description: "Open the current item".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("item.open"),
                    arguments: BTreeMap::new(),
                },
                enabled: true,
            }],
        );
        assert_eq!(menu.title(), "Inspectable Menu");
        assert_eq!(menu.items().len(), 1);
        assert_eq!(menu.items()[0].command.id.as_str(), "item.open");
    }

    #[test]
    fn accelerators_dispatch_enabled_and_disabled_items_for_explicit_resolution() {
        let mut menu = MenuSurface::new(
            "menu",
            "Menu",
            vec![
                MenuItem {
                    label: "&Files".to_owned(),
                    description: String::new(),
                    command: CommandInvocation {
                        id: CommandId::from("files"),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                },
                MenuItem {
                    label: "&Options".to_owned(),
                    description: String::new(),
                    command: CommandInvocation {
                        id: CommandId::from("options"),
                        arguments: BTreeMap::new(),
                    },
                    enabled: false,
                },
            ],
        );
        let action = ActionContext::default();
        let result = menu.update(
            &SurfaceEvent::Text("f".to_owned()),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(result.command.unwrap().id, CommandId::from("files"));
        let result = menu.update(
            &SurfaceEvent::Text("o".to_owned()),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(result.command.unwrap().id, CommandId::from("options"));
        assert_eq!(menu.selected(), 1);
    }

    #[test]
    fn filter_is_hidden_until_alt_typing_and_matches_are_highlighted() {
        let mut menu = MenuSurface::new(
            "menu",
            "Menu",
            vec![MenuItem {
                label: "&Attributes".to_owned(),
                description: "Permissions and timestamps".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("attributes"),
                    arguments: BTreeMap::new(),
                },
                enabled: true,
            }],
        );
        let action = ActionContext::default();
        let scene = menu.scene(
            SceneRect::new(0, 0, 60, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(!scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { content, .. } if content.starts_with("Find:")
        )));

        menu.update(
            &SurfaceEvent::SelectionSearchText("tri".to_owned()),
            &mut UpdateContext { action: &action },
        );
        let scene = menu.scene(
            SceneRect::new(0, 0, 60, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { content, .. } if content == "Find: tri_  Alt+Backspace edit"
        )));
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { content, role, .. }
                if content.eq_ignore_ascii_case("tri") && role.as_str() == "selection.match"
        )));
    }

    #[test]
    fn surface_interaction_conformance_menu_pages_and_keeps_focus_visible() {
        let mut menu = MenuSurface::new(
            "menu",
            "Menu",
            (0..12)
                .map(|index| MenuItem {
                    label: format!("Item {index}"),
                    description: format!("Description {index}"),
                    command: CommandInvocation {
                        id: CommandId::from(format!("item.{index}")),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                })
                .collect(),
        );
        let action = ActionContext::default();
        menu.scene(
            SceneRect::new(0, 0, 40, 6),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        for command in ["near.menu.page-down", "near.menu.last"] {
            menu.update(
                &SurfaceEvent::Command(CommandInvocation {
                    id: CommandId::from(command),
                    arguments: BTreeMap::new(),
                }),
                &mut UpdateContext { action: &action },
            );
        }
        assert_eq!(menu.selected(), 11);
        let scene = menu.scene(
            SceneRect::new(0, 0, 40, 6),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| {
            matches!(primitive, ScenePrimitive::Text { content, role, .. } if content.contains("Item 11") && role.as_str() == "control.focused")
        }));
        menu.update(
            &SurfaceEvent::Command(CommandInvocation {
                id: CommandId::from("near.menu.first"),
                arguments: BTreeMap::new(),
            }),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(menu.selected(), 0);
    }
}
