use near_core::ActionContext;
use near_ui::{
    RenderContext, ScenePrimitive, SceneRect, SemanticColor, SemanticTheme, Surface,
    TerminalColorDepth, ViewerSurface,
};

#[test]
fn viewer_and_editor_roles_match_far_surface_palette() {
    let theme = SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap();
    let cyan = Some(SemanticColor::Rgb {
        red: 0,
        green: 170,
        blue: 170,
    });
    let navy = Some(SemanticColor::Rgb {
        red: 0,
        green: 0,
        blue: 128,
    });
    for role in ["viewer.text", "editor.text"] {
        let style = theme.resolve(role, TerminalColorDepth::TrueColor);
        assert_eq!(style.foreground, cyan, "{role}");
        assert_eq!(style.background, navy, "{role}");
    }

    let viewer = ViewerSurface::text("viewer", "Command output", "failure details");
    let scene = viewer.scene(
        SceneRect::new(0, 0, 60, 12),
        &RenderContext {
            focused: true,
            action: &ActionContext::default(),
        },
    );
    for role in [
        "viewer.background",
        "viewer.border",
        "viewer.text",
        "viewer.status",
    ] {
        assert!(scene.primitives().iter().any(|primitive| match primitive {
            ScenePrimitive::Fill { role: actual, .. }
            | ScenePrimitive::Text { role: actual, .. }
            | ScenePrimitive::StyledText { role: actual, .. }
            | ScenePrimitive::Border { role: actual, .. } => actual.as_str() == role,
        }));
    }
}
