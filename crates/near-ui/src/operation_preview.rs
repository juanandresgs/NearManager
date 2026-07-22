use std::collections::BTreeMap;

use near_core::{CapabilitySet, CommandId, CommandInvocation, CommandValue, ContextId, SurfaceId};
use near_ops::{ConflictAction, OperationDecision, OperationPlan, OperationPresentation};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState, UpdateContext,
    UpdateResult,
};

pub struct OperationPreviewSurface {
    id: SurfaceId,
    plan: OperationPlan,
    conflict: ConflictAction,
    high_impact_armed: bool,
}

impl OperationPreviewSurface {
    pub fn new(id: impl Into<SurfaceId>, plan: OperationPlan) -> Self {
        let conflict = match plan.policies().conflict {
            near_ops::ConflictPolicy::Replace => ConflictAction::Replace,
            near_ops::ConflictPolicy::Rename => ConflictAction::Rename,
            near_ops::ConflictPolicy::Ask | near_ops::ConflictPolicy::Skip => ConflictAction::Skip,
        };
        Self {
            id: id.into(),
            plan,
            conflict,
            high_impact_armed: false,
        }
    }

    pub fn plan(&self) -> &OperationPlan {
        &self.plan
    }

    pub fn conflict(&self) -> ConflictAction {
        self.conflict
    }

    fn presentation(&self) -> OperationPresentation {
        self.plan.presentation()
    }

    fn execute_command(&self) -> CommandInvocation {
        CommandInvocation {
            id: CommandId::from("near.operation.confirmed"),
            arguments: BTreeMap::from([
                (
                    "plan".to_owned(),
                    CommandValue::String(self.plan.id().as_str().to_owned()),
                ),
                (
                    "conflict".to_owned(),
                    CommandValue::String(conflict_name(self.conflict).to_owned()),
                ),
                (
                    "high_impact_confirmed".to_owned(),
                    CommandValue::Boolean(self.plan.high_impact() && self.high_impact_armed),
                ),
            ]),
        }
    }

    fn preview_lines(&self) -> Vec<String> {
        let presentation = self.presentation();
        let mut lines = vec![presentation.confirmation];
        if presentation.details.is_empty() {
            lines.extend(self.plan.items().iter().filter_map(|item| {
                item.source
                    .as_ref()
                    .map(|source| format!("  {}", source.location.as_str()))
            }));
        } else {
            lines.extend(presentation.details);
        }
        lines.push(presentation.explanation);
        lines
    }

    fn allows(&self, decision: OperationDecision) -> bool {
        self.presentation().allowed_decisions.contains(&decision)
    }
}

impl Surface for OperationPreviewSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.operation-preview")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        let SurfaceEvent::Command(invocation) = event else {
            return UpdateResult::ignored();
        };
        match invocation.id.as_str() {
            "near.operation.conflict.replace" if self.allows(OperationDecision::Replace) => {
                self.conflict = ConflictAction::Replace;
            }
            "near.operation.conflict.skip" if self.allows(OperationDecision::Skip) => {
                self.conflict = ConflictAction::Skip;
            }
            "near.operation.conflict.rename" if self.allows(OperationDecision::Rename) => {
                self.conflict = ConflictAction::Rename;
            }
            "near.operation.execute"
                if self.allows(OperationDecision::ConfirmHighImpact) && !self.high_impact_armed =>
            {
                self.high_impact_armed = true;
            }
            "near.operation.execute" if self.allows(OperationDecision::Execute) => {
                return UpdateResult::dispatch(self.execute_command());
            }
            _ => return UpdateResult::ignored(),
        }
        UpdateResult::handled()
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        let presentation = self.presentation();
        scene.fill(area, "dialog.background");
        scene.border(
            area,
            Some(format!(" {} ", presentation.title)),
            "dialog.border",
        );
        let inner = area.inset(1);
        let show_conflict_controls = self.allows(OperationDecision::Replace)
            || self.allows(OperationDecision::Skip)
            || self.allows(OperationDecision::Rename);
        let reserved_rows = if show_conflict_controls { 2 } else { 1 };
        for (row, line) in self.preview_lines().into_iter().enumerate() {
            let Ok(row) = u16::try_from(row) else { break };
            if row >= inner.height.saturating_sub(reserved_rows) {
                break;
            }
            scene.text(
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                line,
                "text",
            );
        }
        if show_conflict_controls {
            scene.text(
                SceneRect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
                format!("conflict decision: {}", conflict_name(self.conflict)),
                "status.warning",
            );
        }
        let prompt = if self.allows(OperationDecision::ConfirmHighImpact) && !self.high_impact_armed
        {
            "Enter Arm irreversible operation   Esc Cancel"
        } else if self.allows(OperationDecision::ConfirmHighImpact) {
            "Enter CONFIRM irreversible operation   Esc Cancel"
        } else if show_conflict_controls {
            "Enter Execute   S Skip   R Replace   N Rename   Esc Cancel"
        } else {
            "Enter Execute   Esc Cancel"
        };
        scene.text(
            SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            prompt,
            "text",
        );
        scene
    }
}

fn conflict_name(action: ConflictAction) -> &'static str {
    match action {
        ConflictAction::Replace => "replace",
        ConflictAction::Skip => "skip",
        ConflictAction::Rename => "rename",
        ConflictAction::Cancel => "cancel",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_core::{
        ActionContext, CommandId, CommandInvocation, CommandValue, ListingGeneration, Location,
        SafetyClass,
    };
    use near_ops::{
        ConflictPolicy, OperationKind, OperationPlanner, PlanPolicies, PlanRequest, PlannedItem,
        RecoveryPolicy,
    };

    use super::OperationPreviewSurface;
    use crate::{RenderContext, ScenePrimitive, SceneRect, Surface, SurfaceEvent, UpdateContext};

    #[test]
    fn trash_preview_uses_plain_language_without_conflict_controls() {
        let plan = OperationPlanner::default()
            .plan(PlanRequest {
                kind: OperationKind::Trash,
                items: vec![PlannedItem {
                    source: Some(near_core::ResourceRef {
                        provider: "near.test".into(),
                        location: Location::new("file:///tmp/report.txt"),
                    }),
                    target: Location::new("file:///tmp/Trash/report.txt"),
                    conflict_expected: false,
                    recursive: false,
                    parameters: BTreeMap::new(),
                }],
                destination: Some(Location::new("file:///tmp/Trash")),
                policies: PlanPolicies {
                    conflict: ConflictPolicy::Rename,
                    recovery: RecoveryPolicy::Trash,
                    ..PlanPolicies::default()
                },
                safety: SafetyClass::Reversible,
                context_generation: ListingGeneration(1),
                high_impact: false,
            })
            .unwrap();
        let surface = OperationPreviewSurface::new("preview", plan);
        let action = ActionContext::default();
        let scene = surface.scene(
            SceneRect::new(0, 0, 80, 12),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        let text = scene
            .primitives()
            .iter()
            .filter_map(|primitive| match primitive {
                ScenePrimitive::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Move 1 item to Trash?"));
        assert!(text.contains("Enter Execute   Esc Cancel"));
        assert!(!text.contains("conflict decision"));
        assert!(!text.contains("R Replace"));

        let mut surface = surface;
        let hidden = SurfaceEvent::Command(CommandInvocation {
            id: CommandId::from("near.operation.conflict.replace"),
            arguments: BTreeMap::new(),
        });
        let result = surface.update(&hidden, &mut UpdateContext { action: &action });
        assert!(!result.handled);
        assert_eq!(surface.conflict(), near_ops::ConflictAction::Rename);
    }

    #[test]
    fn high_impact_plan_requires_two_explicit_execute_actions() {
        let plan = OperationPlanner::default()
            .plan(PlanRequest {
                kind: OperationKind::Delete,
                items: vec![PlannedItem {
                    source: Some(near_core::ResourceRef {
                        provider: "near.test".into(),
                        location: Location::new("test:///folder"),
                    }),
                    target: Location::new("test:///folder"),
                    conflict_expected: false,
                    recursive: true,
                    parameters: BTreeMap::new(),
                }],
                destination: None,
                policies: PlanPolicies::default(),
                safety: SafetyClass::Destructive,
                context_generation: ListingGeneration(1),
                high_impact: true,
            })
            .unwrap();
        let mut surface = OperationPreviewSurface::new("preview", plan);
        let action = ActionContext::default();
        let event = SurfaceEvent::Command(CommandInvocation {
            id: CommandId::from("near.operation.execute"),
            arguments: BTreeMap::new(),
        });

        let first = surface.update(&event, &mut UpdateContext { action: &action });
        assert!(first.handled);
        assert!(first.command.is_none());

        let second = surface.update(&event, &mut UpdateContext { action: &action });
        let command = second.command.unwrap();
        assert_eq!(command.id.as_str(), "near.operation.confirmed");
        assert_eq!(
            command.arguments.get("high_impact_confirmed"),
            Some(&CommandValue::Boolean(true))
        );
    }

    #[test]
    fn restore_preview_declares_original_destination() {
        let plan = OperationPlanner::default()
            .plan(PlanRequest {
                kind: OperationKind::Restore,
                items: vec![PlannedItem {
                    source: Some(near_core::ResourceRef {
                        provider: "near.test".into(),
                        location: Location::new("file:///Trash/report 2.txt"),
                    }),
                    target: Location::new("file:///Documents/report.txt"),
                    conflict_expected: true,
                    recursive: false,
                    parameters: BTreeMap::new(),
                }],
                destination: None,
                policies: PlanPolicies {
                    conflict: ConflictPolicy::Ask,
                    recovery: RecoveryPolicy::JournalOnly,
                    ..PlanPolicies::default()
                },
                safety: SafetyClass::Confirmable,
                context_generation: ListingGeneration(1),
                high_impact: false,
            })
            .unwrap();
        let mut surface = OperationPreviewSurface::new("preview", plan);
        let action = ActionContext::default();
        let scene = surface.scene(
            SceneRect::new(0, 0, 90, 14),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        let text = scene
            .primitives()
            .iter()
            .filter_map(|primitive| match primitive {
                ScenePrimitive::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Restore 1 item from Trash?"));
        assert!(text.contains("R Replace"));
        assert!(
            surface
                .presentation()
                .details
                .iter()
                .any(|line| line.contains("file:///Documents/report.txt"))
        );

        let result = surface.update(
            &SurfaceEvent::Command(CommandInvocation {
                id: CommandId::from("near.operation.execute"),
                arguments: BTreeMap::new(),
            }),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(
            result.command.unwrap().id.as_str(),
            "near.operation.confirmed"
        );
    }
}
