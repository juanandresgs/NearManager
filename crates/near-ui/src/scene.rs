use near_core::RoleId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl SceneRect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    pub const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }

    #[must_use]
    pub const fn inset(self, amount: u16) -> Self {
        Self {
            x: self.x.saturating_add(amount),
            y: self.y.saturating_add(amount),
            width: self.width.saturating_sub(amount.saturating_mul(2)),
            height: self.height.saturating_sub(amount.saturating_mul(2)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneColor {
    TerminalDefault,
    Indexed(u8),
    Rgb { red: u8, green: u8, blue: u8 },
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneTextStyle {
    pub foreground: Option<SceneColor>,
    pub background: Option<SceneColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneBorder {
    Theme,
    Plain,
    Double,
    Rounded,
    Thick,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScenePrimitive {
    Fill {
        area: SceneRect,
        role: RoleId,
    },
    Text {
        area: SceneRect,
        content: String,
        role: RoleId,
        alignment: TextAlignment,
    },
    StyledText {
        area: SceneRect,
        content: String,
        role: RoleId,
        style: SceneTextStyle,
    },
    Border {
        area: SceneRect,
        title: Option<String>,
        role: RoleId,
        kind: SceneBorder,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Scene {
    primitives: Vec<ScenePrimitive>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn primitives(&self) -> &[ScenePrimitive] {
        &self.primitives
    }

    pub fn extend(&mut self, scene: Self) {
        self.primitives.extend(scene.primitives);
    }

    pub fn fill(&mut self, area: SceneRect, role: impl Into<RoleId>) {
        self.primitives.push(ScenePrimitive::Fill {
            area,
            role: role.into(),
        });
    }

    pub fn text(&mut self, area: SceneRect, content: impl Into<String>, role: impl Into<RoleId>) {
        self.aligned_text(area, content, role, TextAlignment::Left);
    }

    pub fn aligned_text(
        &mut self,
        area: SceneRect,
        content: impl Into<String>,
        role: impl Into<RoleId>,
        alignment: TextAlignment,
    ) {
        self.primitives.push(ScenePrimitive::Text {
            area,
            content: content.into(),
            role: role.into(),
            alignment,
        });
    }

    pub fn styled_text(
        &mut self,
        area: SceneRect,
        content: impl Into<String>,
        role: impl Into<RoleId>,
        style: SceneTextStyle,
    ) {
        self.primitives.push(ScenePrimitive::StyledText {
            area,
            content: content.into(),
            role: role.into(),
            style,
        });
    }

    pub fn border(&mut self, area: SceneRect, title: Option<String>, role: impl Into<RoleId>) {
        self.primitives.push(ScenePrimitive::Border {
            area,
            title,
            role: role.into(),
            kind: SceneBorder::Theme,
        });
    }
}
