#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use near_core::{CommandId, CommandInvocation, CommandValue, ContextId};
    use near_terminal::{KeyKind, Modifiers, TerminalEvent};

    use near_ui::{
        FarWorkspace, Keymap, KeymapSettings, ResolveResult, SemanticTheme, SurfaceApplication,
        ViewerSurface, WorkspaceAction, format_key_sequence, parse_key_stroke,
    };

    #[test]
    fn contextual_help_prioritizes_active_surface_bindings_over_inherited_bindings() {
        let mut application = SurfaceApplication::new(
            "test.viewer",
            "Viewer",
            ViewerSurface::text("viewer", "Document", "alpha\n"),
        );
        let keymap = Keymap::from_toml(
            r#"
[[context]]
id = "global"

[[context.bindings]]
on = "F1"
run = "near.help.context"
description = "Help"

[[context.bindings]]
on = "F2"
run = "global.second"
description = "Second global command"

[[context]]
id = "surface.viewer"
inherits = ["global"]

[[context.bindings]]
on = "Enter"
run = "viewer.contextual"
description = "Contextual command"
"#,
        )
        .unwrap();
        let theme = SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap();

        application.dispatch(
            &CommandInvocation {
                id: CommandId::from("near.help.context"),
                arguments: BTreeMap::new(),
            },
            &keymap,
        );

        let snapshot = application.snapshot(&theme, 90, 7).text_lines().join("\n");
        assert!(snapshot.contains("viewer.contextual"), "{snapshot}");
        assert!(!snapshot.contains("global.second"), "{snapshot}");
    }

    #[test]
    fn child_context_overrides_parent_and_can_remove_binding() {
        let source = r#"
            [[context]]
            id = "global"
            [[context.bindings]]
            on = "F1"
            run = "global.help"
            [[context.bindings]]
            on = "F2"
            run = "global.other"

            [[context]]
            id = "panel"
            inherits = ["global"]
            [[context.bindings]]
            on = "F1"
            run = "panel.help"
            [[context.removals]]
            on = "F2"
        "#;
        let mut keymap = Keymap::from_toml(source).unwrap();
        let context = [ContextId::from("panel")];
        let result = keymap.resolve(&context, parse_key_stroke("F1").unwrap());
        let ResolveResult::Matched(invocation) = result else {
            panic!("expected binding");
        };
        assert_eq!(invocation.id, CommandId::from("panel.help"));
        assert!(matches!(
            keymap.resolve(&context, parse_key_stroke("F2").unwrap()),
            ResolveResult::NoMatch
        ));
    }

    #[test]
    fn resolves_typed_parameterized_binding() {
        let source = r#"
            [[context]]
            id = "global"
            [[context.bindings]]
            on = "Down"
            run = { command = "near.move", args = { rows = 2, wrap = true, label = "next" } }
        "#;
        let mut keymap = Keymap::from_toml(source).unwrap();
        let result = keymap.resolve(
            &[ContextId::from("global")],
            parse_key_stroke("Down").unwrap(),
        );
        let ResolveResult::Matched(invocation) = result else {
            panic!("expected binding");
        };
        assert_eq!(invocation.arguments["rows"], CommandValue::Integer(2));
        assert_eq!(invocation.arguments["wrap"], CommandValue::Boolean(true));
        assert_eq!(
            invocation.arguments["label"],
            CommandValue::String("next".to_owned())
        );
    }

    #[test]
    fn repeat_events_resolve_the_same_semantic_binding_as_press_events() {
        let source = r#"
            [[context]]
            id = "panel"
            [[context.bindings]]
            on = "Down"
            run = { command = "near.collection.move", args = { rows = 1 } }
        "#;
        let mut keymap = Keymap::from_toml(source).unwrap();
        let context = [ContextId::from("panel")];
        let mut repeated = parse_key_stroke("Down").unwrap();
        repeated.kind = KeyKind::Repeat;

        let ResolveResult::Matched(invocation) = keymap.resolve(&context, repeated) else {
            panic!("expected repeated key to resolve");
        };
        assert_eq!(invocation.id, CommandId::from("near.collection.move"));
        assert_eq!(invocation.arguments["rows"], CommandValue::Integer(1));
    }

    #[test]
    fn repeat_policy_protects_actions_and_preserves_pending_sequences() {
        let source = r#"
            [[context]]
            id = "panel"
            [[context.bindings]]
            on = "Down"
            run = "near.collection.move"
            [[context.bindings]]
            on = "F8"
            run = "near.resource.delete"
            [[context.bindings]]
            on = "F7"
            run = "near.dialog.open"
            repeatable = true
            [[context.bindings]]
            on = ["g", "g"]
            run = "near.collection.first"
        "#;
        let mut keymap = Keymap::from_toml(source).unwrap();
        let context = [ContextId::from("panel")];

        let mut delete_repeat = parse_key_stroke("F8").unwrap();
        delete_repeat.kind = KeyKind::Repeat;
        assert_eq!(
            keymap.resolve(&context, delete_repeat),
            ResolveResult::NoMatch
        );

        let mut opted_in_repeat = parse_key_stroke("F7").unwrap();
        opted_in_repeat.kind = KeyKind::Repeat;
        let ResolveResult::Matched(invocation) = keymap.resolve(&context, opted_in_repeat) else {
            panic!("expected explicitly repeatable binding to resolve");
        };
        assert_eq!(invocation.id, CommandId::from("near.dialog.open"));

        assert!(matches!(
            keymap.resolve(&context, parse_key_stroke("g").unwrap()),
            ResolveResult::Pending { .. }
        ));
        let mut navigation_repeat = parse_key_stroke("Down").unwrap();
        navigation_repeat.kind = KeyKind::Repeat;
        assert_eq!(
            keymap.resolve(&context, navigation_repeat),
            ResolveResult::NoMatch
        );
        let ResolveResult::Matched(invocation) =
            keymap.resolve(&context, parse_key_stroke("g").unwrap())
        else {
            panic!("expected pending sequence to survive an unrelated repeat");
        };
        assert_eq!(invocation.id, CommandId::from("near.collection.first"));
    }

    #[test]
    fn function_hints_follow_the_exact_held_modifiers() {
        let source = r#"
            [[context]]
            id = "panel"
            [[context.bindings]]
            on = "F3"
            run = "near.view"
            description = "View"
            hint = { group = "function", slot = 3 }
            [[context.bindings]]
            on = "Alt+F3"
            run = "near.alt-view"
            description = "Alternate view"
            hint = { group = "function", slot = 3 }
        "#;
        let keymap = Keymap::from_toml(source).unwrap();
        let context = [ContextId::from("panel")];
        let base = keymap.function_hints_for_modifiers(&context, Modifiers::default());
        assert_eq!(base[0].1.description.as_deref(), Some("View"));
        let alt = keymap.function_hints_for_modifiers(
            &context,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        );
        assert_eq!(alt[0].1.description.as_deref(), Some("Alternate view"));
    }

    #[test]
    fn sequence_timeout_uses_injected_time() {
        let source = r#"
            [settings]
            sequence_timeout_ms = 500
            [[context]]
            id = "global"
            [[context.bindings]]
            on = "g"
            run = "near.go"
            [[context.bindings]]
            on = ["g", "g"]
            run = "near.first"
        "#;
        let mut keymap = Keymap::from_toml(source).unwrap();
        let context = [ContextId::from("global")];
        let result = keymap.resolve_at(&context, parse_key_stroke("g").unwrap(), Duration::ZERO);
        let ResolveResult::Pending {
            sequence,
            continuations,
        } = result
        else {
            panic!("expected pending sequence");
        };
        assert_eq!(format_key_sequence(&sequence), "g");
        assert_eq!(format_key_sequence(&continuations), "g");
        assert!(matches!(
            keymap.expire_pending_at(Duration::from_millis(499)),
            ResolveResult::NoMatch
        ));
        let ResolveResult::Matched(invocation) =
            keymap.expire_pending_at(Duration::from_millis(500))
        else {
            panic!("expected timeout fallback");
        };
        assert_eq!(invocation.id, CommandId::from("near.go"));
    }

    #[test]
    fn reports_conflicts_with_context_and_source() {
        let source = r#"
            [[context]]
            id = "global"
            [[context.bindings]]
            on = "F1"
            run = "near.first"
            [[context.bindings]]
            on = "F1"
            run = "near.second"
        "#;
        let keymap = Keymap::from_toml_named("user.toml", source).unwrap();
        let conflict = &keymap.conflicts()[0];
        assert_eq!(conflict.first.source, "user.toml");
        assert_eq!(conflict.first.context, ContextId::from("global"));
        assert_eq!(format_key_sequence(&conflict.sequence), "F1");
    }

    #[test]
    fn rejects_inheritance_cycles() {
        let source = r#"
            [[context]]
            id = "one"
            inherits = ["two"]
            [[context]]
            id = "two"
            inherits = ["one"]
        "#;
        assert!(Keymap::from_toml(source).is_err());
    }

    #[test]
    fn first_active_context_has_highest_precedence() {
        let source = r#"
            [[context]]
            id = "workspace"
            [[context.bindings]]
            on = "Enter"
            run = "workspace.open"

            [[context]]
            id = "dialog"
            [[context.bindings]]
            on = "Enter"
            run = "dialog.accept"
        "#;
        let mut keymap = Keymap::from_toml(source).unwrap();
        let result = keymap.resolve(
            &[ContextId::from("dialog"), ContextId::from("workspace")],
            parse_key_stroke("Enter").unwrap(),
        );
        let ResolveResult::Matched(invocation) = result else {
            panic!("expected binding");
        };
        assert_eq!(invocation.id, CommandId::from("dialog.accept"));
    }

    #[test]
    fn validates_parent_and_function_key_ranges() {
        let unknown_parent = r#"
            [[context]]
            id = "panel"
            inherits = ["missing"]
        "#;
        assert!(Keymap::from_toml(unknown_parent).is_err());
        assert!(parse_key_stroke("F25").is_err());
        assert_eq!(
            parse_key_stroke("Ctrl+Shift+F").unwrap().key,
            near_terminal::Key::Character('f')
        );
    }

    #[test]
    fn settings_rewrite_preserves_bindings_and_controls_pending_display() {
        let source = r#"
            [settings]
            sequence_timeout_ms = 700
            show_pending_sequence = true
            prefer_physical_keys = false
            [[context]]
            id = "workspace.panel"
            [[context.bindings]]
            on = ["g", "g"]
            run = "near.panel.first"
        "#;
        let settings = KeymapSettings {
            sequence_timeout: Duration::from_millis(250),
            show_pending_sequence: false,
            prefer_physical_keys: false,
        };
        let rewritten = Keymap::rewrite_settings_toml(source, settings).unwrap();
        let mut keymap = Keymap::from_toml(&rewritten).unwrap();
        assert_eq!(*keymap.settings(), settings);
        let action = FarWorkspace::demo().handle_terminal_event(
            &mut keymap,
            near_terminal::TerminalEvent::Key(parse_key_stroke("g").unwrap()),
        );
        assert_eq!(action, WorkspaceAction::PendingSequence(String::new()));
        assert!(rewritten.contains("near.panel.first"));
    }

    #[test]
    fn unsupported_physical_key_preference_fails_closed() {
        let source = "[settings]\nprefer_physical_keys = true\n";
        assert!(Keymap::from_toml(source).is_err());
    }

    #[test]
    fn active_command_line_text_precedes_bindings() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(include_str!("../../../specs/keymap.toml")).unwrap();
        for key_name in [
            "c", "d", "Space", "f", "o", "l", "d", "e", "r", "-", "s", "a", "m", "p", "l", "e",
        ] {
            workspace.handle_terminal_event(
                &mut keymap,
                TerminalEvent::Key(parse_key_stroke(key_name).unwrap()),
            );
        }
        let frame = workspace
            .snapshot(
                &near_ui::SemanticTheme::from_toml(include_str!("../../../specs/theme.toml"))
                    .unwrap(),
                &keymap,
                100,
                30,
            )
            .join("\n");
        assert!(frame.contains("cd folder-sample"), "{frame}");
        assert!(!frame.contains('√'), "{frame}");
    }

    #[test]
    fn bound_alt_chord_does_not_start_filename_lookup() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(include_str!("../../../specs/keymap.toml")).unwrap();
        let action = workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(parse_key_stroke("Alt+Left").unwrap()),
        );
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.collection.scroll-horizontal"
        ));
        let frame = workspace.snapshot(
            &near_ui::SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap(),
            &keymap,
            100,
            30,
        );
        assert!(!frame.join("\n").contains("Find:"));
    }
}
