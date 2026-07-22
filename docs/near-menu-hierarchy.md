# Near F9 Menu Hierarchy

Near's F9 menu follows Far Manager's five-part top-level organization while dispatching the same typed commands used by key bindings, the command palette, macros, and extensions.

## Opening and Switching

- `F9` opens the active panel's **Left** or **Right** submenu immediately; it does not open an extra category chooser.
- A persistent `Left | Files | Commands | Options | Right` bar owns the top screen row, matching Far rather than appearing inside a centered dialog.
- The active category's submenu drops from row 1 beneath that category and clamps horizontally only when terminal width requires it.
- `Left` and `Right` move through adjacent top-level categories without closing the menu.
- `Tab` and `Shift+Tab` switch directly between the panel-specific Left and Right menus.
- Clicking a category in the top row switches to its anchored submenu using the same command route as keyboard navigation.
- Choosing Left or Right explicitly targets that named panel.

## Canonical Groups

- **Left / Right** begin with directly selectable Brief, Medium, Full, Wide, and Detailed layouts, then panel types, sorting, filtering, re-read, location, and custom-layout configuration.
- **Files** follows Far's view, edit, copy, rename/move, link, folder creation, deletion, archive, attributes, apply-command, description, and selection sequence. Reversible Trash and in-place Rename are Near-native additions kept beside their canonical counterparts.
- **Commands** follows search and histories with panel swapping, folder comparison, user menus, associations, filtering, extension commands, screens, tasks, and devices. Saved search panels, macros, and the persistent terminal follow as Near extensions.
- **Options** exposes Far-shaped system, panel, tree, interface, dialog, menu, command-line, completion, confirmation, panel-mode, description, viewer, editor, color, and highlighting entries. These route into Near's typed settings and specialized surfaces.

The exact ordered command lists are versioned in `specs/menu-actions.toml`. Automated tests reject reordering, missing commands, duplicate accelerators, or static actions that produce neither an effect nor an explicit denial.

## Accelerators and Filtering

An ampersand in a menu item's source label declares its accelerator. Rendering turns the accelerated letter into a visible `[X]` marker. Pressing that unmodified letter activates an enabled item immediately. Text without a matching accelerator continues to filter the current menu, preserving fast fuzzy navigation for larger command groups.

Each submenu assigns unique accelerators to its entries. Arrow keys and Enter remain available through the contextual menu keymap.

## Availability

Menu entries are checked through the central `CommandRegistry` against the current `ActionContext`. Missing current resources, peer destinations, provider locations, or required capabilities disable the command before activation. The description begins with a concise `Unavailable:` reason so the explanation remains visible within the menu width.

The menu therefore cannot drift into a separate command system: availability, typed arguments, safety classification, diagnostics, and dispatch remain shared with every other invocation surface.
