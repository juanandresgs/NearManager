# Live Themes and Semantic Colors

Near loads the effective configured theme together with the shipped terminal-native and high-contrast presets. **Options → Colors and themes** previews any preset immediately without restarting the workspace. The working theme is separate from the committed session baseline: **Commit preview** advances the baseline, while **Roll back preview** restores the complete previous theme in one operation.

The semantic-role editor lists every declared role with its direct foreground and background values. Its title reports the detected terminal color depth. Editable values accept `default` for the terminal default, `ansi:N` for indexed terminals, `#RRGGBB` for true-color terminals, or blank to inherit through the role fallback graph. Invalid values and unknown roles leave the working theme unchanged.

Role edits update only direct colors; fallbacks, glyphs, density, and modifiers remain intact. Rendering always uses a depth-adjusted clone of the current working theme, so preset and role previews affect panels, dialogs, menus, status lines, and the key bar during the next frame. Session rollback replaces the entire working theme rather than attempting field-by-field reversal.
