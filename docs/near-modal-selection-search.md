# Modal Selection Search

Selectable Near modals share one Alt+typing search convention.

- Hold Alt/Option and type anywhere in an open selectable modal to open its filter and build a case-insensitive selection query; the filter does not take a separate focus step.
- `Alt+Backspace` removes the last query character.
- Up and Down move only through matching rows.
- Enter activates the visibly selected match.
- The filter row is hidden until the first Alt+character, then renders the active query at the bottom of the modal.
- Matching text in labels, descriptions, shortcuts, locations, field names, task details, and help results uses the semantic `selection.match` highlight role.
- Ordinary menu accelerator typing remains separate: an unmodified accelerator can activate its command, while Alt+typing only searches and never triggers the accelerator.

The shared behavior applies to menus, the command palette, command/folder/resource histories, help topics, dialog fields, and task rows. Search matches the meaningful labels and supporting text for each surface: menu descriptions, command IDs and shortcuts, resource locations, dialog field IDs, task messages and states, and help topic contents.

Non-selectable messages and operation previews do not invent row selection. Full-screen editor, viewer, terminal, tree, and collection surfaces retain their own domain-specific search commands.

The workspace intercepts Alt-character and Alt-Backspace events while a modal is open before normal keymap dispatch. This keeps modal selection search consistent even when the same Alt chord has a panel-context binding. Each surface owns its query and filtered projection, so rendering, arrow movement, mouse-visible menu rows, and Enter activation use the same source indices.
