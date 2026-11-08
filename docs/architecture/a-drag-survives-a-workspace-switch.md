## A drag survives a workspace switch

Reported on 2026-09-22 (PR #186) as a drag from Flea into a terminal on another workspace pasting
nothing: Flea 0.3.1, Hyprland 0.56.2, ghostty 1.3.1 as the drop target, a uinput pointer. A drag onto
the same workspace landed, and so did one onto a terminal on the other monitor. A drag that switched
workspace mid-gesture landed nothing.

`CMonitor::changeWorkspace` (`src/output/Monitor.cpp:1457` at Hyprland v0.56.2) calls
`releaseAllMouseButtons()`, and that function (`src/managers/input/InputManager.cpp:2132-2143`)
returns at once while `PROTO::data->dndActive()`. It releases the buttons of a gesture the compositor
does not own as a drag, which is any drag an application still keeps to itself. GTK also starts its
Wayland drag at the drag threshold. The 0.3.1 drag was a Qt Wayland drag started at the threshold
too (`Drag.dragType: Drag.Automatic` in `ui/FileDrag.qml`), so this source does not explain that
report, and it was not measured again.

Flea 0.4.0's GPUI drag stayed inside the application until the pointer left the window and only then
became a Wayland drag. Measured on Hyprland 0.56.2 with Flea 0.4.0: a file carried out of the window
first and then taken to another workspace with the workspace key landed in a second Flea window there;
the same carry with the pointer still over Flea when the key was pressed ended the drag at the switch
and moved nothing. With the 0.4.1 drag below, both carries landed in the second window.

Since 0.4.1 the file drags of List, Columns and Grid start the Wayland drag at the drag threshold
(`external_drag_at_start` in gpui-omarchy, set in `crates/flea-gui/src/window/render.rs`,
`crates/flea-gui/src/window/columns_gestures.rs` and `crates/flea-gui/src/grid/pane_view.rs`).
While the pointer is over Flea the drag stays live in its window, so drops on rows, folders, the
sidebar, tabs and crumbs, spring-loaded folders, Escape and the keys held for the drop action behave
as before; the compositor draws the drag picture and the pointer. If the compositor refuses the drag
at the threshold, it stays inside Flea and becomes a Wayland drag when it leaves the window, as before
0.4.1. The drop action another application sees is the one chosen when the drag starts. Tab drags
still become Wayland drags only when they leave the window.

Two things a reproduction meets: ghostty's paste protection raises "Potentially Unsafe Paste" when the
terminal has no bracketed paste, which reads as a failed drop, and a press taken from an IPC centre can
land on the column header rather than the row, so read a screenshot before calling a drag failed.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
