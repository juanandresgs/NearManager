# Far Manager Parity Roadmap

`project/far-parity.toml` is the normative workflow-level parity sheet for the build-matched Far Manager 3 research corpus. It separates reusable platform primitives from finished operator workflows; a provider API or demo surface does not count as feature parity by itself.

## Current Audit

The reconciliation contains 60 workflow items across 12 areas. The remaining partial records are concentrated in direct terminal/operator qualification for panel interaction, menus, settings, shell, associations, temporary panels, viewer, editor, and platform input behavior.

Items use four states:

- `verified`: acceptance is exercised by checked-in implementation evidence.
- `partial`: a primitive or narrow workflow exists, but at least one acceptance clause is absent.
- `missing`: no usable end-to-end workflow exists.
- `out-of-scope`: intentionally excluded with an explicit rationale.

## Delivery Order

1. **Interaction fidelity:** parent navigation, full-screen viewer/editor/shell surfaces, modifier keybar layers, complete menu hierarchy, and live command line.
2. **Daily file-management parity:** sorting, panel modes, selection masks, rename, delete/wipe, links, attributes, comparison, histories, and shortcuts.
3. **Content workflows:** complete viewer, internal editor, advanced search, result panels, descriptions, and apply-command.
4. **Customization and automation:** settings UI, highlighting, user menus, associations, macro management, and advanced configuration.
5. **Provider ecosystem:** archives, temporary panels, remote providers, plugin menus/help/configuration, process/device panels, and elevation adapters.

`python3 tools/validate_far_parity.py` validates structure, state vocabulary, acceptance criteria, area coverage, and evidence paths. Status upgrades require an end-to-end test or demonstration matching every acceptance clause.
