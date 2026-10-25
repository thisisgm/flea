## Drag to Trash

A row drag dropped on the rail's Trash row sends the backend's existing `trash` request, the one
`dd` sends, so it journals the same `Trashed` step and `z` restores it. `ui/RailTrashRow.qml` is the
Trash row, a `SidebarRow` with a `DropArea`, and `ui/js/TrashDrop.js` decides.

**Only a drag this window lifted may trash.** A foreign drop, another Flea's or the shelf's carries
files this window did not lift, so the Trash drop refuses them rather than deleting files on the
strength of another sender's offer. A drag carrying paths trashes those
paths. A selection too wide to carry paths trashes by index, only through the backend that lifted
it and over the source listing, named in the numbering of the lift, which the row marker carries as its eighth field,
so `src/backend/rowguard.rs` refuses it after a re-list rather than trashing whatever those indices
name now. `tests/js/trashdrop.js` drives the decisions and `tests/drag.sh` case RT the real drop.

A platform drag delivers no hover, so an auto-hidden rail would never come up for one:
`ui/PaneRail.qml` lays a `DropArea` over the edge strip and the rail's width that keeps the overlay
rail up while a row drag rests there, and refuses the drop itself, so a drop elsewhere on the overlay
rail no longer falls through to the listing row under it. A child row DropArea that takes the drag
keeps the rail up through its own `containsDrag`, until the drag leaves the rail.

---

Navigation: [architecture index](README.md),
[load-bearing rules](../../AGENTS.md#the-seven-load-bearing-rules).
Source paths in this reference are relative to the repository root.
