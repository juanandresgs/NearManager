# Showcase media policy

Near Manager's public visuals are product evidence. They must describe the current application
honestly, remain readable at README width, and make important keyboard workflows discoverable.

## Reassessment rule

Every substantial user-facing change must record one of these outcomes in its pull request:

1. **Keep the primary GIF.** Use this when the default interface and baseline interaction story are
   still representative. Include a short rationale.
2. **Refresh the primary GIF.** Use this when the default layout, navigation, key bindings, theme,
   terminology, or central workflow has materially changed.
3. **Add a differentiated GIF.** Use this when a major provider, surface, or workflow deserves its
   own focused demonstration and adding it to the primary overview would reduce clarity. Place the
   focused GIF with the relevant feature documentation; link it from the README only when it helps
   the product overview.

Routine internal refactors, tests, and invisible portability fixes may keep the current visual
without further media work. A release milestone or a large collection of user-facing features must
perform the reassessment even if each individual change appeared small.

## Capture and review requirements

- Record the current application binary in a native terminal renderer.
- Use disposable fixture data and an isolated terminal window; never capture an active desktop or
  working project.
- Tell one compact story in roughly 10 to 15 seconds with three to five readable transitions.
- Prefer meaningful interaction over command typing. Keep shortcut labels or contextual help visible
  when keyboard discoverability is part of the story.
- Preserve the application's real typography, cell geometry, box drawing, and semantic colors.
- Inspect a contact sheet and the full-size animation before replacing a published asset.
- Stage the candidate for human approval before changing README links, committing, or publishing.
- Use only project-generated or explicitly licensed media.

The primary asset is `docs/assets/near-manager-preview.gif`. It should remain an animated GIF at
least 1000 pixels wide, between 8 and 20 seconds long, and below 5 MiB. Focused differentiated GIFs
may use other dimensions when their documentation context requires it.

## Current primary story

The primary GIF recorded for Near 0.2.2 shows:

1. Dual-panel file and release navigation.
2. Selection of `README.md`.
3. `Ctrl+Q` Quick View.
4. The `F9` panel menu.
5. `F1` Context Help with discoverable bindings.

It was captured from the real application in native Ghostty against disposable data, then encoded
at 1200 by 924 pixels and 15 frames per second.
