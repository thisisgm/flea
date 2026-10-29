# File chooser pointer controls

Open and Save dialogs offer the same destination and grid controls:

- Click the displayed path or its padding to open the existing path input.
  The current directory starts selected. Enter navigates; Escape cancels editing.
  Ctrl+L, completion and folder-jump suggestions keep their existing behavior.
- The eye button beside the view controls shows or hides dotfiles. Its accessible
  name and hover label describe the next action. This changes only the current
  request, not the saved hidden-file preference. Relisting preserves the save
  filename and its selection; existing hidden-file shortcuts still work.
- Ctrl+wheel in the grid moves through Small, Medium, Large, Extra large, Huge
  and Largest, using the same thumbnail-size and Ctrl+wheel preferences as the
  main grid. Wheel notches and phased touchpad travel accumulate identically;
  plain wheel input still scrolls. The cursor and filename do not change.

Path editing and the eye button are unavailable while submitting or after backend
loss. View/visibility buttons retain the existing pointer-only chrome Tab policy;
keyboard actions continue through the listing, including unhandled Filename keys.
None of these controls writes the selected file or adds file-manager operations.

`tests/picker-interactions.sh` runs real Qt mouse/key/wheel events against the real
chooser and backend in Open/Save and initial List/Grid combinations. It checks
path activation, selection, Escape, focus transfer, typed navigation, actual hidden
rows, request-local visibility, submission guards, filename preservation, all six
zoom stops, bounds, ordinary scrolling, and refetching newly visible rows after
zoom-out without a scroll event. Synthetic handler events additionally
cover fractional notches, phased touchpad travel and notches with incidental pixel
deltas. The suite is registered in `tests/run-all.sh` and uses marker-owned,
disposable fixtures through `tools/flea-sandbox-guard`.
