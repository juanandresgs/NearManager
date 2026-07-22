use near_app::{
    CancellationToken, CollectionEntry, CollectionSurface, ListRequest, ListingGeneration,
    Location, ResourceProvider, SceneRect, SemanticTheme, SurfaceShell, block_on, snapshot_scene,
};
use near_reference_providers::{PluginCatalogProvider, PluginItem, ProcessProvider};

const THEME: &str = include_str!("../../../specs/theme-terminal-native.toml");

fn provider_surface(
    provider: &dyn ResourceProvider,
    location: Location,
    id: &str,
    title: &str,
) -> CollectionSurface {
    let page = block_on(provider.list(
        &location,
        ListRequest {
            generation: ListingGeneration(1),
            continuation: None,
            page_size: 256,
            cancellation: CancellationToken::default(),
        },
    ))
    .expect("reference provider should list its root");
    CollectionSurface::new(
        id,
        "near-demo.collection",
        title,
        location,
        page.entries
            .into_iter()
            .map(|entry| CollectionEntry {
                resource: entry.resource,
                metadata: entry.metadata,
                details: entry.details,
                selected: false,
            })
            .collect(),
    )
}

fn plugin_provider() -> PluginCatalogProvider {
    PluginCatalogProvider::new(vec![
        PluginItem {
            id: "near.archive".to_owned(),
            name: "Archive Provider".to_owned(),
            version: "0.1.0".to_owned(),
            description: "Browsable archive resources".to_owned(),
        },
        PluginItem {
            id: "near.git".to_owned(),
            name: "Git Provider".to_owned(),
            version: "0.1.0".to_owned(),
            description: "Repository status resources".to_owned(),
        },
    ])
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--help" | "-h") => {
            println!("usage: near-demo");
            return;
        }
        Some("--version") => {
            println!("near-demo {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(argument) => {
            eprintln!("unknown near-demo argument: {argument}");
            std::process::exit(2);
        }
        None => {}
    }
    let processes = ProcessProvider::local();
    let plugins = plugin_provider();
    let shell = SurfaceShell::focused_peer(
        provider_surface(
            &processes,
            ProcessProvider::root(),
            "near-demo.processes",
            "Processes",
        ),
        provider_surface(
            &plugins,
            PluginCatalogProvider::root(),
            "near-demo.plugins",
            "Plugins",
        ),
    );
    let scene = shell.scene(SceneRect::new(0, 0, 100, 16));
    let theme = SemanticTheme::from_toml(THEME).expect("shipped theme is valid");
    let snapshot = snapshot_scene(&scene, &theme, 100, 16);
    for line in snapshot.text_lines() {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_app::{
        ActionContext, ArgumentKind, ArgumentSchema, CollectionLookupMode, CommandDescriptor,
        CommandId, CommandInvocation, EditorSettings, MenuItem, MenuSurface, PaneSlot,
        ResourceOpenPolicy, SafetyClass, Scene, SceneColor, ScenePrimitive, SceneTextStyle,
        ShellClosePolicy, ShellMode, ShellProfile, Surface, SurfaceEvent, TabRegistry, TaskRecord,
        TaskState, TaskSurface, UpdateContext, ViewerEncoding, ViewerSettings, ViewerStateEntry,
        ZoomablePanePresentation,
    };
    use near_reference_providers::ProcessRecord;

    use super::*;

    #[test]
    fn public_scene_contract_supports_terminal_cell_styles() {
        let mut scene = Scene::new();
        scene.styled_text(
            SceneRect::new(0, 0, 2, 1),
            "界",
            "terminal.text",
            SceneTextStyle {
                foreground: Some(SceneColor::Indexed(1)),
                bold: true,
                ..SceneTextStyle::default()
            },
        );
        assert!(matches!(
            &scene.primitives()[0],
            ScenePrimitive::StyledText { area, style, .. }
                if area.width == 2
                    && style.foreground == Some(SceneColor::Indexed(1))
                    && style.bold
        ));
    }

    #[test]
    fn public_collection_lookup_supports_internal_match_spans() {
        let provider = plugin_provider();
        let mut surface = provider_surface(
            &provider,
            PluginCatalogProvider::root(),
            "test.lookup",
            "Plugins",
        );
        surface.set_lookup(Some("rchive".to_owned()), CollectionLookupMode::Contains);
        let scene = surface.scene(
            SceneRect::new(0, 0, 60, 8),
            &near_app::RenderContext {
                focused: true,
                action: &ActionContext::default(),
            },
        );
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { content, role, .. }
                if content == "rchive" && role.as_str() == "lookup.match.focused"
        )));
    }

    #[test]
    fn public_tab_and_zoom_models_support_terminal_workspace_composition() {
        let mut tabs = TabRegistry::default();
        let first = tabs.insert("Agent One", "session-one");
        let second = tabs.insert("Agent Two", "session-two");
        assert_eq!(tabs.active_id(), Some(second));
        tabs.select(first);
        assert_eq!(tabs.active().unwrap().value(), &"session-one");

        let mut presentation = ZoomablePanePresentation::default();
        presentation.place(PaneSlot::Second);
        presentation.toggle_zoom();
        assert!(presentation.is_full_screen());
        presentation.toggle_zoom();
        assert_eq!(presentation.pane(), Some(PaneSlot::Second));
    }

    #[test]
    fn mixed_non_filesystem_workspace_uses_shared_surface_contracts() {
        let processes = ProcessProvider::new(vec![
            ProcessRecord {
                pid: 10,
                cpu: "1.0".to_owned(),
                command: "alpha".to_owned(),
            },
            ProcessRecord {
                pid: 20,
                cpu: "2.0".to_owned(),
                command: "beta".to_owned(),
            },
        ]);
        let plugins = plugin_provider();
        let mut shell = SurfaceShell::focused_peer(
            provider_surface(
                &processes,
                ProcessProvider::root(),
                "test.processes",
                "Processes",
            ),
            provider_surface(
                &plugins,
                PluginCatalogProvider::root(),
                "test.plugins",
                "Plugins",
            ),
        );
        let process = shell.action_context().current.unwrap();
        assert_eq!(process.provider.as_str(), "near.process");
        assert!(shell.focus_peer());
        let plugin = shell.action_context().current.unwrap();
        assert_eq!(plugin.provider.as_str(), "near.plugin-catalog");
    }

    #[test]
    fn mixed_workspace_snapshot_renders_both_provider_domains() {
        let processes = ProcessProvider::new(vec![ProcessRecord {
            pid: 10,
            cpu: "1.0".to_owned(),
            command: "alpha".to_owned(),
        }]);
        let plugins = plugin_provider();
        let shell = SurfaceShell::focused_peer(
            provider_surface(
                &processes,
                ProcessProvider::root(),
                "test.processes",
                "Processes",
            ),
            provider_surface(
                &plugins,
                PluginCatalogProvider::root(),
                "test.plugins",
                "Plugins",
            ),
        );
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let snapshot = snapshot_scene(&shell.scene(SceneRect::new(0, 0, 100, 16)), &theme, 100, 16)
            .text_lines()
            .join("\n");
        assert!(snapshot.contains("Processes"));
        assert!(snapshot.contains("Plugins"));
        assert!(snapshot.contains("alpha"));
        assert!(snapshot.contains("Archive Pro"));
    }

    #[test]
    fn public_menu_contract_dispatches_disabled_items_for_application_denials() {
        let mut menu = MenuSurface::new(
            "near-demo.menu",
            "Process Actions",
            vec![MenuItem {
                label: "&Terminate".to_owned(),
                description: "Unavailable: process is protected".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near-demo.process.terminate"),
                    arguments: BTreeMap::new(),
                },
                enabled: false,
            }],
        );
        assert_eq!(menu.title(), "Process Actions");
        assert_eq!(menu.items().len(), 1);

        let action = ActionContext::default();
        let result = menu.update(
            &SurfaceEvent::Text("t".to_owned()),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(
            result.command.unwrap().id.as_str(),
            "near-demo.process.terminate"
        );
    }

    #[test]
    fn public_shell_profile_contract_is_application_neutral() {
        let profile = ShellProfile::from_toml(
            "schema = 1\nprogram = '/bin/sh'\nmode = 'clean'\nclose_policy = 'keep-open'\n",
        )
        .unwrap();
        let resolved = profile.resolve();
        assert_eq!(resolved.mode, ShellMode::Clean);
        assert_eq!(resolved.close_policy, ShellClosePolicy::KeepOpen);
        assert!(resolved.close_policy.keeps_process_on_close());
        assert!(!resolved.close_policy.closes_on_exit());
    }

    #[test]
    fn public_task_history_contract_is_application_neutral() {
        let tasks = TaskSurface::new(
            "near-demo.tasks",
            vec![TaskRecord {
                id: "1".to_owned(),
                title: "Refresh process catalog".to_owned(),
                state: TaskState::Completed,
                completed: 1,
                total: Some(1),
                message: "Done".to_owned(),
            }],
        );
        assert_eq!(tasks.tasks()[0].title, "Refresh process catalog");
        assert_eq!(tasks.tasks()[0].state, TaskState::Completed);
    }

    #[test]
    fn public_command_discoverability_contract_is_application_neutral() {
        let descriptor = CommandDescriptor {
            id: CommandId::from("near-demo.process.signal"),
            title: "Signal process".to_owned(),
            description: "Signal a selected process".to_owned(),
            category: vec!["Process".to_owned()],
            safety: SafetyClass::Confirmable,
            arguments: BTreeMap::from([(
                "signal".to_owned(),
                ArgumentSchema {
                    kind: ArgumentKind::Integer,
                    required: true,
                    description: "Signal number".to_owned(),
                    default: None,
                },
            )]),
        };
        assert!(!descriptor.invokable_without_arguments());
    }

    #[test]
    fn public_document_policy_contract_is_application_neutral() {
        let editor = EditorSettings::from_toml(
            "schema = 1\ntab_size = 8\nexpand_tabs = true\nopen_policy = 'association'\n",
        )
        .unwrap();
        assert_eq!(editor.tab_size, 8);
        assert!(editor.expand_tabs);
        assert_eq!(editor.open_policy, ResourceOpenPolicy::Association);
        assert_eq!(
            ViewerSettings::default().open_policy,
            ResourceOpenPolicy::Internal
        );
        assert_eq!(ViewerSettings::default().encoding, ViewerEncoding::Auto);
        let viewer = ViewerSettings {
            remember_position: false,
            ..ViewerSettings::default()
        };
        let state = viewer
            .filter_state(ViewerStateEntry {
                provider: "near-demo".into(),
                location: Location::new("demo://resource"),
                offset: 42,
                bookmarks: BTreeMap::new(),
                navigation_history: vec![0, 42],
                navigation_index: 1,
                encoding: Some("utf-16le".to_owned()),
                wrap: Some(true),
                hex: Some(false),
            })
            .unwrap();
        assert_eq!(state.offset, 0);
        assert_eq!(state.encoding.as_deref(), Some("utf-16le"));
    }
}
