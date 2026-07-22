# Near accessibility contract

Near treats color as enhancement, never as the only carrier of interaction state. The semantic scene remains usable with `NO_COLOR`, monochrome terminals, limited palettes, and user-supplied themes.

## Verification checklist

- **Focus:** focused panels have a distinct border role; focused rows use reverse video in monochrome.
- **Selection:** selected resources retain a visible selection glyph and bold treatment; focused selection combines reverse and underline treatment.
- **Safety:** operation previews spell out `Arm irreversible operation` and then `CONFIRM irreversible operation`; conflict decisions are named in text.
- **Validation:** dialog errors are rendered as error text and warning semantics, not color alone.
- **Tasks:** each row prints the task state, title, numeric progress, and message. Failed tasks add warning semantics without replacing the textual state.
- **Keyboard operation:** prompts and key-bar labels name available actions.
- **Reduced motion:** Near has no animated transitions, spinners, blinking roles, or timer-driven decorative updates. Progress is a static textual value refreshed only by task events; the idle loop sleeps on the shared poll interval.

## Theme checks

`TerminalColorDepth::Monochrome` removes foreground and background colors while adding semantic modifiers for focus, selection, warnings, matches, and disabled controls. Unit tests verify these states remain distinct and that the ASCII selection glyph is present.

The shipped high-contrast theme uses explicit RGB foreground/background pairs. Its executable contrast test applies the WCAG relative-luminance formula and requires at least 4.5:1 for every core text-bearing role. Terminal-native themes cannot promise an RGB ratio because the user's terminal owns those colors, so they rely on modifiers, glyphs, labels, and layout instead.
