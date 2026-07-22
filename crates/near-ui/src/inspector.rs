use near_core::{CapabilitySet, ContextId, ResourceRef, SurfaceId};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState, UpdateContext,
    UpdateResult,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectorField {
    pub label: String,
    pub value: String,
    pub warning: bool,
}

pub struct InspectorSurface {
    id: SurfaceId,
    title: String,
    resource: Option<ResourceRef>,
    fields: Vec<InspectorField>,
    scroll: usize,
    visible_rows: Cell<usize>,
}

impl InspectorSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        resource: Option<ResourceRef>,
        fields: Vec<InspectorField>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            resource,
            fields,
            scroll: 0,
            visible_rows: Cell::new(1),
        }
    }
}

impl Surface for InspectorSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.inspector")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState {
            current: self.resource.clone(),
            selected: Vec::new(),
            location: self
                .resource
                .as_ref()
                .map(|resource| resource.location.clone()),
        }
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        let SurfaceEvent::Command(invocation) = event else {
            return UpdateResult::ignored();
        };
        match invocation.id.as_str() {
            "near.inspector.up" => self.scroll = self.scroll.saturating_sub(1),
            "near.inspector.down" => {
                self.scroll = self
                    .scroll
                    .saturating_add(1)
                    .min(self.fields.len().saturating_sub(1));
            }
            "near.inspector.home" => self.scroll = 0,
            "near.inspector.end" => {
                self.scroll = self
                    .fields
                    .len()
                    .saturating_sub(self.visible_rows.get().max(1));
            }
            "near.inspector.page-up" => {
                self.scroll = self.scroll.saturating_sub(self.visible_rows.get().max(1));
            }
            "near.inspector.page-down" => {
                self.scroll = self
                    .scroll
                    .saturating_add(self.visible_rows.get().max(1))
                    .min(self.fields.len().saturating_sub(1));
            }
            _ => return UpdateResult::ignored(),
        }
        UpdateResult::handled()
    }

    fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        let border = if context.focused {
            "panel.border.focused"
        } else {
            "panel.border"
        };
        scene.fill(area, border);
        scene.border(area, Some(format!(" {} ", self.title)), border);
        let inner = area.inset(1);
        self.visible_rows.set(usize::from(inner.height).max(1));
        scene.fill(inner, "panel.background");
        for (index, field) in self.fields.iter().skip(self.scroll).enumerate() {
            let Ok(row) = u16::try_from(index) else { break };
            if row >= inner.height {
                break;
            }
            let role = if field.warning {
                "status.warning"
            } else {
                "text"
            };
            scene.text(
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                format!("{:<16} {}", field.label, field.value),
                role,
            );
        }
        scene
    }
}
use std::cell::Cell;
