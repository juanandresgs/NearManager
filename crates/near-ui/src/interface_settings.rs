use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupPanel {
    #[default]
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct InterfaceSettings {
    pub schema: u32,
    pub show_status_line: bool,
    pub show_keybar: bool,
    pub tree_indent_width: u8,
    pub menu_wrap_navigation: bool,
    pub dialog_wrap_focus: bool,
    pub command_line_completion: bool,
    pub startup_panel: StartupPanel,
}

impl Default for InterfaceSettings {
    fn default() -> Self {
        Self {
            schema: 1,
            show_status_line: true,
            show_keybar: true,
            tree_indent_width: 2,
            menu_wrap_navigation: false,
            dialog_wrap_focus: true,
            command_line_completion: true,
            startup_panel: StartupPanel::Left,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InterfaceSettingsError {
    #[error("interface settings are invalid: {0}")]
    Parse(String),
    #[error("unsupported interface settings schema {0}")]
    UnsupportedSchema(u32),
    #[error("tree indentation must be between 1 and 8 cells")]
    InvalidTreeIndent,
}

impl InterfaceSettings {
    /// Parses and validates a versioned interface settings document.
    ///
    /// # Errors
    ///
    /// Returns schema, type, or range validation failures.
    pub fn from_toml(source: &str) -> Result<Self, InterfaceSettingsError> {
        let settings: Self = toml::from_str(source)
            .map_err(|error| InterfaceSettingsError::Parse(error.to_string()))?;
        settings.validate()?;
        Ok(settings)
    }

    /// Validates this interface policy.
    ///
    /// # Errors
    ///
    /// Returns unsupported schema or range failures.
    pub fn validate(self) -> Result<(), InterfaceSettingsError> {
        if self.schema != 1 {
            return Err(InterfaceSettingsError::UnsupportedSchema(self.schema));
        }
        if !(1..=8).contains(&self.tree_indent_width) {
            return Err(InterfaceSettingsError::InvalidTreeIndent);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{InterfaceSettings, StartupPanel};

    #[test]
    fn startup_panel_is_versioned_and_defaults_left_for_existing_documents() {
        let legacy = InterfaceSettings::from_toml(
            r"
            schema = 1
            show_status_line = true
            show_keybar = true
            tree_indent_width = 2
            menu_wrap_navigation = false
            dialog_wrap_focus = true
            command_line_completion = true
            ",
        )
        .unwrap();
        assert_eq!(legacy.startup_panel, StartupPanel::Left);

        let right = InterfaceSettings::from_toml(
            r#"
            schema = 1
            startup_panel = "right"
            "#,
        )
        .unwrap();
        assert_eq!(right.startup_panel, StartupPanel::Right);
    }
}
