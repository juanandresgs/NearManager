use near_core::{CapabilitySet, CommandId, CommandInvocation, CommandValue, ContextId, SurfaceId};
#[cfg(feature = "embedded-pty")]
use near_pty::{
    PtyCell, PtyCellStyle, PtyColor, PtyError, PtySessionHandle, ShellProfile, TerminalSize,
};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState, UpdateContext,
    UpdateResult,
};
#[cfg(feature = "embedded-pty")]
use crate::{SceneColor, SceneTextStyle};

#[cfg(feature = "embedded-pty")]
fn render_pty_cells(
    scene: &mut Scene,
    area: SceneRect,
    rows: &[Vec<PtyCell>],
    start: usize,
    end: usize,
) {
    for (visible_row, cells) in rows[start.min(rows.len())..end.min(rows.len())]
        .iter()
        .enumerate()
    {
        let Ok(visible_row) = u16::try_from(visible_row) else {
            break;
        };
        let limit = cells.len().min(usize::from(area.width));
        let mut run_start = 0;
        while run_start < limit {
            let style = cells[run_start].style;
            let mut run_end = run_start + 1;
            while run_end < limit && cells[run_end].style == style {
                run_end += 1;
            }
            let mut contents = String::new();
            for cell in &cells[run_start..run_end] {
                if cell.width == 0 {
                    continue;
                }
                if cell.contents.is_empty() {
                    contents.push(' ');
                } else {
                    contents.push_str(&cell.contents);
                }
            }
            scene.styled_text(
                SceneRect::new(
                    area.x + u16::try_from(run_start).unwrap_or(u16::MAX),
                    area.y + visible_row,
                    u16::try_from(run_end - run_start).unwrap_or(u16::MAX),
                    1,
                ),
                contents,
                "terminal.text",
                terminal_scene_style(style),
            );
            run_start = run_end;
        }
    }
}

#[cfg(feature = "embedded-pty")]
fn terminal_scene_style(style: PtyCellStyle) -> SceneTextStyle {
    SceneTextStyle {
        foreground: Some(terminal_scene_color(style.foreground)),
        background: Some(terminal_scene_color(style.background)),
        bold: style.bold,
        dim: style.dim,
        italic: style.italic,
        underline: style.underline,
        inverse: style.inverse,
    }
}

#[cfg(feature = "embedded-pty")]
const fn terminal_scene_color(color: PtyColor) -> SceneColor {
    match color {
        PtyColor::Default => SceneColor::TerminalDefault,
        PtyColor::Indexed(index) => SceneColor::Indexed(index),
        PtyColor::Rgb { red, green, blue } => SceneColor::Rgb { red, green, blue },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalInputMode {
    Normal,
    Application,
    Copy,
}

pub struct TerminalSurface {
    id: SurfaceId,
    title: String,
    lines: Vec<String>,
    scrollback_limit: usize,
    scroll: usize,
    mode: TerminalInputMode,
    cursor: Option<(u16, u16)>,
}

impl TerminalSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        scrollback_limit: usize,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            lines: Vec::new(),
            scrollback_limit: scrollback_limit.max(1),
            scroll: 0,
            mode: TerminalInputMode::Normal,
            cursor: None,
        }
    }

    pub fn append_output(&mut self, output: &str) {
        for line in output.split('\n') {
            self.lines.push(line.to_owned());
        }
        let excess = self.lines.len().saturating_sub(self.scrollback_limit);
        if excess > 0 {
            self.lines.drain(..excess);
        }
        self.scroll = 0;
    }

    pub fn set_cursor(&mut self, cursor: Option<(u16, u16)>) {
        self.cursor = cursor;
    }

    pub fn mode(&self) -> TerminalInputMode {
        self.mode
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    fn input_command(text: String) -> CommandInvocation {
        CommandInvocation {
            id: CommandId::from("near.terminal.input"),
            arguments: [("text".to_owned(), CommandValue::String(text))]
                .into_iter()
                .collect(),
        }
    }
}

impl Surface for TerminalSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.terminal")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        match event {
            SurfaceEvent::Text(text) | SurfaceEvent::Paste(text)
                if self.mode != TerminalInputMode::Copy =>
            {
                UpdateResult::dispatch(Self::input_command(text.clone()))
            }
            SurfaceEvent::Backspace if self.mode != TerminalInputMode::Copy => {
                UpdateResult::dispatch(Self::input_command("\u{7f}".to_owned()))
            }
            SurfaceEvent::Command(invocation) => match invocation.id.as_str() {
                "near.terminal.scroll-up" => {
                    self.scroll = self
                        .scroll
                        .saturating_add(1)
                        .min(self.lines.len().saturating_sub(1));
                    UpdateResult::handled()
                }
                "near.terminal.scroll-down" => {
                    self.scroll = self.scroll.saturating_sub(1);
                    UpdateResult::handled()
                }
                "near.terminal.copy-mode" => {
                    self.mode = TerminalInputMode::Copy;
                    UpdateResult::handled()
                }
                "near.terminal.application-mode" => {
                    self.mode = TerminalInputMode::Application;
                    UpdateResult::handled()
                }
                "near.terminal.normal-mode" => {
                    self.mode = TerminalInputMode::Normal;
                    UpdateResult::handled()
                }
                "near.terminal.clear" => {
                    self.lines.clear();
                    self.scroll = 0;
                    UpdateResult::handled()
                }
                _ => UpdateResult::ignored(),
            },
            _ => UpdateResult::ignored(),
        }
    }

    fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        let border = if context.focused {
            "panel.border.focused"
        } else {
            "panel.border"
        };
        scene.fill(area, "terminal.background");
        scene.border(
            area,
            Some(format!(" {} [{:?}] ", self.title, self.mode)),
            border,
        );
        let inner = area.inset(1);
        let end = self.lines.len().saturating_sub(self.scroll);
        let start = end.saturating_sub(usize::from(inner.height));
        for (row, line) in self.lines[start..end].iter().enumerate() {
            let Ok(row) = u16::try_from(row) else {
                break;
            };
            scene.text(
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                line,
                "terminal.text",
            );
        }
        if let Some((column, row)) = self.cursor
            && column < inner.width
            && row < inner.height
        {
            scene.text(
                SceneRect::new(inner.x + column, inner.y + row, 1, 1),
                " ",
                "terminal.cursor",
            );
        }
        scene
    }
}

#[cfg(feature = "embedded-pty")]
pub struct EmbeddedTerminalSurface {
    id: SurfaceId,
    title: String,
    session: PtySessionHandle,
    scroll: usize,
    mode: TerminalInputMode,
    last_error: Option<String>,
}

#[cfg(feature = "embedded-pty")]
pub struct EmbeddedTerminalDockSurface {
    id: SurfaceId,
    session: PtySessionHandle,
    terminal_size: TerminalSize,
}

#[cfg(feature = "embedded-pty")]
impl EmbeddedTerminalDockSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        session: PtySessionHandle,
        terminal_size: TerminalSize,
    ) -> Self {
        Self {
            id: id.into(),
            session,
            terminal_size,
        }
    }
}

#[cfg(feature = "embedded-pty")]
impl Surface for EmbeddedTerminalDockSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.terminal-dock")]
    }

    fn capabilities(&self) -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        capabilities.insert("terminal.docked");
        capabilities
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        _ = match event {
            SurfaceEvent::Text(text) => self.session.write(text.as_bytes()),
            SurfaceEvent::Paste(text) => self.session.paste(text),
            SurfaceEvent::Backspace => self.session.write(&[0x7f]),
            _ => return UpdateResult::ignored(),
        };
        UpdateResult::handled()
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let _ = self.session.resize(self.terminal_size);
        let snapshot = self.session.snapshot();
        let cursor_row = usize::from(snapshot.cursor.0);
        let visible_rows = usize::from(area.height);
        let end = (cursor_row + 1).max(visible_rows).min(snapshot.lines.len());
        let start = end.saturating_sub(visible_rows);
        let mut scene = Scene::new();
        scene.fill(area, "terminal.background");
        render_pty_cells(&mut scene, area, &snapshot.cells, start, end);
        if snapshot.exit_code.is_none() && cursor_row >= start && cursor_row < end {
            let row = u16::try_from(cursor_row - start).unwrap_or(u16::MAX);
            if row < area.height && snapshot.cursor.1 < area.width {
                scene.text(
                    SceneRect::new(area.x + snapshot.cursor.1, area.y + row, 1, 1),
                    " ",
                    "terminal.cursor",
                );
            }
        }
        scene
    }
}

#[cfg(feature = "embedded-pty")]
impl EmbeddedTerminalSurface {
    /// Spawns an interactive zsh terminal surface.
    ///
    /// # Errors
    ///
    /// Returns native PTY or zsh spawn failures.
    pub fn spawn_zsh(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        current_directory: Option<&std::path::Path>,
    ) -> Result<Self, PtyError> {
        Ok(Self {
            id: id.into(),
            title: title.into(),
            session: PtySessionHandle::spawn_zsh(current_directory)?,
            scroll: 0,
            mode: TerminalInputMode::Normal,
            last_error: None,
        })
    }

    /// Creates an embedded surface backed by the native platform shell.
    ///
    /// # Errors
    ///
    /// Returns PTY backend or process-spawn failures.
    pub fn spawn_shell(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        current_directory: Option<&std::path::Path>,
    ) -> Result<Self, PtyError> {
        Ok(Self {
            id: id.into(),
            title: title.into(),
            session: PtySessionHandle::spawn_shell(current_directory)?,
            scroll: 0,
            mode: TerminalInputMode::Normal,
            last_error: None,
        })
    }

    /// Creates an embedded surface from a versioned shell profile.
    ///
    /// # Errors
    ///
    /// Returns PTY backend or process-spawn failures.
    pub fn spawn_profile(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        profile: &ShellProfile,
        current_directory: Option<&std::path::Path>,
    ) -> Result<Self, PtyError> {
        Ok(Self::from_session(
            id,
            title,
            PtySessionHandle::spawn_profile(profile, current_directory)?,
        ))
    }

    pub fn from_session(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        session: PtySessionHandle,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            session,
            scroll: 0,
            mode: TerminalInputMode::Normal,
            last_error: None,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        if let Err(error) = self.session.write(bytes) {
            self.last_error = Some(error.to_string());
        }
    }

    fn write_key(&mut self, key: &str) {
        let application = self.session.snapshot().application_cursor;
        let bytes: &[u8] = match key {
            "enter" => b"\r",
            "tab" => b"\t",
            "escape" => b"\x1b",
            "up" if application => b"\x1bOA",
            "down" if application => b"\x1bOB",
            "right" if application => b"\x1bOC",
            "left" if application => b"\x1bOD",
            "up" => b"\x1b[A",
            "down" => b"\x1b[B",
            "right" => b"\x1b[C",
            "left" => b"\x1b[D",
            "home" => b"\x1b[H",
            "end" => b"\x1b[F",
            "delete" => b"\x1b[3~",
            _ => return,
        };
        self.write(bytes);
    }

    fn terminal_command(&mut self, invocation: &CommandInvocation) -> bool {
        match invocation.id.as_str() {
            "near.terminal.send-key" => {
                if let Some(key) = invocation
                    .arguments
                    .get("key")
                    .and_then(CommandValue::as_str)
                {
                    self.write_key(key);
                }
            }
            "near.terminal.interrupt" => {
                if let Err(error) = self.session.interrupt() {
                    self.last_error = Some(error.to_string());
                }
            }
            "near.terminal.eof" => {
                if let Err(error) = self.session.end_of_file() {
                    self.last_error = Some(error.to_string());
                }
            }
            "near.terminal.scroll-up" => {
                self.scroll = self.scroll.saturating_add(1);
                self.session.set_scrollback(self.scroll);
                self.mode = TerminalInputMode::Copy;
            }
            "near.terminal.scroll-down" => {
                self.scroll = self.scroll.saturating_sub(1);
                self.session.set_scrollback(self.scroll);
                if self.scroll == 0 {
                    self.mode = TerminalInputMode::Normal;
                }
            }
            "near.terminal.copy-mode" => self.mode = TerminalInputMode::Copy,
            "near.terminal.normal-mode" => {
                self.scroll = 0;
                self.session.set_scrollback(0);
                self.mode = TerminalInputMode::Normal;
            }
            "near.terminal.clear" => self.write(&[0x0c]),
            _ => return false,
        }
        true
    }
}

#[cfg(feature = "embedded-pty")]
impl Surface for EmbeddedTerminalSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.terminal")]
    }

    fn capabilities(&self) -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        capabilities.insert("terminal.embedded");
        capabilities
    }

    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        match event {
            SurfaceEvent::Text(text) if self.mode != TerminalInputMode::Copy => {
                self.write(text.as_bytes());
                UpdateResult::handled()
            }
            SurfaceEvent::Paste(text) if self.mode != TerminalInputMode::Copy => {
                if let Err(error) = self.session.paste(text) {
                    self.last_error = Some(error.to_string());
                }
                UpdateResult::handled()
            }
            SurfaceEvent::Backspace if self.mode != TerminalInputMode::Copy => {
                self.write(&[0x7f]);
                UpdateResult::handled()
            }
            SurfaceEvent::Command(invocation) if self.terminal_command(invocation) => {
                UpdateResult::handled()
            }
            _ => UpdateResult::ignored(),
        }
    }

    fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene {
        let inner = area.inset(1);
        let size = TerminalSize::new(inner.height, inner.width);
        let _ = self.session.resize(size);
        let snapshot = self.session.snapshot();
        let mut scene = Scene::new();
        scene.fill(area, "terminal.background");
        let state = if snapshot.alternate_screen {
            "alternate"
        } else if snapshot.exit_code.is_some() {
            "exited"
        } else {
            "running"
        };
        let cwd = snapshot
            .current_directory_uri
            .as_deref()
            .map_or(String::new(), |cwd| format!(" {cwd}"));
        let error = self
            .last_error
            .as_deref()
            .map_or(String::new(), |error| format!(" error: {error}"));
        let profile = snapshot
            .shell_profile
            .as_ref()
            .map_or_else(String::new, |profile| {
                format!(" {}", profile.lifecycle_label())
            });
        scene.border(
            area,
            Some(format!(
                " {} [{state}/{:?}{profile}]{cwd}{error} ",
                self.title, self.mode,
            )),
            if context.focused {
                "panel.border.focused"
            } else {
                "panel.border"
            },
        );
        render_pty_cells(
            &mut scene,
            inner,
            &snapshot.cells,
            0,
            usize::from(inner.height),
        );
        if snapshot.exit_code.is_none() {
            let (row, column) = snapshot.cursor;
            if row < inner.height && column < inner.width {
                scene.text(
                    SceneRect::new(inner.x + column, inner.y + row, 1, 1),
                    " ",
                    "terminal.cursor",
                );
            }
        }
        scene
    }
}

#[cfg(all(test, feature = "embedded-pty"))]
mod tests {
    use near_pty::{PtyCell, PtyCellStyle, PtyColor};

    use super::{Scene, SceneColor, SceneRect, render_pty_cells};
    use crate::ScenePrimitive;

    fn style(foreground: PtyColor) -> PtyCellStyle {
        PtyCellStyle {
            foreground,
            background: PtyColor::Default,
            bold: foreground != PtyColor::Default,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }

    #[test]
    fn pty_cells_render_as_styled_runs_with_wide_geometry() {
        let rows = vec![vec![
            PtyCell {
                contents: "A".to_owned(),
                width: 1,
                style: style(PtyColor::Default),
            },
            PtyCell {
                contents: "R".to_owned(),
                width: 1,
                style: style(PtyColor::Indexed(1)),
            },
            PtyCell {
                contents: "界".to_owned(),
                width: 2,
                style: style(PtyColor::Rgb {
                    red: 2,
                    green: 3,
                    blue: 4,
                }),
            },
            PtyCell {
                contents: String::new(),
                width: 0,
                style: style(PtyColor::Rgb {
                    red: 2,
                    green: 3,
                    blue: 4,
                }),
            },
        ]];
        let mut scene = Scene::new();
        render_pty_cells(&mut scene, SceneRect::new(0, 0, 4, 1), &rows, 0, 1);
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::StyledText { content, style, .. }
                if content == "R"
                    && style.foreground == Some(SceneColor::Indexed(1))
                    && style.bold
        )));
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::StyledText { area, content, .. }
                if content == "界" && area.width == 2
        )));
    }
}
