#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_core::{ListingGeneration, SafetyClass};
    use near_ops::{OperationKind, OperationPlanner, PlanPolicies, PlanRequest, PlannedItem};

    use near_ui::{ConfirmationPolicy, ConfirmationPolicyError};

    fn plan(safety: SafetyClass, high_impact: bool) -> near_ops::OperationPlan {
        OperationPlanner::default()
            .plan(PlanRequest {
                kind: OperationKind::Trash,
                items: vec![PlannedItem {
                    source: Some(near_core::ResourceRef {
                        provider: "near.test".into(),
                        location: near_core::Location::new("test:///item"),
                    }),
                    target: near_core::Location::new("test:///item"),
                    conflict_expected: false,
                    recursive: high_impact,
                    parameters: BTreeMap::default(),
                }],
                destination: None,
                policies: PlanPolicies::default(),
                safety,
                context_generation: ListingGeneration(1),
                high_impact,
            })
            .unwrap()
    }

    #[test]
    fn expert_policy_can_skip_only_lower_impact_previews() {
        let policy = ConfirmationPolicy::from_toml(
            r#"
                schema = 1
                [confirmations]
                reversible = "execute"
                confirmable = "execute"
            "#,
        )
        .unwrap();
        assert!(!policy.requires_preview(&plan(SafetyClass::Reversible, false)));
        assert!(!policy.requires_preview(&plan(SafetyClass::Confirmable, false)));
        assert!(policy.requires_preview(&plan(SafetyClass::Destructive, false)));
        assert!(policy.requires_preview(&plan(SafetyClass::Confirmable, true)));
    }

    #[test]
    fn mandatory_safeguards_cannot_be_disabled() {
        let error = ConfirmationPolicy::from_toml(
            r#"
                schema = 1
                [confirmations]
                destructive = "execute"
            "#,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ConfirmationPolicyError::MandatorySafeguard("destructive")
        ));
    }

    #[test]
    fn policy_round_trip_preserves_editable_choices_and_mandatory_safeguards() {
        let mut policy = ConfirmationPolicy::default();
        policy.set_reversible_preview(false);
        let encoded = policy.to_toml().unwrap();
        assert!(encoded.contains("destructive = \"preview\""));
        assert_eq!(ConfirmationPolicy::from_toml(&encoded).unwrap(), policy);
    }
}
