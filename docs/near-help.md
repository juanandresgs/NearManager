# Runtime Hypertext Help

Near builds help from the running command registry, active contextual keymap, package version, and installed extension catalog. The help surface is reusable by suite applications and does not depend on file-manager coordinates or hard-coded key labels.

## Interaction

- `F1` opens the active surface context with effective, rebound keys.
- `Shift+F1` opens generated contents grouped by semantic command category.
- `Shift+F2` opens installed extension help, including contributed commands and command-line prefixes.
- `Tab` and `Shift+Tab` select links; `Enter` opens the selected topic.
- `F7` starts full-text search across titles, introductions, sources, links, command IDs, keys, and descriptions. `Ctrl+Enter` advances through results.
- `Alt+F1` or `Backspace` returns through topic history. During search, Backspace edits the query before leaving search.

The footer identifies the topic source. Core topics are generated from the running Near version; extension topics identify their extension owner. An unavailable link target cannot dispatch arbitrary behavior because links only resolve topic IDs inside the immutable help catalog.

## Extension Discovery

Every registered `CommandExtension` receives a generated topic. Its semantic descriptors and registered prefixes appear through the same help graph as built-in commands. This keeps help synchronized with capability-controlled extension loading and avoids requiring plugins to inject terminal markup.
