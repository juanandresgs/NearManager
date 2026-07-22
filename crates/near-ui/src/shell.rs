use near_core::{ActionContext, CapabilitySet, SurfaceId};

use crate::{RenderContext, Scene, SceneRect, Surface, SurfaceEvent, UpdateContext, UpdateResult};

pub struct SurfaceShell {
    surfaces: Vec<Box<dyn Surface>>,
    focused: usize,
    peer: Option<usize>,
}

impl SurfaceShell {
    pub fn single(surface: impl Surface + 'static) -> Self {
        Self {
            surfaces: vec![Box::new(surface)],
            focused: 0,
            peer: None,
        }
    }

    pub fn focused_peer(focused: impl Surface + 'static, peer: impl Surface + 'static) -> Self {
        Self {
            surfaces: vec![Box::new(focused), Box::new(peer)],
            focused: 0,
            peer: Some(1),
        }
    }

    pub fn focused_id(&self) -> SurfaceId {
        self.surfaces[self.focused].id()
    }

    pub fn peer_id(&self) -> Option<SurfaceId> {
        self.peer.map(|index| self.surfaces[index].id())
    }

    pub fn focus_peer(&mut self) -> bool {
        let Some(peer) = self.peer else {
            return false;
        };
        let previous = self.focused;
        self.focused = peer;
        self.peer = Some(previous);
        true
    }

    pub fn swap_peer_positions(&mut self) -> bool {
        let Some(peer) = self.peer else {
            return false;
        };
        let previous = self.focused;
        self.surfaces.swap(self.focused, peer);
        self.focused = peer;
        self.peer = Some(previous);
        true
    }

    pub fn action_context(&self) -> ActionContext {
        let focused = self.surfaces[self.focused].state();
        let peer = self.peer.map(|index| self.surfaces[index].state());
        ActionContext {
            focused_surface: Some(self.surfaces[self.focused].id()),
            peer_surface: self.peer_id(),
            current: focused.current,
            selected: focused.selected,
            location: focused.location,
            peer_location: peer.and_then(|state| state.location),
            capabilities: self.surfaces[self.focused].capabilities(),
            peer_capabilities: self.peer.map_or_else(CapabilitySet::default, |index| {
                self.surfaces[index].capabilities()
            }),
        }
    }

    pub fn active_contexts(&self) -> Vec<near_core::ContextId> {
        self.surfaces[self.focused].contexts()
    }

    pub fn dispatch_focused(&mut self, event: &SurfaceEvent) -> UpdateResult {
        let action = self.action_context();
        self.surfaces[self.focused].update(event, &mut UpdateContext { action: &action })
    }

    pub fn scene(&self, area: SceneRect) -> Scene {
        let action = self.action_context();
        let mut scene = Scene::new();
        if self.peer.is_some() {
            let left_width = area.width / 2;
            let right_width = area.width.saturating_sub(left_width);
            let slots = [
                SceneRect::new(area.x, area.y, left_width, area.height),
                SceneRect::new(area.x + left_width, area.y, right_width, area.height),
            ];
            for (slot, surface) in self.surfaces.iter().take(2).enumerate() {
                scene.extend(surface.scene(
                    slots[slot],
                    &RenderContext {
                        focused: slot == self.focused,
                        action: &action,
                    },
                ));
            }
        } else {
            scene.extend(self.surfaces[self.focused].scene(
                area,
                &RenderContext {
                    focused: true,
                    action: &action,
                },
            ));
        }
        scene
    }

    pub fn capabilities(&self) -> CapabilitySet {
        self.surfaces[self.focused].capabilities()
    }
}

#[cfg(test)]
mod tests {
    use near_core::{CapabilitySet, ContextId, Location, ProviderId, ResourceRef, SurfaceId};

    use crate::{
        RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceShell, SurfaceState,
        UpdateContext, UpdateResult,
    };

    struct StubSurface {
        id: &'static str,
        location: &'static str,
    }

    impl Surface for StubSurface {
        fn id(&self) -> SurfaceId {
            SurfaceId::from(self.id)
        }

        fn contexts(&self) -> Vec<ContextId> {
            vec![ContextId::from("test.surface")]
        }

        fn capabilities(&self) -> CapabilitySet {
            CapabilitySet::default()
        }

        fn state(&self) -> SurfaceState {
            SurfaceState {
                current: Some(ResourceRef {
                    provider: ProviderId::from("test"),
                    location: Location::new(format!("{}/current", self.location)),
                }),
                selected: Vec::new(),
                location: Some(Location::new(self.location)),
            }
        }

        fn update(
            &mut self,
            _event: &SurfaceEvent,
            _context: &mut UpdateContext<'_>,
        ) -> UpdateResult {
            UpdateResult::ignored()
        }

        fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene {
            let mut scene = Scene::new();
            scene.text(
                area,
                self.id,
                if context.focused {
                    "control.focused"
                } else {
                    "text"
                },
            );
            scene
        }
    }

    #[test]
    fn focused_and_peer_context_move_independently_of_positions() {
        let mut shell = SurfaceShell::focused_peer(
            StubSurface {
                id: "left",
                location: "/left",
            },
            StubSurface {
                id: "right",
                location: "/right",
            },
        );
        let context = shell.action_context();
        assert_eq!(context.focused_surface, Some(SurfaceId::from("left")));
        assert_eq!(context.peer_surface, Some(SurfaceId::from("right")));
        assert_eq!(context.location.unwrap().as_str(), "/left");
        assert_eq!(context.peer_location.unwrap().as_str(), "/right");

        assert!(shell.focus_peer());
        let context = shell.action_context();
        assert_eq!(context.focused_surface, Some(SurfaceId::from("right")));
        assert_eq!(context.peer_surface, Some(SurfaceId::from("left")));
        let scene = shell.scene(SceneRect::new(0, 0, 20, 4));
        assert_eq!(scene.primitives().len(), 2);

        assert!(shell.swap_peer_positions());
        let swapped = shell.action_context();
        assert_eq!(swapped.focused_surface, Some(SurfaceId::from("right")));
        assert_eq!(swapped.peer_surface, Some(SurfaceId::from("left")));
        let scene = shell.scene(SceneRect::new(0, 0, 20, 4));
        let crate::ScenePrimitive::Text { content, .. } = &scene.primitives()[0] else {
            panic!("expected positioned surface text");
        };
        assert_eq!(content, "right");
    }

    #[test]
    fn single_surface_has_no_peer_requirement() {
        let mut shell = SurfaceShell::single(StubSurface {
            id: "only",
            location: "/only",
        });
        assert!(!shell.focus_peer());
        let context = shell.action_context();
        assert_eq!(context.focused_surface, Some(SurfaceId::from("only")));
        assert_eq!(context.peer_surface, None);
        assert_eq!(context.peer_location, None);
    }
}
