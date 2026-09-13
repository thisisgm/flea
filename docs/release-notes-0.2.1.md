- Add an Empty Trash control to the Trash window, beside the item count. It opens the same
  confirmation the menu row does, so permanent deletion still confirms.
- Add an optional sweep that empties the Trash of anything deleted 30 days ago or longer. It is
  off until you switch it on in Settings, Places, Trash, because permanent deletion is outside
  the undo journal.
- Add Settings, View, Opening: which folder a window opens in (Home, the last folder, or one you
  choose) and where a new tab opens (the current folder, Home, or wherever a window would).
- Close the preview with a click anywhere outside it, not only with escape.
- Go to a search result with one click instead of two, and into a folder in the columns view with
  one click, which is what its own neighbouring columns already did.
- Show every internal drive in Devices, not only removable ones. A second internal drive never
  appeared at all, and on a machine with two the wrong one could be labelled as the system disk.
- Keep the cursor and the selection where they were after deleting a file, so a directory of
  images can be walked and pruned without starting over; thanks to the reporter.
- Extract .rar archives. Creating one is not offered, because nothing in the Arch repositories
  can write that format.
- Draw every control's frame in its own role rather than in the colour of the rules around it, so
  a control no longer dissolves into the chrome it sits in at smaller sizes. The file picker
  changes the most.
- Fix three tests that reached the sandbox without the guard the others use, which is what made
  the 0.2.0 check() fail on the Omarchy build container.
- Close the preview with a second press of space, on every kind including media, the way Finder
  does. Playback moves to p, so a media preview still plays and pauses from the keyboard.
- Add a network place with a from the list as well as the rail, which is what the key table has
  always said it did.
