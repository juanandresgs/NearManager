#[cfg(test)]
mod tests {
    use near_ui::{ColumnAlignment, PanelColumnKind, PanelModeCatalog, PanelModeError};

    #[test]
    fn builtins_and_custom_modes_merge_with_independent_defaults() {
        let catalog = PanelModeCatalog::from_toml(
            r#"
                schema = 1
                [defaults]
                left = "compact"
                right = "audit"

                [[modes]]
                id = "audit"
                label = "Audit"

                [[modes.columns]]
                kind = "name"
                width = 30
                alignment = "right"

                [[modes.columns]]
                kind = "permissions"
                width = 10
            "#,
        )
        .unwrap();

        assert_eq!(catalog.left_default(), "compact");
        assert_eq!(catalog.right_default(), "audit");
        assert!(catalog.mode("medium").is_some());
        let audit = catalog.mode("audit").unwrap();
        assert_eq!(audit.columns[0].kind, PanelColumnKind::Name);
        assert_eq!(audit.columns[0].width, Some(30));
        assert_eq!(audit.columns[0].alignment, ColumnAlignment::Right);
        assert_eq!(
            PanelModeCatalog::from_toml(&catalog.to_toml()).unwrap(),
            catalog
        );
    }

    #[test]
    fn unknown_defaults_fail_closed() {
        assert_eq!(
            PanelModeCatalog::from_toml(
                "schema = 1\n[defaults]\nleft = \"missing\"\nright = \"medium\"\n"
            ),
            Err(PanelModeError::UnknownDefault("missing".to_owned()))
        );
    }
}
