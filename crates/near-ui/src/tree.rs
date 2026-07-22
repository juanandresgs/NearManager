use near_core::{CapabilitySet, ContextId, ResourceRef, SurfaceId};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState, UpdateContext,
    UpdateResult,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub resource: Option<ResourceRef>,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

pub struct TreeSurface {
    id: SurfaceId,
    title: String,
    roots: Vec<TreeNode>,
    cursor: usize,
    indent_width: u8,
}

impl TreeSurface {
    pub fn new(id: impl Into<SurfaceId>, title: impl Into<String>, roots: Vec<TreeNode>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            roots,
            cursor: 0,
            indent_width: 2,
        }
    }

    #[must_use]
    pub fn with_indent_width(mut self, width: u8) -> Self {
        self.indent_width = width.clamp(1, 8);
        self
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn visible(&self) -> Vec<(usize, &TreeNode)> {
        fn collect<'a>(
            nodes: &'a [TreeNode],
            depth: usize,
            result: &mut Vec<(usize, &'a TreeNode)>,
        ) {
            for node in nodes {
                result.push((depth, node));
                if node.expanded {
                    collect(&node.children, depth + 1, result);
                }
            }
        }
        let mut result = Vec::new();
        collect(&self.roots, 0, &mut result);
        result
    }

    fn current_id(&self) -> Option<String> {
        self.visible()
            .get(self.cursor)
            .map(|(_, node)| node.id.clone())
    }

    fn node_mut<'a>(nodes: &'a mut [TreeNode], id: &str) -> Option<&'a mut TreeNode> {
        for node in nodes {
            if node.id == id {
                return Some(node);
            }
            if let Some(found) = Self::node_mut(&mut node.children, id) {
                return Some(found);
            }
        }
        None
    }
}

impl Surface for TreeSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.tree")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        let current = self
            .visible()
            .get(self.cursor)
            .and_then(|(_, node)| node.resource.clone());
        SurfaceState {
            location: current.as_ref().map(|resource| resource.location.clone()),
            current,
            selected: Vec::new(),
        }
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        let SurfaceEvent::Command(invocation) = event else {
            return UpdateResult::ignored();
        };
        match invocation.id.as_str() {
            "near.tree.up" => self.cursor = self.cursor.saturating_sub(1),
            "near.tree.down" => {
                self.cursor = self
                    .cursor
                    .saturating_add(1)
                    .min(self.visible().len().saturating_sub(1));
            }
            "near.tree.toggle" => {
                if let Some(id) = self.current_id()
                    && let Some(node) = Self::node_mut(&mut self.roots, &id)
                    && !node.children.is_empty()
                {
                    node.expanded = !node.expanded;
                }
                self.cursor = self.cursor.min(self.visible().len().saturating_sub(1));
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
        scene.fill(inner, "panel.background");
        for (index, (depth, node)) in self.visible().into_iter().enumerate() {
            let Ok(row) = u16::try_from(index) else { break };
            if row >= inner.height {
                break;
            }
            let branch = if node.children.is_empty() {
                " "
            } else if node.expanded {
                "▾"
            } else {
                "▸"
            };
            let label = format!(
                "{}{} {}",
                " ".repeat(depth.saturating_mul(usize::from(self.indent_width))),
                branch,
                node.label
            );
            let role = if context.focused && index == self.cursor {
                "panel.item.focused"
            } else {
                "panel.item"
            };
            scene.text(
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                label,
                role,
            );
        }
        scene
    }
}
