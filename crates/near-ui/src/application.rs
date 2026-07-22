use std::{collections::BTreeMap, io, time::Duration};

use near_core::{ActionContext, CommandId, CommandInvocation, ContextId};
use near_terminal::{
    Key, KeyKind, KeyStroke, TerminalEvent, TerminalEventReactor, TerminalRuntimeEvent,
    TerminalSession, TerminalSessionError, dimensions,
};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};
use thiserror::Error;

use crate::render_loop::RenderInvalidation;
use crate::{
    HelpEntry, HelpSurface, Keymap, MenuItem, MenuSurface, RenderContext, ResolveResult, Scene,
    SceneRect, SemanticSnapshot, SemanticTheme, Surface, SurfaceEvent, TerminalColorDepth,
    UpdateContext, format_key_sequence,
    scene_renderer::{render_scene, snapshot_scene},
    semantic::RoleBuffer,
};

#[derive(Debug, Error)]
pub enum RunApplicationError {
    #[error(transparent)]
    TerminalSession(#[from] TerminalSessionError),
    #[error("terminal I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("terminated by signal {0}")]
    Terminated(i32),
}

enum ApplicationOverlay {
    Help(HelpSurface),
    Palette(MenuSurface),
}

impl ApplicationOverlay {
    fn surface(&self) -> &dyn Surface {
        match self {
            Self::Help(surface) => surface,
            Self::Palette(surface) => surface,
        }
    }

    fn surface_mut(&mut self) -> &mut dyn Surface {
        match self {
            Self::Help(surface) => surface,
            Self::Palette(surface) => surface,
        }
    }
}

pub struct SurfaceApplication {
    id: String,
    title: String,
    surface: Box<dyn Surface>,
    overlay: Option<ApplicationOverlay>,
    should_quit: bool,
}

impl SurfaceApplication {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        surface: impl Surface + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            surface: Box::new(surface),
            overlay: None,
            should_quit: false,
        }
    }

    pub fn active_contexts(&self) -> Vec<ContextId> {
        self.overlay.as_ref().map_or_else(
            || self.surface.contexts(),
            |overlay| overlay.surface().contexts(),
        )
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn action_context(&self) -> ActionContext {
        let state = self.surface.state();
        ActionContext {
            focused_surface: Some(self.surface.id()),
            current: state.current,
            selected: state.selected,
            location: state.location,
            capabilities: self.surface.capabilities(),
            ..ActionContext::default()
        }
    }

    pub fn dispatch(&mut self, invocation: &CommandInvocation, keymap: &Keymap) {
        match invocation.id.as_str() {
            "near.app.quit" => self.should_quit = true,
            "near.overlay.cancel" => {
                if self.overlay.is_some() {
                    self.overlay = None;
                } else {
                    self.should_quit = true;
                }
            }
            "near.help.context" | "near.help.contents" | "near.help.extensions" => {
                self.open_help(keymap);
            }
            "near.command-palette.open" => self.open_palette(keymap),
            _ => self.dispatch_to_active(&SurfaceEvent::Command(invocation.clone()), keymap),
        }
    }

    pub fn handle_terminal_event(&mut self, keymap: &mut Keymap, event: TerminalEvent) {
        self.handle_terminal_event_with(keymap, event, |keymap, contexts, stroke| {
            keymap.resolve(contexts, stroke)
        });
    }

    pub fn handle_terminal_event_at(
        &mut self,
        keymap: &mut Keymap,
        event: TerminalEvent,
        now: Duration,
    ) {
        self.handle_terminal_event_with(keymap, event, |keymap, contexts, stroke| {
            keymap.resolve_at(contexts, stroke, now)
        });
    }

    fn handle_terminal_event_with(
        &mut self,
        keymap: &mut Keymap,
        event: TerminalEvent,
        resolve: impl FnOnce(&mut Keymap, &[ContextId], KeyStroke) -> ResolveResult,
    ) {
        match event {
            TerminalEvent::Key(stroke) => {
                match resolve(keymap, &self.active_contexts(), stroke.clone()) {
                    ResolveResult::Matched(invocation) => self.dispatch(&invocation, keymap),
                    ResolveResult::NoMatch => {
                        if let Some(event) = unmatched_surface_event(&stroke) {
                            self.dispatch_to_active(&event, keymap);
                        }
                    }
                    ResolveResult::Pending { .. } => {}
                }
            }
            TerminalEvent::Paste(text) => {
                self.dispatch_to_active(&SurfaceEvent::Text(text), keymap);
            }
            TerminalEvent::FocusGained => {
                self.dispatch_to_active(&SurfaceEvent::FocusGained, keymap);
            }
            TerminalEvent::FocusLost => {
                self.dispatch_to_active(&SurfaceEvent::FocusLost, keymap);
            }
            _ => {}
        }
    }

    pub fn handle_keymap_timeout(&mut self, keymap: &mut Keymap) {
        if let ResolveResult::Matched(invocation) = keymap.expire_pending() {
            self.dispatch(&invocation, keymap);
        }
    }

    pub fn handle_keymap_timeout_at(&mut self, keymap: &mut Keymap, now: Duration) {
        if let ResolveResult::Matched(invocation) = keymap.expire_pending_at(now) {
            self.dispatch(&invocation, keymap);
        }
    }

    pub fn scene(&self, area: SceneRect) -> Scene {
        let action = self.action_context();
        let mut scene = self.surface.scene(
            area,
            &RenderContext {
                focused: self.overlay.is_none(),
                action: &action,
            },
        );
        if let Some(overlay) = &self.overlay {
            scene.extend(overlay.surface().scene(
                area,
                &RenderContext {
                    focused: true,
                    action: &action,
                },
            ));
        }
        scene
    }

    pub fn snapshot(&self, theme: &SemanticTheme, width: u16, height: u16) -> SemanticSnapshot {
        snapshot_scene(
            &self.scene(SceneRect::new(0, 0, width, height)),
            theme,
            width,
            height,
        )
    }

    fn dispatch_to_active(&mut self, event: &SurfaceEvent, keymap: &Keymap) {
        let action = self.action_context();
        let result = if let Some(overlay) = &mut self.overlay {
            overlay
                .surface_mut()
                .update(event, &mut UpdateContext { action: &action })
        } else {
            self.surface
                .update(event, &mut UpdateContext { action: &action })
        };
        if let Some(command) = result.command {
            self.overlay = None;
            self.dispatch(&command, keymap);
        }
    }

    fn primary_bindings<'a>(&self, keymap: &'a Keymap) -> Vec<&'a crate::KeyBinding> {
        keymap.bindings_for(&self.surface.contexts())
    }

    fn open_help(&mut self, keymap: &Keymap) {
        let active_contexts = self.surface.contexts();
        let mut bindings = self.primary_bindings(keymap);
        bindings.sort_by_key(|binding| {
            active_contexts
                .iter()
                .position(|context| context == &binding.origin.context)
                .unwrap_or(usize::MAX)
        });
        let entries = bindings
            .into_iter()
            .map(|binding| HelpEntry {
                keys: format_key_sequence(&binding.sequence),
                command: binding.invocation.id.as_str().to_owned(),
                description: binding.description.clone().unwrap_or_default(),
            })
            .collect();
        self.overlay = Some(ApplicationOverlay::Help(HelpSurface::new(
            format!("{}.help", self.id),
            format!("{} Help", self.title),
            "Effective commands and bindings for the active surface.",
            entries,
        )));
    }

    fn open_palette(&mut self, keymap: &Keymap) {
        let mut commands = BTreeMap::<CommandId, MenuItem>::new();
        for binding in self.primary_bindings(keymap) {
            commands
                .entry(binding.invocation.id.clone())
                .or_insert_with(|| MenuItem {
                    label: binding
                        .description
                        .clone()
                        .unwrap_or_else(|| binding.invocation.id.as_str().to_owned()),
                    description: format_key_sequence(&binding.sequence),
                    command: binding.invocation.clone(),
                    enabled: true,
                });
        }
        self.overlay = Some(ApplicationOverlay::Palette(MenuSurface::new(
            format!("{}.palette", self.id),
            format!("{} Commands", self.title),
            commands.into_values().collect(),
        )));
    }
}

fn unmatched_surface_event(stroke: &KeyStroke) -> Option<SurfaceEvent> {
    if stroke.kind == KeyKind::Release {
        return None;
    }
    match stroke.key {
        Key::Character(character)
            if !stroke.modifiers.control
                && !stroke.modifiers.alt
                && !stroke.modifiers.super_key =>
        {
            Some(SurfaceEvent::Text(character.to_string()))
        }
        Key::Backspace => Some(SurfaceEvent::Backspace),
        _ => None,
    }
}

/// Runs a backend-independent surface through Near's terminal runtime.
///
/// # Errors
///
/// Returns an error if terminal initialization, rendering, input, signal handling, or restoration
/// fails.
pub fn run_surface_application(
    mut application: SurfaceApplication,
    theme: &SemanticTheme,
    mut keymap: Keymap,
) -> Result<(), RunApplicationError> {
    let theme = theme
        .clone()
        .with_depth(TerminalColorDepth::detect_from_environment());
    let mut session = TerminalSession::enter()?;
    let mut reactor = TerminalEventReactor::new()?;
    let backend = CrosstermBackend::new(session.output_mut());
    let (columns, rows) = dimensions();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, columns, rows)),
        },
    )?;
    let mut roles = RoleBuffer::new(columns, rows, "workspace.background");
    let mut terminated = None;
    let mut redraw = RenderInvalidation::initial();
    while !application.should_quit() {
        if redraw.take() {
            terminal.draw(|frame| {
                let area = frame.area();
                roles.fill(area, "workspace.background");
                let scene =
                    application.scene(SceneRect::new(area.x, area.y, area.width, area.height));
                render_scene(frame, &scene, &theme, &mut roles);
            })?;
        }
        let runtime_event = reactor.wait(keymap.time_until_pending_timeout())?;
        let event = match runtime_event {
            TerminalRuntimeEvent::Terminal(event) => event,
            TerminalRuntimeEvent::Terminate(signal) => {
                terminated = Some(signal);
                break;
            }
            TerminalRuntimeEvent::Wake => continue,
            TerminalRuntimeEvent::Timeout => {
                if keymap
                    .time_until_pending_timeout()
                    .is_some_and(|timeout| timeout.is_zero())
                {
                    application.handle_keymap_timeout(&mut keymap);
                    redraw.request();
                }
                continue;
            }
        };
        if let TerminalEvent::Resize { columns, rows } = event {
            terminal.resize(Rect::new(0, 0, columns, rows))?;
            roles = RoleBuffer::new(columns, rows, "workspace.background");
        }
        application.handle_terminal_event(&mut keymap, event);
        redraw.request();
    }
    drop(terminal);
    session.restore()?;
    terminated.map_or(Ok(()), |signal| {
        Err(RunApplicationError::Terminated(signal))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_core::{CommandId, CommandInvocation};

    use super::SurfaceApplication;
    use crate::{Keymap, SemanticTheme, ViewerSurface};

    const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
    const THEME: &str = include_str!("../../../specs/theme.toml");

    #[test]
    fn single_surface_runtime_routes_help_palette_and_viewer_commands() {
        let mut application = SurfaceApplication::new(
            "test.viewer",
            "Viewer",
            ViewerSurface::text("viewer", "Document", "alpha\nbeta\n"),
        );
        let keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        application.dispatch(
            &CommandInvocation {
                id: CommandId::from("near.help.context"),
                arguments: BTreeMap::new(),
            },
            &keymap,
        );
        assert!(
            application
                .snapshot(&theme, 90, 24)
                .text_lines()
                .join("\n")
                .contains("Viewer Help")
        );
        application.dispatch(
            &CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
            &keymap,
        );
        application.dispatch(
            &CommandInvocation {
                id: CommandId::from("near.viewer.toggle-hex"),
                arguments: BTreeMap::new(),
            },
            &keymap,
        );
        assert!(
            application
                .snapshot(&theme, 90, 24)
                .text_lines()
                .join("\n")
                .contains("61 6c 70 68")
        );
    }

    #[test]
    fn single_surface_runtime_has_no_peer_requirement() {
        let application = SurfaceApplication::new(
            "test.single",
            "Single",
            ViewerSurface::text("viewer", "Document", "content"),
        );
        assert!(application.action_context().peer_surface.is_none());
        assert!(application.action_context().peer_location.is_none());
    }
}
