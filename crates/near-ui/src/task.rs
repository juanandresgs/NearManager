#![allow(clippy::similar_names)]

use near_core::{CapabilitySet, CommandId, CommandInvocation, ContextId, SurfaceId};
pub use near_runtime::{TaskRecord, TaskState};

use crate::{
    ListNavigation, RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState,
    UpdateContext, UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

pub struct TaskSurface {
    id: SurfaceId,
    tasks: Vec<TaskRecord>,
    navigation: ListNavigation,
    search: SelectionSearch,
}

impl TaskSurface {
    pub fn new(id: impl Into<SurfaceId>, tasks: Vec<TaskRecord>) -> Self {
        Self {
            id: id.into(),
            tasks,
            navigation: ListNavigation::default(),
            search: SelectionSearch::default(),
        }
    }

    pub fn tasks(&self) -> &[TaskRecord] {
        &self.tasks
    }

    pub fn cursor(&self) -> usize {
        self.navigation.cursor()
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| {
                self.search.matches([
                    task.id.as_str(),
                    task.title.as_str(),
                    task.message.as_str(),
                    format!("{:?}", task.state).as_str(),
                ])
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn move_by(&mut self, rows: isize) {
        let visible = self.visible_indices();
        self.navigation.move_by(&visible, rows);
    }
}

impl Surface for TaskSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.tasks")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        let SurfaceEvent::Command(invocation) = event else {
            return match event {
                SurfaceEvent::SelectionSearchText(text) => {
                    self.search.push(text);
                    self.move_by(0);
                    UpdateResult::handled()
                }
                SurfaceEvent::SelectionSearchBackspace => {
                    self.search.pop();
                    self.move_by(0);
                    UpdateResult::handled()
                }
                _ => UpdateResult::ignored(),
            };
        };
        match invocation.id.as_str() {
            "near.tasks.up" => self.move_by(-1),
            "near.tasks.down" => self.move_by(1),
            "near.tasks.first" => {
                let visible = self.visible_indices();
                self.navigation.first(&visible);
            }
            "near.tasks.last" => {
                let visible = self.visible_indices();
                self.navigation.last(&visible);
            }
            "near.tasks.page-up" => {
                let visible = self.visible_indices();
                self.navigation.page(&visible, -1);
            }
            "near.tasks.page-down" => {
                let visible = self.visible_indices();
                self.navigation.page(&visible, 1);
            }
            "near.tasks.cancel" => {
                let Some(task) = self.tasks.get(self.navigation.cursor()) else {
                    return UpdateResult::ignored();
                };
                return UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from("near.task.cancel"),
                    arguments: [(
                        "task".to_owned(),
                        near_core::CommandValue::String(task.id.clone()),
                    )]
                    .into_iter()
                    .collect(),
                });
            }
            "near.tasks.retry" => {
                let Some(task) = self.tasks.get(self.navigation.cursor()) else {
                    return UpdateResult::ignored();
                };
                return UpdateResult::dispatch(CommandInvocation {
                    id: CommandId::from("near.task.retry"),
                    arguments: [(
                        "task".to_owned(),
                        near_core::CommandValue::String(task.id.clone()),
                    )]
                    .into_iter()
                    .collect(),
                });
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
        scene.border(area, Some(" Tasks ".to_owned()), border);
        let inner = area.inset(1);
        scene.fill(inner, "panel.background");
        let visible = self.visible_indices();
        let rows = usize::from(
            inner
                .height
                .saturating_sub(u16::from(self.search.is_active())),
        );
        for (row, index) in self
            .navigation
            .window(&visible, rows)
            .iter()
            .copied()
            .enumerate()
        {
            let task = &self.tasks[index];
            let Ok(row) = u16::try_from(row) else { break };
            if row
                >= inner
                    .height
                    .saturating_sub(u16::from(self.search.is_active()))
            {
                break;
            }
            let role = if context.focused && index == self.navigation.cursor() {
                "panel.item.focused"
            } else if task.state == TaskState::Failed {
                "status.warning"
            } else {
                "panel.item"
            };
            let progress = task.total.map_or_else(
                || task.completed.to_string(),
                |total| format!("{}/{}", task.completed, total),
            );
            let content = format!(
                "{:<10?} {:<18} {:>10} {}",
                task.state, task.title, progress, task.message
            );
            searchable_text(
                &mut scene,
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                &content,
                role,
                &self.search,
            );
        }
        if self.search.is_active() {
            scene.text(
                SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
                self.search.prompt(),
                "text",
            );
        }
        scene
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_core::{ActionContext, CommandId, CommandInvocation};

    use super::*;

    #[test]
    fn surface_interaction_conformance_tasks_page_and_keep_focus_visible() {
        let mut surface = TaskSurface::new(
            "tasks",
            (0..12)
                .map(|index| TaskRecord {
                    id: index.to_string(),
                    title: format!("Task {index:02}"),
                    state: TaskState::Running,
                    completed: 0,
                    total: Some(1),
                    message: "running".to_owned(),
                })
                .collect(),
        );
        let action = ActionContext::default();
        surface.scene(
            SceneRect::new(0, 0, 60, 6),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        for command in ["near.tasks.page-down", "near.tasks.last"] {
            surface.update(
                &SurfaceEvent::Command(CommandInvocation {
                    id: CommandId::from(command),
                    arguments: BTreeMap::new(),
                }),
                &mut UpdateContext { action: &action },
            );
        }
        assert_eq!(surface.cursor(), 11);
        let scene = surface.scene(
            SceneRect::new(0, 0, 60, 6),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| {
            matches!(primitive, crate::ScenePrimitive::Text { content, role, .. } if content.contains("Task 11") && role.as_str() == "panel.item.focused")
        }));
    }
}
