#![allow(clippy::cast_possible_wrap, clippy::wildcard_imports)]

use super::*;

impl FarWorkspace {
    pub(super) fn effective_settings_surface_for(&self, category: Option<&str>) -> SettingsSurface {
        macro_rules! setting {
            ($id:expr, $document:expr, $category:expr, $title:expr, $description:expr, $value:expr, $default:expr, $scope:ident) => {
                Self::setting_entry(
                    self,
                    $id,
                    $document,
                    $category,
                    $title,
                    $description,
                    $value,
                    $default,
                    SettingApplyScope::$scope,
                )
            };
        }
        let mut entries = vec![
            setting!(
                "interface.show_status_line",
                "interface.toml",
                "System",
                "Show status line",
                "Reserve the operation and selection status row below the panels",
                SettingValue::Boolean(self.settings.interface.show_status_line),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "interface.show_keybar",
                "interface.toml",
                "Interface",
                "Show function keybar",
                "Reserve the context-sensitive function-key row",
                SettingValue::Boolean(self.settings.interface.show_keybar),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "interface.tree_indent_width",
                "interface.toml",
                "Tree",
                "Tree indentation",
                "Terminal cells per tree depth from 1 through 8",
                SettingValue::Integer(i64::from(self.settings.interface.tree_indent_width)),
                SettingValue::Integer(2),
                Live
            ),
            setting!(
                "interface.menu_wrap_navigation",
                "interface.toml",
                "Menu",
                "Wrap menu navigation",
                "Move from the first menu item to the last and back",
                SettingValue::Boolean(self.settings.interface.menu_wrap_navigation),
                SettingValue::Boolean(false),
                Live
            ),
            setting!(
                "interface.dialog_wrap_focus",
                "interface.toml",
                "Dialog",
                "Wrap dialog focus",
                "Cycle focus from the last dialog field to the first",
                SettingValue::Boolean(self.settings.interface.dialog_wrap_focus),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "interface.command_line_completion",
                "interface.toml",
                "Completion",
                "Fallback command-line completion",
                "Complete from Near history and panel names only when the native PTY shell is unavailable",
                SettingValue::Boolean(self.settings.interface.command_line_completion),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "interface.startup_panel",
                "interface.toml",
                "Interface",
                "Startup panel",
                "Panel focused when a new Near process starts: left | right",
                SettingValue::String(
                    match self.settings.interface.startup_panel {
                        crate::StartupPanel::Left => "left",
                        crate::StartupPanel::Right => "right",
                    }
                    .to_owned()
                ),
                SettingValue::String("left".to_owned()),
                Restart
            ),
            setting!(
                "keymap.sequence_timeout_ms",
                "keymap.toml",
                "Interface",
                "Key sequence timeout",
                "Milliseconds to wait for a longer multi-key binding",
                SettingValue::Integer(
                    i64::try_from(self.settings.keymap.sequence_timeout.as_millis())
                        .unwrap_or(i64::MAX)
                ),
                SettingValue::Integer(700),
                Live
            ),
            setting!(
                "keymap.show_pending_sequence",
                "keymap.toml",
                "Interface",
                "Show pending key sequence",
                "Display incomplete multi-key bindings while waiting for continuation",
                SettingValue::Boolean(self.settings.keymap.show_pending_sequence),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "keymap.prefer_physical_keys",
                "keymap.toml",
                "Interface",
                "Prefer physical keys",
                "Rejected until the terminal backend exposes physical key identity",
                SettingValue::Boolean(self.settings.keymap.prefer_physical_keys),
                SettingValue::Boolean(false),
                Live
            ),
            setting!(
                "confirmations.reversible",
                "confirmations.toml",
                "Confirmation",
                "Preview reversible operations",
                "Show a preview before Trash and other reversible operations",
                SettingValue::Boolean(self.settings.confirmations.reversible_preview()),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "confirmations.confirmable",
                "confirmations.toml",
                "Confirmation",
                "Preview confirmable actions",
                "Show a preview before external and other confirmable actions",
                SettingValue::Boolean(self.settings.confirmations.confirmable_preview()),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "viewer.wrap",
                "viewer.toml",
                "Viewer",
                "Wrap text",
                "Wrap long lines in newly opened viewers",
                SettingValue::Boolean(self.settings.viewer.wrap),
                SettingValue::Boolean(false),
                NewSurface
            ),
            setting!(
                "viewer.hex",
                "viewer.toml",
                "Viewer",
                "Hexadecimal mode",
                "Open new viewers in hexadecimal mode",
                SettingValue::Boolean(self.settings.viewer.hex),
                SettingValue::Boolean(false),
                NewSurface
            ),
            setting!(
                "viewer.remember_per_resource",
                "viewer.toml",
                "Viewer",
                "Remember resource state",
                "Restore bookmarks, position, and display mode per resource",
                SettingValue::Boolean(self.settings.viewer.remember_per_resource),
                SettingValue::Boolean(true),
                Live
            ),
            setting!(
                "viewer.encoding",
                "viewer.toml",
                "Viewer",
                "Text encoding",
                "Default for new viewers: auto | utf8-lossy | utf16le | utf16be | latin1",
                SettingValue::String(self.settings.viewer.encoding.config_name().to_owned()),
                SettingValue::String("auto".to_owned()),
                NewSurface
            ),
            setting!(
                "viewer.open_policy",
                "viewer.toml",
                "Viewer",
                "Open policy",
                "Resource opening precedence: internal | external | association",
                SettingValue::String(open_policy_name(self.settings.viewer.open_policy).to_owned()),
                SettingValue::String("internal".to_owned()),
                NewSurface
            ),
            setting!(
                "editor.persistent_blocks",
                "editor.toml",
                "Editor",
                "Persistent blocks",
                "Keep selections active after editing commands",
                SettingValue::Boolean(self.settings.editor.persistent_blocks),
                SettingValue::Boolean(false),
                NewSurface
            ),
            setting!(
                "editor.expand_tabs",
                "editor.toml",
                "Editor",
                "Expand tabs",
                "Insert spaces instead of tab characters",
                SettingValue::Boolean(self.settings.editor.expand_tabs),
                SettingValue::Boolean(false),
                NewSurface
            ),
            setting!(
                "editor.tab_size",
                "editor.toml",
                "Editor",
                "Tab size",
                "Display and insertion width from 1 through 16",
                SettingValue::Integer(i64::from(self.settings.editor.tab_size)),
                SettingValue::Integer(4),
                NewSurface
            ),
            setting!(
                "editor.open_policy",
                "editor.toml",
                "Editor",
                "Open policy",
                "Resource opening precedence: internal | external | association",
                SettingValue::String(open_policy_name(self.settings.editor.open_policy).to_owned()),
                SettingValue::String("internal".to_owned()),
                NewSurface
            ),
            setting!(
                "panel-modes.left",
                "panel-modes.toml",
                "Panel",
                "Left panel default mode",
                "View mode applied to the left panel: brief | medium | full | metadata",
                SettingValue::String(self.settings.panel_modes.left_default().to_owned()),
                SettingValue::String("medium".to_owned()),
                Live
            ),
            setting!(
                "panel-modes.right",
                "panel-modes.toml",
                "Panel",
                "Right panel default mode",
                "View mode applied to the right panel: brief | medium | full | metadata",
                SettingValue::String(self.settings.panel_modes.right_default().to_owned()),
                SettingValue::String("medium".to_owned()),
                Live
            ),
            setting!(
                "history.command_max_unlocked",
                "history.toml",
                "Command-line",
                "Command history limit",
                "Maximum unlocked command entries",
                SettingValue::Integer(self.settings.history.command_max_unlocked as i64),
                SettingValue::Integer(200),
                Live
            ),
            setting!(
                "history.folder_max_unlocked",
                "history.toml",
                "Panel",
                "Folder history limit",
                "Maximum unlocked folder entries",
                SettingValue::Integer(self.settings.history.folder_max_unlocked as i64),
                SettingValue::Integer(200),
                Live
            ),
            setting!(
                "history.resource_max_unlocked",
                "history.toml",
                "Viewer and Editor",
                "Resource history limit",
                "Maximum unlocked viewed and edited entries",
                SettingValue::Integer(self.settings.history.resource_max_unlocked as i64),
                SettingValue::Integer(100),
                Live
            ),
        ];
        entries.extend(
            [
                (
                    "viewer.remember_position",
                    "Remember position",
                    "Persist byte offset and navigation history per resource",
                    self.settings.viewer.remember_position,
                ),
                (
                    "viewer.remember_bookmarks",
                    "Remember bookmarks",
                    "Persist numbered provider-resource bookmarks",
                    self.settings.viewer.remember_bookmarks,
                ),
                (
                    "viewer.remember_encoding",
                    "Remember encoding",
                    "Persist resolved text encoding per resource",
                    self.settings.viewer.remember_encoding,
                ),
                (
                    "viewer.remember_view_mode",
                    "Remember view mode",
                    "Persist wrap and text or hexadecimal mode per resource",
                    self.settings.viewer.remember_view_mode,
                ),
                (
                    "viewer.detect_binary",
                    "Detect binary",
                    "Open NUL or control-heavy resources in hexadecimal mode",
                    self.settings.viewer.detect_binary,
                ),
            ]
            .map(|(id, title, description, value)| {
                Self::setting_entry(
                    self,
                    id,
                    "viewer.toml",
                    "Viewer",
                    title,
                    description,
                    SettingValue::Boolean(value),
                    SettingValue::Boolean(true),
                    SettingApplyScope::NewSurface,
                )
            }),
        );
        #[cfg(feature = "embedded-pty")]
        let entries = {
            let mut entries = entries;
            entries.extend([
                setting!(
                    "shell.program",
                    "shell.toml",
                    "Command-line",
                    "Shell program",
                    "Blank uses the platform account shell",
                    SettingValue::String(
                        self.settings
                            .shell
                            .program
                            .as_ref()
                            .map_or_else(String::new, |path| path.display().to_string())
                    ),
                    SettingValue::String(String::new()),
                    NewSurface
                ),
                setting!(
                    "shell.mode",
                    "shell.toml",
                    "Command-line",
                    "Shell mode",
                    "platform-default | login | interactive | clean",
                    SettingValue::String(self.settings.shell.mode.to_string()),
                    SettingValue::String("platform-default".to_owned()),
                    NewSurface
                ),
                setting!(
                    "shell.startup_command",
                    "shell.toml",
                    "Command-line",
                    "Startup command",
                    "Optional command executed when a new embedded shell opens",
                    SettingValue::String(
                        self.settings
                            .shell
                            .startup_command
                            .clone()
                            .unwrap_or_default()
                    ),
                    SettingValue::String(String::new()),
                    NewSurface
                ),
                setting!(
                    "shell.arguments",
                    "shell.toml",
                    "Command-line",
                    "Additional arguments",
                    "Comma-separated arguments appended after mode arguments",
                    SettingValue::Strings(self.settings.shell.arguments.clone()),
                    SettingValue::Strings(Vec::new()),
                    NewSurface
                ),
                setting!(
                    "shell.close_policy",
                    "shell.toml",
                    "Command-line",
                    "Close policy",
                    "warn | keep-open | close",
                    SettingValue::String(self.settings.shell.close_policy.to_string()),
                    SettingValue::String("warn".to_owned()),
                    NewSurface
                ),
                setting!(
                    "shell.inherit_environment",
                    "shell.toml",
                    "Command-line",
                    "Inherit environment",
                    "Pass Near's environment into newly opened shells",
                    SettingValue::Boolean(self.settings.shell.inherit_environment),
                    SettingValue::Boolean(true),
                    NewSurface
                ),
            ]);
            entries
        };
        let entries = category.map_or(entries.clone(), |category| {
            entries
                .into_iter()
                .filter(|entry| entry.descriptor.category.eq_ignore_ascii_case(category))
                .collect()
        });
        SettingsSurface::new(
            "near-fm.settings",
            category.map_or_else(
                || "Typed Settings".to_owned(),
                |category| format!("{category} Settings"),
            ),
            entries,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn setting_entry(
        &self,
        id: &str,
        document: &str,
        category: &str,
        title: &str,
        description: &str,
        value: SettingValue,
        default_value: SettingValue,
        apply_scope: SettingApplyScope,
    ) -> SettingSurfaceEntry {
        SettingSurfaceEntry {
            descriptor: SettingDescriptor {
                id: id.to_owned(),
                document: document.to_owned(),
                path: id.to_owned(),
                category: category.to_owned(),
                title: title.to_owned(),
                description: description.to_owned(),
                advanced: id == "keymap.prefer_physical_keys",
                value_kind: value.kind(),
                default_value,
                apply_scope,
                apply_order: 0,
                availability: SettingPlatformAvailability::All,
            },
            state: SettingState {
                value,
                provenance: self.settings.provenance_for(id, document),
            },
        }
    }

    pub(super) fn show_settings_value_dialog(&mut self, invocation: &CommandInvocation) {
        let Some(id) = invocation
            .arguments
            .get("id")
            .and_then(CommandValue::as_str)
        else {
            "Settings editor is missing an identifier".clone_into(&mut self.status);
            return;
        };
        let value = invocation
            .arguments
            .get("value")
            .and_then(CommandValue::as_str)
            .unwrap_or_default();
        let guidance = setting_value_guidance(id);
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            "near-fm.setting-value",
            guidance.map_or_else(
                || format!("Edit {id}"),
                |guidance| format!("Edit {id} — {guidance}"),
            ),
            vec![
                DialogField {
                    id: "id".to_owned(),
                    label: "Setting".to_owned(),
                    value: id.to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "value".to_owned(),
                    label: "Value".to_owned(),
                    value: value.to_owned(),
                    required: false,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: CommandId::from("near.settings.apply-candidate"),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    pub(super) fn apply_settings_candidate(&mut self, invocation: &CommandInvocation) {
        let settings_category = self.active_settings_category.clone();
        let Some(id) = invocation
            .arguments
            .get("id")
            .and_then(CommandValue::as_str)
        else {
            "Settings candidate is missing an identifier".clone_into(&mut self.status);
            return;
        };
        let Some(value) = invocation
            .arguments
            .get("value")
            .and_then(CommandValue::as_str)
        else {
            "Settings candidate is missing a value".clone_into(&mut self.status);
            return;
        };
        let previous_viewer = self.settings.viewer;
        let previous_editor = self.settings.editor;
        let previous_history = self.settings.history;
        let previous_confirmations = self.settings.confirmations.clone();
        let previous_panel_modes = self.settings.panel_modes.clone();
        let previous_keymap = self.settings.keymap;
        let previous_interface = self.settings.interface;
        let previous_keymap_source = self.settings.keymap_source.clone();
        let previous_pending_keymap = self.settings.pending_keymap_source.clone();
        #[cfg(feature = "embedded-pty")]
        let previous_shell = self.settings.shell.clone();
        let mut result = match id {
            "interface.show_status_line" => parse_setting_bool(value)
                .map(|value| self.settings.interface.show_status_line = value),
            "interface.show_keybar" => {
                parse_setting_bool(value).map(|value| self.settings.interface.show_keybar = value)
            }
            "interface.tree_indent_width" => value
                .parse::<u8>()
                .map_err(|error| error.to_string())
                .and_then(|value| {
                    if (1..=8).contains(&value) {
                        self.settings.interface.tree_indent_width = value;
                        Ok(())
                    } else {
                        Err("tree indentation must be between 1 and 8".to_owned())
                    }
                }),
            "interface.menu_wrap_navigation" => parse_setting_bool(value)
                .map(|value| self.settings.interface.menu_wrap_navigation = value),
            "interface.dialog_wrap_focus" => parse_setting_bool(value)
                .map(|value| self.settings.interface.dialog_wrap_focus = value),
            "interface.command_line_completion" => parse_setting_bool(value)
                .map(|value| self.settings.interface.command_line_completion = value),
            "interface.startup_panel" => match value.trim().to_ascii_lowercase().as_str() {
                "left" => {
                    self.settings.interface.startup_panel = crate::StartupPanel::Left;
                    Ok(())
                }
                "right" => {
                    self.settings.interface.startup_panel = crate::StartupPanel::Right;
                    Ok(())
                }
                _ => Err("startup panel must be left or right".to_owned()),
            },
            "keymap.sequence_timeout_ms" => value
                .parse::<u64>()
                .map_err(|error| error.to_string())
                .and_then(|milliseconds| {
                    if (50..=10_000).contains(&milliseconds) {
                        self.settings.keymap.sequence_timeout = Duration::from_millis(milliseconds);
                        Ok(())
                    } else {
                        Err("key sequence timeout must be between 50 and 10000 ms".to_owned())
                    }
                }),
            "keymap.show_pending_sequence" => parse_setting_bool(value)
                .map(|value| self.settings.keymap.show_pending_sequence = value),
            "keymap.prefer_physical_keys" => parse_setting_bool(value).and_then(|value| {
                if value {
                    Err(
                        "physical key identity is unavailable in the terminal event model"
                            .to_owned(),
                    )
                } else {
                    self.settings.keymap.prefer_physical_keys = false;
                    Ok(())
                }
            }),
            "confirmations.reversible" => parse_setting_bool(value)
                .map(|value| self.settings.confirmations.set_reversible_preview(value)),
            "confirmations.confirmable" => parse_setting_bool(value)
                .map(|value| self.settings.confirmations.set_confirmable_preview(value)),
            "viewer.wrap" => {
                parse_setting_bool(value).map(|value| self.settings.viewer.wrap = value)
            }
            "viewer.hex" => parse_setting_bool(value).map(|value| self.settings.viewer.hex = value),
            "viewer.detect_binary" => {
                parse_setting_bool(value).map(|value| self.settings.viewer.detect_binary = value)
            }
            "viewer.remember_per_resource" => parse_setting_bool(value)
                .map(|value| self.settings.viewer.remember_per_resource = value),
            "viewer.remember_position" => parse_setting_bool(value)
                .map(|value| self.settings.viewer.remember_position = value),
            "viewer.remember_bookmarks" => parse_setting_bool(value)
                .map(|value| self.settings.viewer.remember_bookmarks = value),
            "viewer.remember_encoding" => parse_setting_bool(value)
                .map(|value| self.settings.viewer.remember_encoding = value),
            "viewer.remember_view_mode" => parse_setting_bool(value)
                .map(|value| self.settings.viewer.remember_view_mode = value),
            "viewer.encoding" => crate::ViewerEncoding::parse(value)
                .ok_or_else(|| "invalid viewer encoding".to_owned())
                .map(|value| self.settings.viewer.encoding = value),
            "viewer.open_policy" => {
                parse_open_policy(value).map(|value| self.settings.viewer.open_policy = value)
            }
            "editor.persistent_blocks" => parse_setting_bool(value).map(|value| {
                self.settings.editor.persistent_blocks = value;
            }),
            "editor.expand_tabs" => {
                parse_setting_bool(value).map(|value| self.settings.editor.expand_tabs = value)
            }
            "editor.tab_size" => value
                .parse::<u8>()
                .map_err(|error| error.to_string())
                .and_then(|value| {
                    if (1..=16).contains(&value) {
                        self.settings.editor.tab_size = value;
                        Ok(())
                    } else {
                        Err("tab size must be between 1 and 16".to_owned())
                    }
                }),
            "editor.open_policy" => {
                parse_open_policy(value).map(|value| self.settings.editor.open_policy = value)
            }
            "panel-modes.left" => self
                .settings
                .panel_modes
                .set_defaults(value, previous_panel_modes.right_default())
                .map_err(|error| error.to_string()),
            "panel-modes.right" => self
                .settings
                .panel_modes
                .set_defaults(previous_panel_modes.left_default(), value)
                .map_err(|error| error.to_string()),
            "history.command_max_unlocked" => parse_positive_usize(value)
                .map(|value| self.settings.history.command_max_unlocked = value),
            "history.folder_max_unlocked" => parse_positive_usize(value)
                .map(|value| self.settings.history.folder_max_unlocked = value),
            "history.resource_max_unlocked" => parse_positive_usize(value)
                .map(|value| self.settings.history.resource_max_unlocked = value),
            #[cfg(feature = "embedded-pty")]
            "shell.program" => {
                self.settings.shell.program = (!value.trim().is_empty()).then(|| value.into());
                Ok(())
            }
            #[cfg(feature = "embedded-pty")]
            "shell.mode" => value.parse().map(|value| self.settings.shell.mode = value),
            #[cfg(feature = "embedded-pty")]
            "shell.startup_command" => {
                self.settings.shell.startup_command =
                    (!value.trim().is_empty()).then(|| value.to_owned());
                Ok(())
            }
            #[cfg(feature = "embedded-pty")]
            "shell.arguments" => {
                self.settings.shell.arguments = value
                    .split(',')
                    .map(str::trim)
                    .filter(|argument| !argument.is_empty())
                    .map(str::to_owned)
                    .collect();
                Ok(())
            }
            #[cfg(feature = "embedded-pty")]
            "shell.close_policy" => value
                .parse()
                .map(|value| self.settings.shell.close_policy = value),
            #[cfg(feature = "embedded-pty")]
            "shell.inherit_environment" => parse_setting_bool(value)
                .map(|value| self.settings.shell.inherit_environment = value),
            _ => Err(format!("unknown runtime setting {id}")),
        };
        if result.is_ok() && id.starts_with("keymap.") {
            result = self
                .settings
                .keymap_source
                .as_deref()
                .ok_or_else(|| "keymap source is unavailable".to_owned())
                .and_then(|source| {
                    Keymap::rewrite_settings_toml(source, self.settings.keymap)
                        .map_err(|error| error.to_string())
                })
                .map(|source| {
                    self.settings.keymap_source = Some(source.clone());
                    self.settings.pending_keymap_source = Some(source);
                });
        }
        let document = format!("{}.toml", id.split('.').next().unwrap_or_default());
        let mut persisted = false;
        if result.is_ok() {
            match self.settings.persist(&document) {
                Ok(value) => persisted = value,
                Err(error) => result = Err(format!("cannot persist {document}: {error}")),
            }
        }
        if result.is_err() {
            self.settings.viewer = previous_viewer;
            self.settings.editor = previous_editor;
            self.settings.history = previous_history;
            self.settings.confirmations = previous_confirmations;
            self.settings.panel_modes = previous_panel_modes;
            self.settings.keymap = previous_keymap;
            self.settings.keymap_source = previous_keymap_source;
            self.settings.pending_keymap_source = previous_pending_keymap;
            self.settings.interface = previous_interface;
            #[cfg(feature = "embedded-pty")]
            {
                self.settings.shell = previous_shell;
            }
        }
        if result.is_ok() {
            if persisted {
                self.settings.record_persisted_origin(&document);
            }
            self.apply_history_settings();
            self.apply_panel_mode_defaults();
        }
        let succeeded = result.is_ok();
        self.status = result.map_or_else(
            |error| format!("Setting rejected: {error}"),
            |()| {
                format!(
                    "Applied {id}={value} {}",
                    if persisted {
                        "and persisted"
                    } else {
                        "for this session"
                    }
                )
            },
        );
        if succeeded {
            if matches!(
                self.overlay_history.last(),
                Some(Overlay::Surface(surface)) if surface.id().as_str() == "near-fm.settings"
            ) {
                self.overlay_history.pop();
            }
            self.overlay = Some(Overlay::Surface(Box::new(
                self.effective_settings_surface_for(settings_category.as_deref())
                    .with_message(self.status.clone()),
            )));
        }
    }

    pub(super) fn reload_settings(&mut self) {
        let category = self.active_settings_category.clone();
        self.status = match self.settings.reload() {
            Ok(true) => {
                self.apply_history_settings();
                self.apply_panel_mode_defaults();
                "Reloaded externally edited settings".to_owned()
            }
            Ok(false) => "Settings reload is unavailable for this embedder".to_owned(),
            Err(error) => format!("Settings reload rejected; last-valid values retained: {error}"),
        };
        self.overlay = Some(Overlay::Surface(Box::new(
            self.effective_settings_surface_for(category.as_deref())
                .with_message(self.status.clone()),
        )));
    }

    fn apply_history_settings(&mut self) {
        self.command_line
            .set_max_unlocked_history(self.settings.history.command_max_unlocked);
        self.folder_navigation.max_unlocked = self.settings.history.folder_max_unlocked;
        self.resource_history.max_unlocked = self.settings.history.resource_max_unlocked;
    }

    fn apply_panel_mode_defaults(&mut self) {
        let modes = &self.settings.panel_modes;
        let left = modes.mode(modes.left_default()).cloned();
        let right = modes.mode(modes.right_default()).cloned();
        if let Some(mode) = left {
            self.left.set_view_mode(mode);
        }
        if let Some(mode) = right {
            self.right.set_view_mode(mode);
        }
    }
}

fn setting_value_guidance(id: &str) -> Option<&'static str> {
    match id {
        "viewer.open_policy" | "editor.open_policy" => Some("internal | external | association"),
        "viewer.encoding" => Some("auto | utf-8 | utf-16le | utf-16be | latin-1"),
        "shell.mode" => Some("platform-default | login | interactive"),
        "shell.close_policy" => Some("warn | keep-open | close"),
        "panel-modes.left" | "panel-modes.right" => Some("brief | medium | full"),
        "interface.startup_panel" => Some("left | right; applies after restart"),
        _ => None,
    }
}
