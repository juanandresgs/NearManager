#![allow(clippy::obfuscated_if_else)]

use std::collections::BTreeMap;

use near_core::{CapabilitySet, CommandInvocation, CommandValue, ContextId, SurfaceId};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState, UpdateContext,
    UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogField {
    pub id: String,
    pub label: String,
    pub value: String,
    pub required: bool,
    pub secret: bool,
}

pub struct DialogSurface {
    id: SurfaceId,
    title: String,
    fields: Vec<DialogField>,
    focused: usize,
    accept: CommandInvocation,
    cancel: CommandInvocation,
    error: Option<String>,
    search: SelectionSearch,
    wrap_focus: bool,
}

impl DialogSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        fields: Vec<DialogField>,
        accept: CommandInvocation,
        cancel: CommandInvocation,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            fields,
            focused: 0,
            accept,
            cancel,
            error: None,
            search: SelectionSearch::default(),
            wrap_focus: true,
        }
    }

    pub fn values(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .map(|field| (field.id.clone(), field.value.clone()))
            .collect()
    }

    fn validate(&mut self) -> bool {
        if let Some(field) = self
            .fields
            .iter()
            .find(|field| field.required && field.value.trim().is_empty())
        {
            self.error = Some(format!("{} is required", field.label));
            return false;
        }
        self.error = None;
        true
    }

    fn select_search_match(&mut self) {
        if let Some(index) = self.fields.iter().position(|field| {
            self.search
                .matches([field.label.as_str(), field.id.as_str()])
        }) {
            self.focused = index;
        }
    }
}

impl Surface for DialogSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.dialog")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn configure_interaction(&mut self, _menu_wrap: bool, dialog_wrap: bool) {
        self.wrap_focus = dialog_wrap;
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        match event {
            SurfaceEvent::Text(text) | SurfaceEvent::Paste(text) => {
                if let Some(field) = self.fields.get_mut(self.focused) {
                    field.value.push_str(text);
                }
                UpdateResult::handled()
            }
            SurfaceEvent::Backspace => {
                if let Some(field) = self.fields.get_mut(self.focused) {
                    field.value.pop();
                }
                UpdateResult::handled()
            }
            SurfaceEvent::SelectionSearchText(text) => {
                self.search.push(text);
                self.select_search_match();
                UpdateResult::handled()
            }
            SurfaceEvent::SelectionSearchBackspace => {
                self.search.pop();
                self.select_search_match();
                UpdateResult::handled()
            }
            SurfaceEvent::Command(invocation) => match invocation.id.as_str() {
                "near.dialog.next" => {
                    if !self.fields.is_empty() {
                        self.focused = if self.wrap_focus {
                            (self.focused + 1) % self.fields.len()
                        } else {
                            (self.focused + 1).min(self.fields.len() - 1)
                        };
                    }
                    UpdateResult::handled()
                }
                "near.dialog.previous" => {
                    if !self.fields.is_empty() {
                        self.focused = self.focused.checked_sub(1).unwrap_or_else(|| {
                            if self.wrap_focus {
                                self.fields.len() - 1
                            } else {
                                0
                            }
                        });
                    }
                    UpdateResult::handled()
                }
                "near.dialog.accept" => {
                    if !self.validate() {
                        return UpdateResult::handled();
                    }
                    let mut command = self.accept.clone();
                    command.arguments.extend(
                        self.values()
                            .into_iter()
                            .map(|(key, value)| (key, CommandValue::String(value))),
                    );
                    UpdateResult::dispatch(command)
                }
                "near.dialog.cancel" => UpdateResult::dispatch(self.cancel.clone()),
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
        for (index, field) in self.fields.iter().enumerate() {
            let Ok(row) = u16::try_from(index) else { break };
            if row >= inner.height.saturating_sub(2) {
                break;
            }
            let display = if field.secret {
                "•".repeat(field.value.chars().count())
            } else {
                field.value.clone()
            };
            let role = if index == self.focused {
                "control.focused"
            } else {
                "dialog.background"
            };
            let content = format!("{:<14} {display}", field.label);
            searchable_text(
                &mut scene,
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                &content,
                role,
                &self.search,
            );
        }
        if let Some(error) = &self.error {
            scene.text(
                SceneRect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
                error,
                "status.warning",
            );
        }
        let search_prompt = self
            .search
            .is_active()
            .then(|| format!("   {}", self.search.prompt()))
            .unwrap_or_default();
        scene.text(
            SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            format!("Enter Accept   Esc Cancel   Tab Next{search_prompt}"),
            "text",
        );
        scene
    }
}
