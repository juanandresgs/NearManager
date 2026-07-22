use near_core::{
    ActionContext, CapabilitySet, CommandInvocation, ContextId, Location, ResourceRef, SurfaceId,
    ViewerStateEntry,
};

use crate::{Scene, SceneRect};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SurfaceState {
    pub current: Option<ResourceRef>,
    pub selected: Vec<ResourceRef>,
    pub location: Option<Location>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceEvent {
    Command(CommandInvocation),
    Text(String),
    SelectionSearchText(String),
    SelectionSearchBackspace,
    Paste(String),
    Backspace,
    FocusGained,
    FocusLost,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateResult {
    pub handled: bool,
    pub redraw: bool,
    pub command: Option<CommandInvocation>,
}

impl UpdateResult {
    pub const fn handled() -> Self {
        Self {
            handled: true,
            redraw: true,
            command: None,
        }
    }

    pub const fn ignored() -> Self {
        Self {
            handled: false,
            redraw: false,
            command: None,
        }
    }

    pub fn dispatch(command: CommandInvocation) -> Self {
        Self {
            handled: true,
            redraw: true,
            command: Some(command),
        }
    }
}

pub struct UpdateContext<'a> {
    pub action: &'a ActionContext,
}

pub struct RenderContext<'a> {
    pub focused: bool,
    pub action: &'a ActionContext,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfacePresentation {
    #[default]
    Modal,
    FullScreen,
}

pub trait Surface {
    fn id(&self) -> SurfaceId;

    fn contexts(&self) -> Vec<ContextId>;

    fn capabilities(&self) -> CapabilitySet;

    fn state(&self) -> SurfaceState;

    fn presentation(&self) -> SurfacePresentation {
        SurfacePresentation::Modal
    }

    fn configure_interaction(&mut self, _menu_wrap: bool, _dialog_wrap: bool) {}

    fn viewer_state(&self) -> Option<ViewerStateEntry> {
        None
    }

    fn update(&mut self, event: &SurfaceEvent, context: &mut UpdateContext<'_>) -> UpdateResult;

    fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene;
}
