# M1 Mouse Interaction Evidence

- `near-terminal::TerminalEvent` retains mouse buttons, coordinates, modifiers, wheel directions, and drag phases.
- `TerminalSession` enables and restores mouse capture in the same failure-safe lifecycle as other optional terminal capabilities.
- Workspace tests verify panel focus, cursor placement, right-click selection, wheel movement, key-bar activation, visible menu-row activation, copy drag previews, and Shift-move drag previews.
- Drag completion invokes the ordinary operation commands and produces `OperationPreviewSurface`; it does not bypass planning or confirmation.
- Keyboard bindings and workflows remain unchanged, so no mouse is required.
