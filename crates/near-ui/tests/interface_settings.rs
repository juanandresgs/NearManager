use std::collections::BTreeMap;

use near_core::{ActionContext, CommandInvocation};
use near_ui::{
    DialogField, DialogSurface, InterfaceSettings, MenuItem, MenuSurface, RenderContext, SceneRect,
    SemanticTheme, Surface, SurfaceEvent, TreeNode, TreeSurface, UpdateContext, snapshot_scene,
};

fn command(id: &str) -> CommandInvocation {
    CommandInvocation {
        id: id.into(),
        arguments: BTreeMap::new(),
    }
}

#[test]
fn interface_document_validates_schema_and_tree_range() {
    assert_eq!(
        InterfaceSettings::from_toml(include_str!("../../../specs/interface.toml")).unwrap(),
        InterfaceSettings::default()
    );
    assert!(InterfaceSettings::from_toml("schema = 2").is_err());
    assert!(InterfaceSettings::from_toml("schema = 1\ntree_indent_width = 0").is_err());
}

#[test]
fn menu_and_dialog_navigation_follow_runtime_policy() {
    let action = ActionContext::default();
    let mut context = UpdateContext { action: &action };
    let mut menu = MenuSurface::new(
        "menu",
        "Menu",
        vec![
            MenuItem {
                label: "One".into(),
                description: String::new(),
                command: command("one"),
                enabled: true,
            },
            MenuItem {
                label: "Two".into(),
                description: String::new(),
                command: command("two"),
                enabled: true,
            },
        ],
    );
    menu.configure_interaction(true, true);
    menu.update(
        &SurfaceEvent::Command(command("near.menu.up")),
        &mut context,
    );
    assert_eq!(menu.selected(), 1);

    let fields = vec![
        DialogField {
            id: "first".into(),
            label: "First".into(),
            value: String::new(),
            required: false,
            secret: false,
        },
        DialogField {
            id: "last".into(),
            label: "Last".into(),
            value: String::new(),
            required: false,
            secret: false,
        },
    ];
    let mut dialog = DialogSurface::new(
        "dialog",
        "Dialog",
        fields.clone(),
        command("ok"),
        command("cancel"),
    );
    dialog.configure_interaction(false, false);
    dialog.update(
        &SurfaceEvent::Command(command("near.dialog.previous")),
        &mut context,
    );
    dialog.update(&SurfaceEvent::Text("x".into()), &mut context);
    assert_eq!(dialog.values()["first"], "x");
    let mut dialog =
        DialogSurface::new("dialog", "Dialog", fields, command("ok"), command("cancel"));
    dialog.configure_interaction(false, true);
    dialog.update(
        &SurfaceEvent::Command(command("near.dialog.previous")),
        &mut context,
    );
    dialog.update(&SurfaceEvent::Text("x".into()), &mut context);
    assert_eq!(dialog.values()["last"], "x");
}

#[test]
fn tree_indentation_changes_semantic_output() {
    let tree = TreeSurface::new(
        "tree",
        "Tree",
        vec![TreeNode {
            id: "root".into(),
            label: "root".into(),
            resource: None,
            expanded: true,
            children: vec![TreeNode {
                id: "child".into(),
                label: "child".into(),
                resource: None,
                expanded: false,
                children: Vec::new(),
            }],
        }],
    )
    .with_indent_width(4);
    let scene = tree.scene(
        SceneRect::new(0, 0, 40, 8),
        &RenderContext {
            focused: true,
            action: &ActionContext::default(),
        },
    );
    let snapshot = snapshot_scene(
        &scene,
        &SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap(),
        40,
        8,
    );
    assert!(snapshot.text_lines().join("\n").contains("    child"));
}
