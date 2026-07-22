use near_config::{ConfigLayerKind, SettingProvenance};
use near_core::{CommandInvocation, CommandValue};
use near_terminal::TerminalEvent;
use near_ui::{FarWorkspace, Keymap, SemanticTheme, parse_key_stroke};

#[test]
fn typed_settings_render_exact_layer_and_source() {
    let mut workspace = FarWorkspace::demo().with_setting_provenance([(
        "viewer.wrap".to_owned(),
        SettingProvenance {
            layer: ConfigLayerKind::Cli,
            source: "/tmp/operator-viewer.toml".to_owned(),
        },
    )]);
    let mut keymap = Keymap::from_toml(include_str!("../../../specs/keymap.toml")).unwrap();
    workspace.dispatch(&CommandInvocation {
        id: "near.settings.show".into(),
        arguments: std::collections::BTreeMap::<String, CommandValue>::new(),
    });
    for character in "wrap text".chars() {
        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(parse_key_stroke(&character.to_string()).unwrap()),
        );
    }
    let frame = workspace
        .snapshot(
            &SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap(),
            &keymap,
            180,
            30,
        )
        .join("\n");
    assert!(frame.contains("Cli: /tmp/operator-viewer.toml"), "{frame}");
    assert!(
        frame.contains("type=Boolean platform=All scope=NewSurface"),
        "{frame}"
    );
}
