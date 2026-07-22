use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    Scene, SceneBorder, SceneColor, ScenePrimitive, SceneRect, SceneTextStyle, SemanticTheme,
    TextAlignment,
    semantic::{RoleBuffer, SemanticSnapshot},
};

pub(crate) fn render_scene(
    frame: &mut Frame<'_>,
    scene: &Scene,
    theme: &SemanticTheme,
    roles: &mut RoleBuffer,
) {
    for primitive in scene.primitives() {
        match primitive {
            ScenePrimitive::Fill { area, role } => {
                let area = to_rect(*area);
                roles.fill(area, role.as_str());
                frame.render_widget(Block::default().style(theme.style(role.as_str())), area);
            }
            ScenePrimitive::Text {
                area,
                content,
                role,
                alignment,
            } => {
                let area = to_rect(*area);
                roles.fill(area, role.as_str());
                frame.render_widget(
                    Paragraph::new(content.as_str())
                        .alignment(to_alignment(*alignment))
                        .style(theme.style(role.as_str())),
                    area,
                );
            }
            ScenePrimitive::StyledText {
                area,
                content,
                role,
                style,
            } => {
                let area = to_rect(*area);
                roles.fill(area, role.as_str());
                frame.render_widget(
                    Paragraph::new(content.as_str())
                        .style(apply_text_style(theme.style(role.as_str()), *style)),
                    area,
                );
            }
            ScenePrimitive::Border {
                area,
                title,
                role,
                kind,
            } => {
                let area = to_rect(*area);
                roles.fill(area, role.as_str());
                let mut block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(to_border_type(*kind, theme))
                    .style(theme.style(role.as_str()));
                if let Some(title) = title {
                    block = block.title(title.as_str());
                }
                frame.render_widget(block, area);
            }
        }
    }
}

fn apply_text_style(mut style: Style, text: SceneTextStyle) -> Style {
    if let Some(foreground) = text.foreground {
        style = style.fg(to_color(foreground));
    }
    if let Some(background) = text.background {
        style = style.bg(to_color(background));
    }
    for (enabled, modifier) in [
        (text.bold, Modifier::BOLD),
        (text.dim, Modifier::DIM),
        (text.italic, Modifier::ITALIC),
        (text.underline, Modifier::UNDERLINED),
        (text.inverse, Modifier::REVERSED),
    ] {
        if enabled {
            style = style.add_modifier(modifier);
        }
    }
    style
}

const fn to_color(color: SceneColor) -> Color {
    match color {
        SceneColor::TerminalDefault => Color::Reset,
        SceneColor::Indexed(index) => Color::Indexed(index),
        SceneColor::Rgb { red, green, blue } => Color::Rgb(red, green, blue),
    }
}

/// Renders a backend-independent scene through Near's private test backend.
///
/// # Panics
///
/// Panics only if Ratatui's infallible test backend unexpectedly reports an error.
pub fn snapshot_scene(
    scene: &Scene,
    theme: &SemanticTheme,
    width: u16,
    height: u16,
) -> SemanticSnapshot {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend is infallible");
    let mut roles = RoleBuffer::new(width, height, "workspace.background");
    terminal
        .draw(|frame| render_scene(frame, scene, theme, &mut roles))
        .expect("test backend is infallible");
    SemanticSnapshot::from_buffer(terminal.backend().buffer(), &roles)
}

fn to_rect(area: SceneRect) -> Rect {
    Rect::new(area.x, area.y, area.width, area.height)
}

fn to_alignment(alignment: TextAlignment) -> Alignment {
    match alignment {
        TextAlignment::Left => Alignment::Left,
        TextAlignment::Center => Alignment::Center,
        TextAlignment::Right => Alignment::Right,
    }
}

fn to_border_type(kind: SceneBorder, theme: &SemanticTheme) -> BorderType {
    match kind {
        SceneBorder::Theme => match theme.glyphs().border.as_str() {
            "double" => BorderType::Double,
            "rounded" => BorderType::Rounded,
            "thick" => BorderType::Thick,
            _ => BorderType::Plain,
        },
        SceneBorder::Plain => BorderType::Plain,
        SceneBorder::Double => BorderType::Double,
        SceneBorder::Rounded => BorderType::Rounded,
        SceneBorder::Thick => BorderType::Thick,
    }
}

#[cfg(test)]
mod tests {
    use super::{SceneColor, SceneTextStyle, apply_text_style};
    use ratatui::style::{Color, Modifier, Style};

    #[test]
    fn styled_scene_text_maps_terminal_colors_and_attributes() {
        let style = apply_text_style(
            Style::default(),
            SceneTextStyle {
                foreground: Some(SceneColor::Indexed(1)),
                background: Some(SceneColor::Rgb {
                    red: 2,
                    green: 3,
                    blue: 4,
                }),
                bold: true,
                underline: true,
                ..SceneTextStyle::default()
            },
        );
        assert_eq!(style.fg, Some(Color::Indexed(1)));
        assert_eq!(style.bg, Some(Color::Rgb(2, 3, 4)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));
    }
}
