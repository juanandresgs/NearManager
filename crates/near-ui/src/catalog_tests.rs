use std::collections::BTreeMap;

use near_core::{ActionContext, CommandId, CommandInvocation, Location, ProviderId, ResourceRef};

use crate::{
    DialogField, DialogSurface, HelpEntry, HelpSurface, InspectorField, InspectorSurface, MenuItem,
    MenuSurface, RenderContext, SceneRect, SemanticTheme, Surface, SurfaceEvent, TaskRecord,
    TaskState, TaskSurface, TerminalInputMode, TerminalSurface, TreeNode, TreeSurface,
    UpdateContext, ViewerSurface, snapshot_scene,
};

const THEME: &str = include_str!("../../../specs/theme.toml");

fn command(id: &str) -> CommandInvocation {
    CommandInvocation {
        id: CommandId::from(id),
        arguments: BTreeMap::new(),
    }
}

fn render(surface: &dyn Surface, width: u16, height: u16) -> String {
    let action = ActionContext::default();
    let scene = surface.scene(
        SceneRect::new(0, 0, width, height),
        &RenderContext {
            focused: true,
            action: &action,
        },
    );
    let theme = SemanticTheme::from_toml(THEME).unwrap();
    snapshot_scene(&scene, &theme, width, height)
        .text_lines()
        .join("\n")
}

#[test]
fn tree_viewer_and_inspector_support_navigation_and_rendering() {
    let resource = ResourceRef {
        provider: ProviderId::from("test"),
        location: Location::new("test://root"),
    };
    let mut tree = TreeSurface::new(
        "test.tree",
        "Tree",
        vec![TreeNode {
            id: "root".to_owned(),
            label: "root".to_owned(),
            resource: Some(resource.clone()),
            expanded: false,
            children: vec![TreeNode {
                id: "child".to_owned(),
                label: "child".to_owned(),
                resource: None,
                expanded: false,
                children: Vec::new(),
            }],
        }],
    );
    let action = ActionContext::default();
    tree.update(
        &SurfaceEvent::Command(command("near.tree.toggle")),
        &mut UpdateContext { action: &action },
    );
    tree.update(
        &SurfaceEvent::Command(command("near.tree.down")),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(tree.cursor(), 1);
    assert!(render(&tree, 40, 8).contains("child"));

    let mut viewer = ViewerSurface::text("test.viewer", "Viewer", "one\ntwo\nthree")
        .with_resource(resource.clone());
    viewer.update(
        &SurfaceEvent::Command(command("near.viewer.down")),
        &mut UpdateContext { action: &action },
    );
    viewer.update(
        &SurfaceEvent::Command(command("near.viewer.toggle-hex")),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(viewer.scroll(), 1);
    assert!(viewer.is_hex());
    assert!(render(&viewer, 50, 8).contains("00000004"));

    let inspector = InspectorSurface::new(
        "test.inspector",
        "Inspector",
        Some(resource),
        vec![
            InspectorField {
                label: "Kind".to_owned(),
                value: "Virtual".to_owned(),
                warning: false,
            },
            InspectorField {
                label: "Quarantine".to_owned(),
                value: "present".to_owned(),
                warning: true,
            },
        ],
    );
    let screen = render(&inspector, 50, 8);
    assert!(screen.contains("Virtual"));
    assert!(screen.contains("Quarantine"));
}

#[test]
fn menu_dialog_help_and_tasks_return_semantic_effects() {
    let action = ActionContext::default();
    let mut menu = MenuSurface::new(
        "test.menu",
        "Commands",
        vec![MenuItem {
            label: "Inspect".to_owned(),
            description: "Open metadata".to_owned(),
            command: command("test.inspect"),
            enabled: true,
        }],
    );
    let result = menu.update(
        &SurfaceEvent::Command(command("near.menu.activate")),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(result.command.unwrap().id, CommandId::from("test.inspect"));
    assert!(render(&menu, 50, 8).contains("Open metadata"));

    let mut dialog = DialogSurface::new(
        "test.dialog",
        "Create",
        vec![DialogField {
            id: "name".to_owned(),
            label: "Name".to_owned(),
            value: String::new(),
            required: true,
            secret: false,
        }],
        command("test.create"),
        command("test.cancel"),
    );
    let missing = dialog.update(
        &SurfaceEvent::Command(command("near.dialog.accept")),
        &mut UpdateContext { action: &action },
    );
    assert!(missing.command.is_none());
    assert!(render(&dialog, 50, 8).contains("required"));
    dialog.update(
        &SurfaceEvent::Text("report".to_owned()),
        &mut UpdateContext { action: &action },
    );
    let accepted = dialog.update(
        &SurfaceEvent::Command(command("near.dialog.accept")),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(accepted.command.unwrap().id, CommandId::from("test.create"));

    let help = HelpSurface::new(
        "test.help",
        "Help",
        "Generated bindings",
        vec![HelpEntry {
            keys: "F1".to_owned(),
            command: "near.help.context".to_owned(),
            description: "Help".to_owned(),
        }],
    );
    assert!(render(&help, 70, 8).contains("near.help.context"));

    let mut tasks = TaskSurface::new(
        "test.tasks",
        vec![TaskRecord {
            id: "copy-1".to_owned(),
            title: "Copy".to_owned(),
            state: TaskState::Running,
            completed: 4,
            total: Some(10),
            message: "documents".to_owned(),
        }],
    );
    let cancellation = tasks.update(
        &SurfaceEvent::Command(command("near.tasks.cancel")),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(
        cancellation.command.unwrap().id,
        CommandId::from("near.task.cancel")
    );
    assert!(render(&tasks, 70, 8).contains("4/10"));
}

#[test]
fn terminal_surface_models_scrollback_modes_and_input_effects() {
    let action = ActionContext::default();
    let mut terminal = TerminalSurface::new("test.terminal", "zsh", 3);
    terminal.append_output("one\ntwo\nthree\nfour");
    assert_eq!(terminal.lines(), ["two", "three", "four"]);
    terminal.set_cursor(Some((2, 1)));
    let input = terminal.update(
        &SurfaceEvent::Text("ls".to_owned()),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(
        input.command.unwrap().id,
        CommandId::from("near.terminal.input")
    );
    terminal.update(
        &SurfaceEvent::Command(command("near.terminal.copy-mode")),
        &mut UpdateContext { action: &action },
    );
    assert_eq!(terminal.mode(), TerminalInputMode::Copy);
    assert!(
        terminal
            .update(
                &SurfaceEvent::Text("ignored".to_owned()),
                &mut UpdateContext { action: &action }
            )
            .command
            .is_none()
    );
    let screen = render(&terminal, 50, 8);
    assert!(screen.contains("zsh [Copy]"));
    assert!(screen.contains("four"));
}
