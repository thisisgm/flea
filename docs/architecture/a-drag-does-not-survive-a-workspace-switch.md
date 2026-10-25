## A drag does not survive a workspace switch

Reported on 2026-09-22 (PR #186) as a drag from Flea into a terminal on another workspace pasting
nothing, and measured on the reporter's box: Flea 0.3.1, Hyprland 0.56.2, ghostty 1.3.1 as the drop
target, a uinput pointer. The payload is fine: `ui/js/Drag.js` `mimeFor` offers `text/uri-list` and
`text/plain`, and ghostty pastes the path. A drag onto the same workspace landed, and so did one onto
a terminal on the other monitor. A drag that switched workspace mid-gesture landed nothing.

That case is Hyprland's by construction. `CMonitor::changeWorkspace` (`src/output/Monitor.cpp:1454-1458`
at v0.56.2) calls `releaseAllMouseButtons()` on every non-internal workspace change, and a Wayland drag
ends on the button release, so the drop happens wherever the pointer is at that instant. After
`Drag.active = true` in `ui/FileDrag.qml` the compositor owns the gesture; Nautilus and Chromium lose
the same drag. Hyprland issue #15994 reports the call. The README tells users to bring the target
workspace up before lifting the file.

Two things a reproduction meets: ghostty's paste protection raises "Potentially Unsafe Paste" when the
terminal has no bracketed paste, which reads as a failed drop, and a press taken from an IPC centre can
land on the column header rather than the row, so read a screenshot before calling a drag failed.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
