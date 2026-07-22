use std::collections::BTreeMap;

use super::{
    FarWorkspace, FocusedPanel, Frame, MenuItem, MenuSurface, Overlay, Rect, RoleBuffer,
    SemanticTheme, Surface, ZoomablePanePresentation,
};
use crate::{Keymap, PaneSlot, TabId, format_key_sequence};
use near_core::{CommandId, CommandInvocation, CommandValue};
use ratatui::{
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

#[cfg(any(test, feature = "embedded-pty"))]
use super::TerminalTab;
#[cfg(feature = "embedded-pty")]
use super::{EmbeddedTerminalSession, EmbeddedTerminalSurface};
#[cfg(feature = "embedded-pty")]
use near_terminal::{Key, KeyStroke, Modifiers};

#[cfg(feature = "embedded-pty")]
fn write_key(
    session: &EmbeddedTerminalSession,
    stroke: &KeyStroke,
) -> Result<bool, near_pty::PtyError> {
    stroke
        .pty_bytes(session.snapshot().application_cursor)
        .map_or(Ok(false), |bytes| session.write(&bytes).map(|()| true))
}

impl FarWorkspace {
    fn terminal_keybar_entries(
        &self,
        keymap: &Keymap,
    ) -> Vec<(String, &'static str, CommandInvocation)> {
        let contexts = self.active_contexts();
        let mut entries = Vec::new();
        for (command, label) in [
            ("near.terminal.new", "New"),
            ("near.terminal.previous", "Prev"),
            ("near.terminal.next", "Next"),
            ("near.workspace.focus-peer", "Peer"),
            ("near.terminal.open", "Zoom"),
            ("near.screen.list", "Screens"),
        ] {
            if command == "near.workspace.focus-peer" && self.terminal_pane().is_none() {
                continue;
            }
            let command_id = CommandId::from(command);
            let Some(binding) = keymap
                .bindings_for_command(&contexts, &command_id)
                .into_iter()
                .filter(|binding| binding.sequence.len() == 1)
                .min_by_key(|binding| binding.origin.ordinal)
            else {
                continue;
            };
            entries.push((
                compact_terminal_key(&format_key_sequence(&binding.sequence)),
                label,
                binding.invocation.clone(),
            ));
        }
        entries
    }

    pub(super) fn render_terminal_workspace_keybar(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        keymap: &Keymap,
        roles: &mut RoleBuffer,
    ) -> bool {
        if self.terminal_is_full_screen() && self.status.starts_with("No peer pane is visible") {
            roles.fill(area, "status.warning");
            frame.render_widget(
                Paragraph::new(self.status.as_str()).style(theme.style("status.warning")),
                area,
            );
            return true;
        }
        let entries = self.terminal_keybar_entries(keymap);
        if entries.is_empty() || area.width == 0 || area.height == 0 {
            return false;
        }
        let mut spans = Vec::new();
        let mut column = area.x;
        for (key, label, _) in entries {
            let key = format!("{key} ");
            let label = format!("{label}  ");
            spans.push(Span::styled(
                key.clone(),
                theme.style("keybar.key").add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(label.clone(), theme.style("keybar.label")));
            let key_width = u16::try_from(key.chars().count()).unwrap_or(u16::MAX);
            let label_width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
            roles.fill(
                Rect::new(
                    column,
                    area.y,
                    key_width.min(area.right().saturating_sub(column)),
                    1,
                ),
                "keybar.key",
            );
            column = column.saturating_add(key_width);
            roles.fill(
                Rect::new(
                    column,
                    area.y,
                    label_width.min(area.right().saturating_sub(column)),
                    1,
                ),
                "keybar.label",
            );
            column = column.saturating_add(label_width);
            if column >= area.right() {
                break;
            }
        }
        if column < area.right() {
            roles.fill(
                Rect::new(column, area.y, area.right() - column, 1),
                "keybar.label",
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(theme.style("keybar.label")),
            area,
        );
        true
    }

    pub(super) fn terminal_keybar_invocation_at(
        &self,
        column: u16,
        keymap: &Keymap,
    ) -> Option<CommandInvocation> {
        if self.terminal_is_full_screen() && self.status.starts_with("No peer pane is visible") {
            return None;
        }
        let mut start = 0_u16;
        for (key, label, invocation) in self.terminal_keybar_entries(keymap) {
            let width =
                u16::try_from(format!("{key} {label}  ").chars().count()).unwrap_or(u16::MAX);
            if column >= start && column < start.saturating_add(width) {
                return Some(invocation);
            }
            start = start.saturating_add(width);
        }
        None
    }

    pub(super) fn terminal_is_full_screen(&self) -> bool {
        self.terminal_presentation.is_full_screen() && !self.terminals.is_empty()
    }

    pub(super) fn terminal_pane(&self) -> Option<FocusedPanel> {
        self.terminal_presentation.pane().map(|slot| match slot {
            PaneSlot::First => FocusedPanel::Left,
            PaneSlot::Second => FocusedPanel::Right,
        })
    }

    pub(super) fn terminal_owns_focus(&self) -> bool {
        self.terminal_is_full_screen() || self.terminal_pane() == Some(self.focused)
    }

    pub(super) fn active_terminal_surface(&self) -> Option<&dyn Surface> {
        self.terminals
            .active()
            .map(|entry| entry.value().surface.as_ref())
    }

    pub(super) fn active_terminal_surface_mut(&mut self) -> Option<&mut dyn Surface> {
        let entry = self.terminals.active_mut()?;
        Some(entry.value_mut().surface.as_mut())
    }

    pub(super) fn terminal_process_description(&self, id: TabId) -> String {
        let Some(terminal) = self
            .terminals
            .entries()
            .iter()
            .find(|entry| entry.id() == id)
        else {
            return "terminal unavailable".to_owned();
        };
        #[cfg(feature = "embedded-pty")]
        if let Some(session) = &terminal.value().session {
            let snapshot = session.snapshot();
            let shell = snapshot
                .shell_profile
                .as_ref()
                .and_then(|profile| profile.program.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("shell");
            let process = session
                .foreground_process_label()
                .unwrap_or_else(|| shell.to_owned());
            return snapshot.exit_code.map_or_else(
                || format!("{process} • running"),
                |code| format!("{process} • exited {code}"),
            );
        }
        #[cfg(not(feature = "embedded-pty"))]
        let _ = terminal;
        "retained terminal".to_owned()
    }

    #[cfg(feature = "embedded-pty")]
    pub(super) fn active_terminal_session(&self) -> Option<EmbeddedTerminalSession> {
        self.terminals
            .active()
            .and_then(|entry| entry.value().session.clone())
    }

    #[cfg(feature = "embedded-pty")]
    pub(super) fn install_terminal_tab(
        &mut self,
        title: impl Into<String>,
        surface: Box<dyn Surface>,
        #[cfg(feature = "embedded-pty")] session: Option<EmbeddedTerminalSession>,
    ) -> TabId {
        self.terminals.insert(
            title,
            TerminalTab {
                surface,
                #[cfg(feature = "embedded-pty")]
                session,
            },
        )
    }

    #[cfg(test)]
    pub(super) fn install_test_terminal(
        &mut self,
        title: impl Into<String>,
        surface: Box<dyn Surface>,
    ) -> TabId {
        self.terminals.insert(
            title,
            TerminalTab {
                surface,
                #[cfg(feature = "embedded-pty")]
                session: None,
            },
        )
    }

    pub(super) fn render_terminal_view(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        focused: bool,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) {
        let Some(surface) = self.active_terminal_surface() else {
            return;
        };
        if self.terminals.len() <= 1 || area.height < 3 {
            Self::render_surface(frame, area, surface, focused, theme, roles);
            return;
        }
        let tabs = Rect::new(area.x, area.y, area.width, 1);
        let terminal = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        );
        let mut spans = Vec::new();
        let mut column = tabs.x;
        for entry in self.terminals.entries() {
            let active = self.terminals.active_id() == Some(entry.id());
            let label = if active {
                format!(" [{}:{}] ", entry.id().get(), entry.title())
            } else {
                format!(" {}:{} ", entry.id().get(), entry.title())
            };
            let role = if active { "lookup.bar" } else { "keybar.label" };
            let style = if active {
                theme.style(role).add_modifier(Modifier::BOLD)
            } else {
                theme.style(role)
            };
            spans.push(Span::styled(label.clone(), style));
            let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
            roles.fill(
                Rect::new(
                    column,
                    tabs.y,
                    width.min(tabs.right().saturating_sub(column)),
                    1,
                ),
                role,
            );
            column = column.saturating_add(width);
        }
        if column < tabs.right() {
            roles.fill(
                Rect::new(column, tabs.y, tabs.right() - column, 1),
                "keybar.label",
            );
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), tabs);
        Self::render_surface(frame, terminal, surface, focused, theme, roles);
    }

    pub(super) fn place_terminal_in_pane(&mut self, slot: PaneSlot) {
        if self.terminals.is_empty() {
            #[cfg(feature = "embedded-pty")]
            if let Err(error) = self.ensure_embedded_terminal() {
                self.status = format!("Cannot create terminal tab: {error}");
                return;
            }
        }
        if self.terminals.is_empty() {
            "No terminal tab is available".clone_into(&mut self.status);
            return;
        }
        self.persist_active_editor_position();
        self.active_editor = None;
        self.overlay = None;
        self.suspended_overlay = None;
        self.terminal_presentation.place(slot);
        self.focused = match slot {
            PaneSlot::First => FocusedPanel::Left,
            PaneSlot::Second => FocusedPanel::Right,
        };
        self.status = format!(
            "Terminal {} placed in {} pane",
            self.terminals
                .active()
                .map_or("tab", |terminal| terminal.title()),
            if slot == PaneSlot::First {
                "left"
            } else {
                "right"
            }
        );
    }

    pub(super) fn cycle_terminal_tab(&mut self, direction: isize) {
        if self.terminals.cycle(direction).is_none() {
            "No terminal tabs".clone_into(&mut self.status);
            return;
        }
        self.command_line.clear();
        let title = self
            .terminals
            .active()
            .map_or("Terminal", |terminal| terminal.title());
        self.status = format!("Terminal tab: {title}");
    }

    pub(super) fn select_terminal_tab(&mut self, invocation: &CommandInvocation) {
        let selected = invocation
            .arguments
            .get("tab")
            .and_then(CommandValue::as_i64)
            .and_then(|id| u64::try_from(id).ok())
            .map(TabId::from_raw)
            .is_some_and(|id| self.terminals.select(id));
        if selected {
            self.overlay = None;
            let title = self
                .terminals
                .active()
                .map_or("Terminal", |terminal| terminal.title());
            self.status = format!("Terminal tab: {title}");
        } else {
            "Terminal tab is unavailable".clone_into(&mut self.status);
        }
    }

    pub(super) fn show_terminal_workspace_menu(&mut self) {
        let mut items = vec![
            self.menu_item(
                "&New terminal tab",
                "Start another persistent native shell",
                "near.terminal.new",
            ),
            self.menu_item(
                "Next &tab",
                "Activate the next retained terminal",
                "near.terminal.next",
            ),
            self.menu_item(
                "&Previous tab",
                "Activate the previous retained terminal",
                "near.terminal.previous",
            ),
            self.menu_item(
                "Place in &left pane",
                "Replace the left panel view with the active terminal",
                "near.terminal.place-left",
            ),
            self.menu_item(
                "Place in &right pane",
                "Replace the right panel view with the active terminal",
                "near.terminal.place-right",
            ),
            self.menu_item(
                "&Hide terminal pane",
                "Restore the ordinary dual-panel composition",
                "near.terminal.hide",
            ),
            self.menu_item(
                "&Zoom or restore",
                "Toggle the active terminal between pane and full screen",
                "near.terminal.open",
            ),
            self.menu_item(
                "&Close active tab",
                "Apply the shell close policy to the active terminal",
                "near.terminal.close",
            ),
        ];
        items.extend(self.terminals.entries().iter().map(|terminal| {
            let id = terminal.id().get();
            MenuItem {
                label: if id < 10 {
                    format!("&{id} {}", terminal.title())
                } else {
                    format!("Tab {id}: {}", terminal.title())
                },
                description: if self.terminals.active_id() == Some(terminal.id()) {
                    "active terminal tab".to_owned()
                } else {
                    "retained terminal tab".to_owned()
                },
                command: CommandInvocation {
                    id: "near.terminal.select".into(),
                    arguments: BTreeMap::from([(
                        "tab".to_owned(),
                        CommandValue::Integer(i64::try_from(id).unwrap_or(i64::MAX)),
                    )]),
                },
                enabled: true,
            }
        }));
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.terminal-tabs",
            "Terminal Tabs",
            items,
        )));
    }

    pub(super) fn create_terminal_tab(&mut self) {
        #[cfg(feature = "embedded-pty")]
        if self.embedded_pty_enabled {
            match self.spawn_embedded_terminal_tab() {
                Ok(_) => {
                    self.overlay = None;
                    if self.terminal_presentation == ZoomablePanePresentation::Base {
                        self.terminal_presentation =
                            ZoomablePanePresentation::FullScreen { restore: None };
                    }
                    let title = self
                        .terminals
                        .active()
                        .map_or("Terminal", |terminal| terminal.title());
                    self.status = format!("Created terminal tab: {title}");
                }
                Err(error) => self.status = format!("Cannot create terminal tab: {error}"),
            }
            return;
        }
        "Embedded PTY support is disabled".clone_into(&mut self.status);
    }

    pub(super) fn command_dock_rows(&self) -> u16 {
        if self.filename_lookup.is_some() || self.terminal_pane().is_some() {
            return 0;
        }
        #[cfg(feature = "embedded-pty")]
        if self.active_terminal_session().is_some() {
            return 3;
        }
        1
    }

    pub(super) fn render_shell_dock(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) -> bool {
        #[cfg(feature = "embedded-pty")]
        if self.filename_lookup.is_none()
            && self.pending_sequence.is_empty()
            && self.terminal_pane().is_none()
            && let Some(session) = self.active_terminal_session()
        {
            let (width, height) = self.viewport.get();
            let dock = super::EmbeddedTerminalDockSurface::new(
                "near-fm.shell-dock",
                session,
                near_pty::TerminalSize::new(height, width),
            );
            Self::render_surface(frame, area, &dock, true, theme, roles);
            return true;
        }
        #[cfg(not(feature = "embedded-pty"))]
        let _ = (frame, area, theme, roles);
        false
    }

    #[cfg(feature = "embedded-pty")]
    pub(super) fn handle_native_shell_dock_key(&mut self, stroke: &KeyStroke) -> Option<bool> {
        if !self.embedded_pty_enabled {
            return None;
        }
        if self.terminal_pane().is_some() || self.terminal_is_full_screen() {
            return None;
        }
        let session = self.active_terminal_session()?;
        if matches!(stroke.key, Key::Character('o' | 'O'))
            && stroke.modifiers.control
            && !stroke.modifiers.alt
            && !stroke.modifiers.super_key
        {
            return Some(false);
        }
        if stroke.key == Key::Enter && stroke.modifiers == Modifiers::default() {
            let command = self.command_line.commit().unwrap_or_default();
            if command.is_empty() {
                return Some(true);
            }
            self.persist_command_history();
            if self.dispatch_command_prefix(&command) {
                let _ = session.interrupt();
                return Some(true);
            }
            match write_key(&session, stroke) {
                Ok(true) => {
                    self.activate_terminal_screen();
                    self.status = format!("Terminal: {command}");
                }
                Ok(false) => {}
                Err(error) => self.status = format!("Terminal input failed: {error}"),
            }
            return Some(true);
        }
        match write_key(&session, stroke) {
            Ok(true) => {
                if let Some(character) = stroke.text_character() {
                    self.command_line.insert(&character.to_string());
                } else if stroke.key == Key::Backspace {
                    self.command_line.backspace();
                } else if matches!(stroke.key, Key::Character('c' | 'C'))
                    && stroke.modifiers.control
                {
                    self.command_line.clear();
                }
                Some(true)
            }
            Ok(false) => None,
            Err(error) => {
                self.status = format!("Terminal input failed: {error}");
                Some(true)
            }
        }
    }

    pub(super) fn insert_command_text(&mut self, text: &str) {
        #[cfg(feature = "embedded-pty")]
        if self.embedded_pty_enabled {
            match self.ensure_embedded_terminal() {
                Ok(session) => match session.write(text.as_bytes()) {
                    Ok(()) => {
                        self.command_line.insert(text);
                        return;
                    }
                    Err(error) => self.status = format!("Terminal input failed: {error}"),
                },
                Err(error) => self.status = format!("Cannot start terminal: {error}"),
            }
        }
        self.command_line.insert(text);
    }

    pub(super) fn paste_command_text(&mut self, text: &str) {
        #[cfg(feature = "embedded-pty")]
        if self.embedded_pty_enabled {
            match self.ensure_embedded_terminal() {
                Ok(session) => match session.paste(text) {
                    Ok(()) => {
                        self.command_line.insert(text);
                        return;
                    }
                    Err(error) => self.status = format!("Terminal paste failed: {error}"),
                },
                Err(error) => self.status = format!("Cannot start terminal: {error}"),
            }
        }
        self.command_line.insert(text);
    }

    pub(super) fn replace_command_text(&mut self, text: &str) {
        #[cfg(feature = "embedded-pty")]
        if self.embedded_pty_enabled {
            match self.ensure_embedded_terminal() {
                Ok(session) => match session
                    .write(&[0x15])
                    .and_then(|()| session.write(text.as_bytes()))
                {
                    Ok(()) => {
                        self.command_line.set_buffer(text);
                        return;
                    }
                    Err(error) => self.status = format!("Terminal input failed: {error}"),
                },
                Err(error) => self.status = format!("Cannot start terminal: {error}"),
            }
        }
        self.command_line.set_buffer(text);
    }

    pub fn initialize_shell_dock(&mut self) {
        #[cfg(feature = "embedded-pty")]
        if self.embedded_pty_enabled
            && let Err(error) = self.ensure_embedded_terminal()
        {
            self.status = format!("Cannot initialize native shell: {error}");
        }
    }

    pub(super) fn toggle_terminal_screen(&mut self) {
        if self.terminals.is_empty() {
            #[cfg(feature = "embedded-pty")]
            if self.embedded_pty_enabled
                && let Err(error) = self.ensure_embedded_terminal()
            {
                self.overlay = Some(Overlay::Message {
                    title: "Embedded Terminal".to_owned(),
                    body: error,
                });
                return;
            }
        }
        if self.terminals.is_empty() {
            self.overlay = Some(Overlay::Message {
                title: "Embedded Terminal".to_owned(),
                body: "Embedded PTY support is disabled. External handlers remain available."
                    .to_owned(),
            });
            return;
        }
        if self.terminal_is_full_screen() {
            self.terminal_presentation.toggle_zoom();
            if self.terminal_presentation == ZoomablePanePresentation::Base {
                self.overlay = self.suspended_overlay.take();
            }
            "Restored previous terminal layout".clone_into(&mut self.status);
        } else {
            if self.terminal_presentation == ZoomablePanePresentation::Base {
                self.suspended_overlay = self.overlay.take();
            }
            self.quick_view_interactive = false;
            self.terminal_presentation.toggle_zoom();
            "Zoomed terminal".clone_into(&mut self.status);
        }
    }

    pub(super) fn close_terminal_screen(&mut self) {
        #[cfg(feature = "embedded-pty")]
        if let Some(session) = self.active_terminal_session() {
            let exited = session.has_exited();
            let policy = session.close_policy();
            if !exited && policy.keeps_process_on_close() {
                self.hide_terminal_workspace();
                "Shell kept running in a hidden terminal tab".clone_into(&mut self.status);
                return;
            }
            if !exited && policy.warns_before_terminating() {
                self.overlay = Some(Overlay::Menu(MenuSurface::new(
                    "near-fm.shell-close",
                    "Close Running Shell",
                    vec![
                        self.menu_item(
                            "&Terminate shell and close",
                            "Stop the child process and remove its retained screen",
                            "near.terminal.close-confirmed",
                        ),
                        self.menu_item(
                            "&Keep running and hide",
                            "Return to the previous screen without stopping the child",
                            "near.terminal.hide",
                        ),
                        self.menu_item(
                            "&Cancel",
                            "Return to the running shell",
                            "near.overlay.cancel",
                        ),
                    ],
                )));
                return;
            }
            if !exited {
                self.confirm_close_terminal_screen();
                return;
            }
        }
        self.discard_terminal_screen("Closed user screen");
    }

    pub(super) fn confirm_close_terminal_screen(&mut self) {
        #[cfg(feature = "embedded-pty")]
        if let Some(session) = self.active_terminal_session()
            && !session.has_exited()
            && let Err(error) = session.terminate()
        {
            self.status = format!("Cannot terminate shell: {error}");
            return;
        }
        self.discard_terminal_screen("Closed user screen");
    }

    fn discard_terminal_screen(&mut self, message: &str) {
        let was_full_screen = self.terminal_is_full_screen();
        self.terminals.remove_active();
        if self.terminals.is_empty() {
            self.terminal_presentation.hide();
        }
        if was_full_screen && self.terminals.is_empty() {
            self.overlay = self.suspended_overlay.take();
        }
        if self.terminals.is_empty() {
            self.suspended_overlay = None;
        }
        message.clone_into(&mut self.status);
    }

    pub(super) fn hide_terminal_workspace(&mut self) {
        let was_full_screen = self.terminal_is_full_screen();
        self.terminal_presentation.hide();
        if was_full_screen {
            self.overlay = self.suspended_overlay.take();
        }
        self.suspended_overlay = None;
    }

    #[cfg(feature = "embedded-pty")]
    pub(super) fn reconcile_embedded_terminal_exit(&mut self) -> bool {
        let Some(session) = self.active_terminal_session() else {
            return false;
        };
        if !session.has_exited() {
            return false;
        }
        let policy = session.close_policy();
        if policy.closes_on_exit() {
            self.discard_terminal_screen("Shell exited and the user screen closed");
            return true;
        }
        if policy.warns_before_terminating() && !self.status.starts_with("Shell exited;") {
            self.status = "Shell exited; output retained until the user screen closes".to_owned();
            self.overlay = Some(Overlay::Menu(MenuSurface::new(
                "near-fm.shell-exited",
                "Shell Exited — Output Retained",
                vec![
                    self.menu_item(
                        "&Close user screen",
                        "Remove the completed shell transcript",
                        "near.terminal.close-confirmed",
                    ),
                    self.menu_item(
                        "&Keep output open",
                        "Return to the completed shell transcript",
                        "near.overlay.cancel",
                    ),
                ],
            )));
            return true;
        }
        false
    }

    #[cfg(feature = "embedded-pty")]
    pub(super) fn ensure_embedded_terminal(&mut self) -> Result<EmbeddedTerminalSession, String> {
        if let Some(session) = self.active_terminal_session()
            && !session.has_exited()
        {
            return Ok(session);
        }
        self.spawn_embedded_terminal_tab()
    }

    #[cfg(feature = "embedded-pty")]
    pub(super) fn spawn_embedded_terminal_tab(
        &mut self,
    ) -> Result<EmbeddedTerminalSession, String> {
        let current_directory = if let Some(resolver) = &self.command_line_arguments {
            resolver
                .native_working_directory(self.focused_panel().location())?
                .ok_or_else(|| {
                    format!(
                        "The focused provider location cannot host a local interactive shell: {}",
                        self.focused_panel().location().as_str()
                    )
                })?
        } else {
            std::env::current_dir().map_err(|error| error.to_string())?
        };
        let session = EmbeddedTerminalSession::spawn_profile(
            &self.settings.shell,
            Some(current_directory.as_path()),
        )
        .map_err(|error| error.to_string())?;
        let wake = self.tasks.wake_handle();
        let output_wake = wake.clone();
        session.set_output_wake(move || output_wake.wake());
        session.set_exit_wake(move || wake.wake());
        let ordinal = self.terminals.len() + 1;
        let title = format!("Shell {ordinal}");
        self.install_terminal_tab(
            title.clone(),
            Box::new(EmbeddedTerminalSurface::from_session(
                format!("near-fm.embedded-terminal.{ordinal}"),
                title,
                session.clone(),
            )),
            Some(session.clone()),
        );
        Ok(session)
    }
}

fn compact_terminal_key(value: &str) -> String {
    let value = value
        .replace("Ctrl+Alt+", "C-A-")
        .replace("Ctrl+", "C-")
        .replace("PageDown", "PgDn")
        .replace("PageUp", "PgUp");
    if let Some((prefix, character)) = value.rsplit_once('-')
        && character.chars().count() == 1
    {
        return format!("{prefix}-{}", character.to_ascii_uppercase());
    }
    value
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{TerminalSurface, parse_key_stroke};
    use near_terminal::TerminalEvent;

    use super::*;

    #[test]
    fn commands_terminal_accelerator_opens_terminal_tabs_not_tasks() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(include_str!("../../../specs/keymap.toml")).unwrap();
        workspace.dispatch(&CommandInvocation::new("near.menu.commands"));
        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(parse_key_stroke("t").unwrap()),
        );
        let Some(Overlay::Menu(menu)) = &workspace.overlay else {
            panic!("terminal accelerator should open a menu");
        };
        assert_eq!(menu.title(), "Terminal Tabs");
    }

    #[test]
    fn terminal_workspace_menu_has_unique_accelerators() {
        let mut workspace = FarWorkspace::demo();
        workspace.install_test_terminal(
            "Shell One",
            Box::new(TerminalSurface::new("test.shell-one", "Shell One", 100)),
        );
        workspace.install_test_terminal(
            "Shell Two",
            Box::new(TerminalSurface::new("test.shell-two", "Shell Two", 100)),
        );
        workspace.show_terminal_workspace_menu();
        let Some(Overlay::Menu(menu)) = &workspace.overlay else {
            panic!("terminal workspace menu should be open");
        };
        let mut accelerators = BTreeSet::new();
        for item in menu.items() {
            if let Some(accelerator) = item.label.split_once('&').and_then(|(_, suffix)| {
                suffix
                    .chars()
                    .next()
                    .map(|character| character.to_ascii_lowercase())
            }) {
                assert!(accelerators.insert(accelerator));
            }
        }
    }
}
