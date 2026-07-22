# Near Mouse Interaction

Near remains keyboard-first, but terminals that support mouse reporting expose equivalent semantic workflows. `near-terminal` preserves the button, press/drag/release kind, coordinates, modifiers, and wheel direction instead of reducing mouse input to an opaque notification.

## Lifecycle

Mouse capture is an optional `TerminalSession` capability. Startup records whether capture was enabled, failure degrades without blocking the application, suspend disables capture before returning control to an external tool, and resume enables it again. Normal restoration, error cleanup, and panic unwinding disable capture independently from bracketed paste, cursor visibility, keyboard enhancement, the alternate screen, and raw mode.

## Panel Policy

- Left click focuses the panel and places its cursor on the clicked resource.
- Right click focuses the panel, places the cursor, and toggles that resource's selected state.
- Middle click invokes `near.resource.open`, preserving the same provider and command checks as Enter.
- Wheel events move the panel cursor by three rows in the panel under the pointer.
- Clicking a blank panel region changes focus without manufacturing a resource selection.

The bottom key bar is derived from the active keymap. Clicking a rendered function-key segment invokes that binding's existing `CommandInvocation`; it does not maintain a second mouse-only command table.

## Menus and Full-Screen Surfaces

Clicking a visible menu row selects and activates the same command used by Enter. The wheel changes the visible menu selection. Full-screen viewer, editor, and help surfaces receive their existing up/down semantic commands, so mouse scrolling inherits their normal availability and state handling.

## Cross-Panel Drag

A left-button drag starts from the current source resource. Moving over the peer panel displays a copy preview in the status line. Holding Shift changes the preview to move. Releasing over the peer invokes `near.resource.copy-to-peer` or `near.resource.move-to-peer`; the normal planner, conflict policy, confirmation policy, operation preview, journal, cancellation, and execution path remain authoritative.

Dropping in the source panel or outside a peer panel cancels the drag. No filesystem paths or operation implementation details are embedded in mouse handling.
