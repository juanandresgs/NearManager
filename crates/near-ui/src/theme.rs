use std::collections::{BTreeMap, BTreeSet, HashSet};

use near_core::RoleId;
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalColorDepth {
    Monochrome,
    Ansi16,
    Ansi256,
    TrueColor,
}

impl TerminalColorDepth {
    pub fn detect_from_environment() -> Self {
        Self::detect_from(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
            std::env::var_os("NO_COLOR").is_some(),
        )
    }

    pub fn detect_from(colorterm: Option<&str>, term: Option<&str>, no_color: bool) -> Self {
        if no_color || term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
            return Self::Monochrome;
        }
        if colorterm.is_some_and(|value| {
            value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
        }) {
            return Self::TrueColor;
        }
        if term.is_some_and(|value| value.to_ascii_lowercase().contains("256color")) {
            return Self::Ansi256;
        }
        Self::Ansi16
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticColor {
    Default,
    Rgb { red: u8, green: u8, blue: u8 },
    Ansi(u8),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SemanticModifier {
    Bold,
    Dim,
    Italic,
    Underline,
    Reverse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRoleStyle {
    pub requested_role: RoleId,
    pub resolved_role: RoleId,
    pub foreground: Option<SemanticColor>,
    pub background: Option<SemanticColor>,
    pub modifiers: BTreeSet<SemanticModifier>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeGlyphs {
    pub border: String,
    pub border_ascii: String,
    pub selection_mark: String,
    pub selection_mark_ascii: String,
    pub tree_branch: String,
    pub tree_last: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThemeDensity {
    pub dialog_padding_x: u16,
    pub dialog_padding_y: u16,
    pub menu_padding_x: u16,
    pub status_rows: u16,
    pub keybar_rows: u16,
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("invalid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("theme color '{0}' is not defined")]
    UnknownColor(String),
    #[error("invalid RGB color '{0}'")]
    InvalidColor(String),
    #[error("theme must define the core 'text' role")]
    MissingTextRole,
    #[error("role '{role}' falls back to unknown role '{fallback}'")]
    UnknownFallback { role: String, fallback: String },
    #[error("application role '{0}' must declare a core fallback")]
    MissingApplicationFallback(String),
    #[error("role fallback cycle: {0}")]
    FallbackCycle(String),
    #[error("role '{role}' uses unknown modifier '{modifier}'")]
    UnknownModifier { role: String, modifier: String },
    #[error("theme has no semantic role '{0}'")]
    UnknownRole(String),
}

#[derive(Clone, Debug)]
pub struct SemanticTheme {
    name: String,
    palette: BTreeMap<String, SemanticColor>,
    roles: BTreeMap<String, RoleStyle>,
    glyphs: ThemeGlyphs,
    density: ThemeDensity,
    depth: TerminalColorDepth,
}

impl SemanticTheme {
    /// Parses and validates a semantic theme document.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid TOML, colors, modifiers, or fallback graphs.
    pub fn from_toml(source: &str) -> Result<Self, ThemeError> {
        let file: ThemeFile = toml::from_str(source)?;
        let palette = file
            .palette
            .into_iter()
            .map(|(name, value)| parse_color(&value).map(|color| (name, color)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut roles = BTreeMap::new();
        for (name, role) in file.roles {
            let foreground = role
                .fg
                .as_deref()
                .map(|value| resolve_color(value, &palette))
                .transpose()?;
            let background = role
                .bg
                .as_deref()
                .map(|value| resolve_color(value, &palette))
                .transpose()?;
            let modifiers = role
                .modifiers
                .into_iter()
                .map(|modifier| parse_modifier(&name, &modifier))
                .collect::<Result<_, _>>()?;
            roles.insert(
                name,
                RoleStyle {
                    foreground,
                    background,
                    modifiers,
                    fallback: role.fallback,
                },
            );
        }
        if !roles.contains_key("text") {
            return Err(ThemeError::MissingTextRole);
        }
        validate_fallbacks(&roles)?;
        Ok(Self {
            name: file.name,
            palette,
            roles,
            glyphs: file.glyphs.into(),
            density: file.density.into(),
            depth: TerminalColorDepth::TrueColor,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn glyphs(&self) -> &ThemeGlyphs {
        &self.glyphs
    }

    pub fn density(&self) -> ThemeDensity {
        self.density
    }

    #[must_use]
    pub fn with_depth(mut self, depth: TerminalColorDepth) -> Self {
        self.depth = depth;
        self
    }

    pub fn terminal_depth(&self) -> TerminalColorDepth {
        self.depth
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.resolve_role(role).is_some()
    }

    pub fn resolve(&self, role: &str, depth: TerminalColorDepth) -> ResolvedRoleStyle {
        let (resolved_name, style) = self
            .resolve_role(role)
            .unwrap_or_else(|| ("text", self.merge_role("text")));
        let mut modifiers = style.modifiers.clone();
        let (foreground, background) = match depth {
            TerminalColorDepth::TrueColor => (style.foreground, style.background),
            TerminalColorDepth::Ansi256 => (
                style.foreground.map(degrade_ansi256),
                style.background.map(degrade_ansi256),
            ),
            TerminalColorDepth::Ansi16 => (
                style.foreground.map(degrade_ansi16),
                style.background.map(degrade_ansi16),
            ),
            TerminalColorDepth::Monochrome => {
                apply_monochrome_distinction(role, &mut modifiers);
                (None, None)
            }
        };
        ResolvedRoleStyle {
            requested_role: RoleId::from(role),
            resolved_role: RoleId::from(resolved_name),
            foreground,
            background,
            modifiers,
        }
    }

    pub fn palette_len(&self) -> usize {
        self.palette.len()
    }

    pub fn role_names(&self) -> impl Iterator<Item = &str> {
        self.roles.keys().map(String::as_str)
    }

    pub fn role_colors(
        &self,
        role: &str,
    ) -> Option<(Option<SemanticColor>, Option<SemanticColor>)> {
        self.roles
            .get(role)
            .map(|style| (style.foreground, style.background))
    }

    /// Updates direct colors for one semantic role without changing its fallback or modifiers.
    ///
    /// # Errors
    ///
    /// Returns an error when the role is not declared by this theme.
    pub fn set_role_colors(
        &mut self,
        role: &str,
        foreground: Option<SemanticColor>,
        background: Option<SemanticColor>,
    ) -> Result<(), ThemeError> {
        let Some(style) = self.roles.get_mut(role) else {
            return Err(ThemeError::UnknownRole(role.to_owned()));
        };
        style.foreground = foreground;
        style.background = background;
        Ok(())
    }

    pub(crate) fn style(&self, role: &str) -> Style {
        self.style_at_depth(role, self.depth)
    }

    pub(crate) fn style_at_depth(&self, role: &str, depth: TerminalColorDepth) -> Style {
        let resolved = self.resolve(role, depth);
        let mut style = Style::default();
        if let Some(foreground) = resolved.foreground {
            style = style.fg(to_ratatui_color(foreground));
        }
        if let Some(background) = resolved.background {
            style = style.bg(to_ratatui_color(background));
        }
        for modifier in resolved.modifiers {
            style = style.add_modifier(to_ratatui_modifier(modifier));
        }
        style
    }

    fn resolve_role<'a>(&'a self, role: &'a str) -> Option<(&'a str, RoleStyle)> {
        let mut current = role;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(current) {
                return None;
            }
            if self.roles.contains_key(current) {
                return Some((current, self.merge_role(current)));
            }
            if let Some((parent, _)) = current.rsplit_once('.') {
                current = parent;
                continue;
            }
            return self
                .roles
                .contains_key("text")
                .then(|| ("text", self.merge_role("text")));
        }
    }

    fn merge_role(&self, role: &str) -> RoleStyle {
        let current = &self.roles[role];
        let mut merged = current
            .fallback
            .as_deref()
            .map_or_else(RoleStyle::default, |fallback| self.merge_role(fallback));
        if current.foreground.is_some() {
            merged.foreground = current.foreground;
        }
        if current.background.is_some() {
            merged.background = current.background;
        }
        merged.modifiers.extend(&current.modifiers);
        merged.fallback.clone_from(&current.fallback);
        merged
    }
}

/// Parses a terminal-aware editable color value.
///
/// # Errors
///
/// Returns an error unless the value is `default`, `terminal`, `ansi:N`, or `#RRGGBB`.
pub fn parse_semantic_color(value: &str) -> Result<SemanticColor, ThemeError> {
    match value.trim() {
        "default" | "terminal" => Ok(SemanticColor::Default),
        value if value.starts_with("ansi:") => value[5..]
            .parse::<u8>()
            .map(SemanticColor::Ansi)
            .map_err(|_| ThemeError::InvalidColor(value.to_owned())),
        value => parse_color(value),
    }
}

pub fn format_semantic_color(color: SemanticColor) -> String {
    match color {
        SemanticColor::Default => "default".to_owned(),
        SemanticColor::Ansi(index) => format!("ansi:{index}"),
        SemanticColor::Rgb { red, green, blue } => format!("#{red:02X}{green:02X}{blue:02X}"),
    }
}

#[derive(Clone, Debug, Default)]
struct RoleStyle {
    foreground: Option<SemanticColor>,
    background: Option<SemanticColor>,
    modifiers: BTreeSet<SemanticModifier>,
    fallback: Option<String>,
}

fn resolve_color(
    value: &str,
    palette: &BTreeMap<String, SemanticColor>,
) -> Result<SemanticColor, ThemeError> {
    match value {
        "default" | "terminal" => Ok(SemanticColor::Default),
        _ if value.starts_with('#') => parse_color(value),
        _ => palette
            .get(value)
            .copied()
            .ok_or_else(|| ThemeError::UnknownColor(value.to_owned())),
    }
}

fn parse_color(value: &str) -> Result<SemanticColor, ThemeError> {
    let raw = value
        .strip_prefix('#')
        .ok_or_else(|| ThemeError::InvalidColor(value.to_owned()))?;
    if raw.len() != 6 {
        return Err(ThemeError::InvalidColor(value.to_owned()));
    }
    let red = u8::from_str_radix(&raw[0..2], 16)
        .map_err(|_| ThemeError::InvalidColor(value.to_owned()))?;
    let green = u8::from_str_radix(&raw[2..4], 16)
        .map_err(|_| ThemeError::InvalidColor(value.to_owned()))?;
    let blue = u8::from_str_radix(&raw[4..6], 16)
        .map_err(|_| ThemeError::InvalidColor(value.to_owned()))?;
    Ok(SemanticColor::Rgb { red, green, blue })
}

fn parse_modifier(role: &str, value: &str) -> Result<SemanticModifier, ThemeError> {
    match value {
        "bold" => Ok(SemanticModifier::Bold),
        "dim" => Ok(SemanticModifier::Dim),
        "italic" => Ok(SemanticModifier::Italic),
        "underline" => Ok(SemanticModifier::Underline),
        "reverse" => Ok(SemanticModifier::Reverse),
        _ => Err(ThemeError::UnknownModifier {
            role: role.to_owned(),
            modifier: value.to_owned(),
        }),
    }
}

fn validate_fallbacks(roles: &BTreeMap<String, RoleStyle>) -> Result<(), ThemeError> {
    for (name, role) in roles {
        let root = name.split('.').next().unwrap_or(name);
        if !CORE_ROLE_ROOTS.contains(&root) && role.fallback.is_none() {
            return Err(ThemeError::MissingApplicationFallback(name.clone()));
        }
        if let Some(fallback) = &role.fallback
            && !roles.contains_key(fallback)
        {
            return Err(ThemeError::UnknownFallback {
                role: name.clone(),
                fallback: fallback.clone(),
            });
        }
        let mut path = vec![name.as_str()];
        let mut current = role.fallback.as_deref();
        while let Some(fallback) = current {
            if let Some(index) = path.iter().position(|item| *item == fallback) {
                let mut cycle = path[index..].to_vec();
                cycle.push(fallback);
                return Err(ThemeError::FallbackCycle(cycle.join(" -> ")));
            }
            path.push(fallback);
            current = roles
                .get(fallback)
                .and_then(|style| style.fallback.as_deref());
        }
    }
    Ok(())
}

const CORE_ROLE_ROOTS: [&str; 8] = [
    "text",
    "workspace",
    "panel",
    "dialog",
    "control",
    "status",
    "keybar",
    "selection",
];

fn apply_monochrome_distinction(role: &str, modifiers: &mut BTreeSet<SemanticModifier>) {
    if role.contains("focused") {
        modifiers.insert(SemanticModifier::Reverse);
    }
    if role.contains("selected") || role.contains("warning") {
        modifiers.insert(SemanticModifier::Bold);
    }
    if role.contains("warning") || role.contains("match") {
        modifiers.insert(SemanticModifier::Underline);
    }
    if role.contains("disabled") {
        modifiers.insert(SemanticModifier::Dim);
    }
}

fn degrade_ansi16(color: SemanticColor) -> SemanticColor {
    match color {
        SemanticColor::Rgb { red, green, blue } => {
            SemanticColor::Ansi(nearest_palette(red, green, blue, &ANSI16))
        }
        SemanticColor::Ansi(index) => SemanticColor::Ansi(index.min(15)),
        SemanticColor::Default => SemanticColor::Default,
    }
}

fn degrade_ansi256(color: SemanticColor) -> SemanticColor {
    match color {
        SemanticColor::Rgb { red, green, blue } => {
            let red_index = quantize_cube(red);
            let green_index = quantize_cube(green);
            let blue_index = quantize_cube(blue);
            SemanticColor::Ansi(16 + 36 * red_index + 6 * green_index + blue_index)
        }
        other => other,
    }
}

fn quantize_cube(value: u8) -> u8 {
    u8::try_from((u16::from(value) * 5 + 127) / 255).unwrap_or(5)
}

fn nearest_palette(red: u8, green: u8, blue: u8, palette: &[(u8, u8, u8); 16]) -> u8 {
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, (candidate_red, candidate_green, candidate_blue))| {
            let red_delta = i32::from(red) - i32::from(*candidate_red);
            let green_delta = i32::from(green) - i32::from(*candidate_green);
            let blue_delta = i32::from(blue) - i32::from(*candidate_blue);
            red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta
        })
        .map_or(0, |(index, _)| u8::try_from(index).unwrap_or(15))
}

fn to_ratatui_color(color: SemanticColor) -> Color {
    match color {
        SemanticColor::Default => Color::Reset,
        SemanticColor::Rgb { red, green, blue } => Color::Rgb(red, green, blue),
        SemanticColor::Ansi(index) => Color::Indexed(index),
    }
}

fn to_ratatui_modifier(modifier: SemanticModifier) -> Modifier {
    match modifier {
        SemanticModifier::Bold => Modifier::BOLD,
        SemanticModifier::Dim => Modifier::DIM,
        SemanticModifier::Italic => Modifier::ITALIC,
        SemanticModifier::Underline => Modifier::UNDERLINED,
        SemanticModifier::Reverse => Modifier::REVERSED,
    }
}

const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

#[derive(Deserialize)]
struct ThemeFile {
    name: String,
    #[serde(default)]
    palette: BTreeMap<String, String>,
    #[serde(default)]
    roles: BTreeMap<String, RoleFile>,
    #[serde(default)]
    glyphs: GlyphsFile,
    #[serde(default)]
    density: DensityFile,
}

#[derive(Deserialize)]
struct RoleFile {
    fg: Option<String>,
    bg: Option<String>,
    #[serde(default)]
    modifiers: Vec<String>,
    fallback: Option<String>,
}

#[derive(Deserialize)]
struct GlyphsFile {
    #[serde(default = "default_border")]
    border: String,
    #[serde(default = "default_border_ascii")]
    border_ascii: String,
    #[serde(default = "default_selection")]
    selection_mark: String,
    #[serde(default = "default_selection_ascii")]
    selection_mark_ascii: String,
    #[serde(default = "default_tree_branch")]
    tree_branch: String,
    #[serde(default = "default_tree_last")]
    tree_last: String,
}

impl Default for GlyphsFile {
    fn default() -> Self {
        Self {
            border: default_border(),
            border_ascii: default_border_ascii(),
            selection_mark: default_selection(),
            selection_mark_ascii: default_selection_ascii(),
            tree_branch: default_tree_branch(),
            tree_last: default_tree_last(),
        }
    }
}

impl From<GlyphsFile> for ThemeGlyphs {
    fn from(value: GlyphsFile) -> Self {
        Self {
            border: value.border,
            border_ascii: value.border_ascii,
            selection_mark: value.selection_mark,
            selection_mark_ascii: value.selection_mark_ascii,
            tree_branch: value.tree_branch,
            tree_last: value.tree_last,
        }
    }
}

#[derive(Deserialize)]
struct DensityFile {
    #[serde(default)]
    dialog_padding_x: u16,
    #[serde(default)]
    dialog_padding_y: u16,
    #[serde(default)]
    menu_padding_x: u16,
    #[serde(default = "default_one")]
    status_rows: u16,
    #[serde(default = "default_one")]
    keybar_rows: u16,
}

impl Default for DensityFile {
    fn default() -> Self {
        Self {
            dialog_padding_x: 0,
            dialog_padding_y: 0,
            menu_padding_x: 0,
            status_rows: 1,
            keybar_rows: 1,
        }
    }
}

impl From<DensityFile> for ThemeDensity {
    fn from(value: DensityFile) -> Self {
        Self {
            dialog_padding_x: value.dialog_padding_x,
            dialog_padding_y: value.dialog_padding_y,
            menu_padding_x: value.menu_padding_x,
            status_rows: value.status_rows,
            keybar_rows: value.keybar_rows,
        }
    }
}

fn default_border() -> String {
    "single".to_owned()
}

fn default_border_ascii() -> String {
    "+|-".to_owned()
}

fn default_selection() -> String {
    "√".to_owned()
}

fn default_selection_ascii() -> String {
    "*".to_owned()
}

fn default_tree_branch() -> String {
    "├".to_owned()
}

fn default_tree_last() -> String {
    "└".to_owned()
}

const fn default_one() -> u16 {
    1
}

#[cfg(test)]
mod tests {
    use super::{
        SemanticColor, SemanticModifier, SemanticTheme, TerminalColorDepth, ThemeError,
        format_semantic_color, parse_semantic_color,
    };

    const FAR: &str = include_str!("../../../specs/theme.toml");
    const HIGH_CONTRAST: &str = include_str!("../../../specs/theme-high-contrast.toml");
    const TERMINAL_NATIVE: &str = include_str!("../../../specs/theme-terminal-native.toml");

    #[test]
    fn falls_back_through_role_hierarchy() {
        let theme = SemanticTheme::from_toml(FAR).unwrap();
        let style = theme.resolve(
            "panel.item.directory.focused.extra",
            TerminalColorDepth::TrueColor,
        );
        assert_eq!(style.resolved_role.as_str(), "panel.item.directory");
        assert_eq!(theme.palette_len(), 9);
    }

    #[test]
    fn degrades_colors_and_preserves_state_distinctions() {
        let theme = SemanticTheme::from_toml(FAR).unwrap();
        let focused = theme.resolve("panel.item.focused", TerminalColorDepth::Ansi16);
        let selected = theme.resolve("panel.item.selected", TerminalColorDepth::Ansi16);
        assert!(matches!(focused.foreground, Some(SemanticColor::Ansi(_))));
        assert_ne!(focused, selected);

        let monochrome_focus = theme.resolve("panel.item.focused", TerminalColorDepth::Monochrome);
        let monochrome_selection =
            theme.resolve("panel.item.selected", TerminalColorDepth::Monochrome);
        assert!(
            monochrome_focus
                .modifiers
                .contains(&SemanticModifier::Reverse)
        );
        assert!(
            monochrome_selection
                .modifiers
                .contains(&SemanticModifier::Bold)
        );
        assert_ne!(monochrome_focus, monochrome_selection);
    }

    #[test]
    fn validates_fallbacks_and_modifiers() {
        let unknown_fallback = r#"
            name = "bad"
            [roles.text]
            [roles."app.special"]
            fallback = "missing"
        "#;
        assert!(matches!(
            SemanticTheme::from_toml(unknown_fallback),
            Err(ThemeError::UnknownFallback { .. })
        ));

        let unknown_modifier = r#"
            name = "bad"
            [roles.text]
            modifiers = ["blink"]
        "#;
        assert!(matches!(
            SemanticTheme::from_toml(unknown_modifier),
            Err(ThemeError::UnknownModifier { .. })
        ));

        let missing_application_fallback = r#"
            name = "bad"
            [roles.text]
            [roles."app.record"]
            modifiers = ["bold"]
        "#;
        assert!(matches!(
            SemanticTheme::from_toml(missing_application_fallback),
            Err(ThemeError::MissingApplicationFallback(_))
        ));
    }

    #[test]
    fn explicit_fallback_inherits_unspecified_style_fields() {
        let source = r##"
            name = "application"
            [palette]
            white = "#ffffff"
            blue = "#000080"
            [roles.text]
            fg = "white"
            [roles."panel.item"]
            bg = "blue"
            [roles."app.record"]
            fallback = "panel.item"
            modifiers = ["bold"]
        "##;
        let theme = SemanticTheme::from_toml(source).unwrap();
        let style = theme.resolve("app.record", TerminalColorDepth::TrueColor);
        assert_eq!(style.resolved_role.as_str(), "app.record");
        assert_eq!(
            style.background,
            Some(SemanticColor::Rgb {
                red: 0,
                green: 0,
                blue: 128
            })
        );
        assert!(style.modifiers.contains(&SemanticModifier::Bold));
    }

    #[test]
    fn detects_terminal_color_depth_without_terminal_queries() {
        assert_eq!(
            TerminalColorDepth::detect_from(Some("truecolor"), Some("xterm"), false),
            TerminalColorDepth::TrueColor
        );
        assert_eq!(
            TerminalColorDepth::detect_from(None, Some("screen-256color"), false),
            TerminalColorDepth::Ansi256
        );
        assert_eq!(
            TerminalColorDepth::detect_from(None, Some("xterm"), false),
            TerminalColorDepth::Ansi16
        );
        assert_eq!(
            TerminalColorDepth::detect_from(Some("truecolor"), Some("xterm"), true),
            TerminalColorDepth::Monochrome
        );
    }

    #[test]
    fn all_shipped_presets_preserve_focus_and_selection_at_low_color() {
        for source in [FAR, HIGH_CONTRAST, TERMINAL_NATIVE] {
            let theme = SemanticTheme::from_toml(source).unwrap();
            let focused = theme.resolve("panel.item.focused", TerminalColorDepth::Ansi16);
            let selected = theme.resolve("panel.item.selected.focused", TerminalColorDepth::Ansi16);
            assert_ne!(
                focused,
                selected,
                "{} collapsed interaction states",
                theme.name()
            );
            assert!(!theme.glyphs().selection_mark.is_empty());
            assert_eq!(theme.density().status_rows, 1);
        }
    }

    #[test]
    fn monochrome_preserves_critical_interaction_states() {
        let theme = SemanticTheme::from_toml(HIGH_CONTRAST).unwrap();
        let normal = theme.resolve("panel.item", TerminalColorDepth::Monochrome);
        let focused = theme.resolve("panel.item.focused", TerminalColorDepth::Monochrome);
        let selected = theme.resolve("panel.item.selected", TerminalColorDepth::Monochrome);
        let selected_focused = theme.resolve(
            "panel.item.selected.focused",
            TerminalColorDepth::Monochrome,
        );
        let warning = theme.resolve("status.warning", TerminalColorDepth::Monochrome);
        let disabled = theme.resolve("control.disabled", TerminalColorDepth::Monochrome);

        assert!(focused.modifiers.contains(&SemanticModifier::Reverse));
        assert!(selected.modifiers.contains(&SemanticModifier::Bold));
        assert!(
            selected_focused
                .modifiers
                .contains(&SemanticModifier::Reverse)
        );
        assert!(
            selected_focused
                .modifiers
                .contains(&SemanticModifier::Underline)
        );
        assert!(warning.modifiers.contains(&SemanticModifier::Bold));
        assert!(warning.modifiers.contains(&SemanticModifier::Underline));
        assert!(disabled.modifiers.contains(&SemanticModifier::Dim));
        assert_ne!(normal, focused);
        assert_ne!(normal, selected);
        assert_ne!(focused, selected_focused);
        assert!(!theme.glyphs().selection_mark_ascii.is_empty());
    }

    #[test]
    fn high_contrast_rgb_roles_meet_text_contrast_threshold() {
        let theme = SemanticTheme::from_toml(HIGH_CONTRAST).unwrap();
        for role in [
            "text",
            "panel.background",
            "panel.border",
            "panel.title",
            "panel.item",
            "panel.item.directory",
            "panel.item.selected",
            "panel.item.focused",
            "panel.item.selected.focused",
            "dialog.background",
            "dialog.border",
            "viewer.background",
            "viewer.border",
            "viewer.text",
            "viewer.selected",
            "viewer.status",
            "editor.background",
            "editor.border",
            "editor.text",
            "editor.selected",
            "editor.status",
            "control.focused",
            "control.disabled",
            "status.normal",
            "status.warning",
            "keybar.key",
            "keybar.label",
            "lookup.bar",
            "lookup.match",
            "lookup.match.focused",
            "selection.match",
        ] {
            let style = theme.resolve(role, TerminalColorDepth::TrueColor);
            let ratio = contrast_ratio(
                style.foreground.expect("high-contrast foreground"),
                style.background.expect("high-contrast background"),
            );
            assert!(ratio >= 4.5, "{role} contrast ratio was {ratio:.2}:1");
        }
    }

    #[test]
    fn editable_colors_support_terminal_rgb_and_ansi_values() {
        assert_eq!(
            parse_semantic_color("default").unwrap(),
            SemanticColor::Default
        );
        assert_eq!(
            parse_semantic_color("ansi:42").unwrap(),
            SemanticColor::Ansi(42)
        );
        let rgb = SemanticColor::Rgb {
            red: 0x12,
            green: 0x34,
            blue: 0x56,
        };
        assert_eq!(parse_semantic_color("#123456").unwrap(), rgb);
        assert_eq!(format_semantic_color(rgb), "#123456");
        assert!(parse_semantic_color("ansi:999").is_err());
    }

    fn contrast_ratio(foreground: SemanticColor, background: SemanticColor) -> f64 {
        let foreground = relative_luminance(foreground);
        let background = relative_luminance(background);
        let lighter = foreground.max(background);
        let darker = foreground.min(background);
        (lighter + 0.05) / (darker + 0.05)
    }

    fn relative_luminance(color: SemanticColor) -> f64 {
        let SemanticColor::Rgb { red, green, blue } = color else {
            panic!("contrast checks require RGB colors")
        };
        [red, green, blue]
            .map(|component| {
                let component = f64::from(component) / 255.0;
                if component <= 0.040_45 {
                    component / 12.92
                } else {
                    ((component + 0.055) / 1.055).powf(2.4)
                }
            })
            .into_iter()
            .zip([0.212_6, 0.715_2, 0.072_2])
            .map(|(component, weight)| component * weight)
            .sum()
    }
}
