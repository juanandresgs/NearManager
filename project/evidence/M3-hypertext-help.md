# M3 Hypertext Help Evidence

Date: 2026-06-23

## Implemented Slice

- `HelpSurface` now hosts a graph of typed `HelpTopic` and `HelpLink` values with selectable links, topic history, scrolling, and full-text search.
- The Far workspace generates context help from the active context and resolved keymap, contents from the live command registry, and extension topics from registered command descriptors and prefixes.
- The contents topic displays the running Near package version; no copied static command table can drift from the executable.
- Far-compatible bindings expose `F1`, `Shift+F1`, `Shift+F2`, `F7`, `Tab`, `Shift+Tab`, `Enter`, `Alt+F1`, and `Backspace` workflows.

## Automated Evidence

- Surface tests prove links open topics, back navigation restores history, search indexes command descriptions, and a result activates its source topic.
- The workspace integration test proves contents rendering, generated extension discovery, command and prefix visibility, full-text search, result activation, and backward navigation.
- Existing keymap reload tests continue proving context help displays rebound keys rather than compiled defaults.

## Requirement Status

- `REQ-HELP-001` and `WF-HELP-001` are verified.
- `FAR-MENU-003` is verified.
