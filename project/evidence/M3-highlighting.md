# M3 Highlighting and Sort-Group Evidence

Date: 2026-06-23

## Implemented Slice

- `highlighting.toml` is a layered, versioned configuration document with CLI and environment overrides.
- Rules combine name masks, resource kinds, sizes, modified dates, hidden state, read-only state, and executable state through the shared provider-neutral predicate model.
- Stable rule IDs support editable priority and parent inheritance. Parent and child predicates compose; child roles, marks, and sort groups override inherited values.
- Panel rows render rule-selected semantic roles and one-cell marks while focused and selected roles retain interaction visibility.
- Shift+F11 toggles rule-defined sort groups. The complete sort menu exposes the same toggle, and F9 includes an effective highlighting report with priorities and inherited predicate counts.
- Configuration rejects unsupported schemas, duplicate or empty IDs, unknown parents, cycles, content predicates, invalid predicates, and rules without a decoration.

## Verification

- `highlighting::tests::priority_inheritance_and_attributes_resolve_one_effective_decoration`
- `highlighting::tests::invalid_parents_cycles_and_empty_rules_fail_closed`
- `highlighting::tests::shipped_highlighting_catalog_is_valid`
- `collection::tests::highlighting_marks_roles_and_sort_groups_apply_without_hiding_focus`
- `workspace::tests::every_shipped_binding_resolves_to_a_registered_command`
