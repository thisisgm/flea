# Nautilus keyboard preset

Choose **Settings → Keys → Nautilus** for GNOME Files-style shortcuts in Flea's GUI.
Ctrl+H also toggles hidden files in the existing presets. Hidden-file visibility is independent
of the directory watcher fix: unchanged attribute notifications are suppressed in both modes.

The mappings follow [GNOME Files' shortcut dialog](https://github.com/GNOME/nautilus/blob/main/src/resources/ui/shortcuts-dialog.blp)
and [window accelerators](https://github.com/GNOME/nautilus/blob/main/src/nautilus-window.c).

| Task | Shortcut |
| --- | --- |
| Show hidden files | Ctrl+H |
| New tab / new window | Ctrl+T / Ctrl+N |
| Close tab (or its window when last) | Ctrl+W |
| Close the current Flea window | Ctrl+Q |
| Previous / next tab; select tab | Ctrl+PageUp / Ctrl+PageDown; Alt+1…9 |
| Open selected folder in tab / window | Ctrl+Enter / Shift+Enter |
| Open / properties / rename | Ctrl+O or Enter / Ctrl+I or Alt+Enter / F2 |
| Back / forward / parent / home | Alt+Left / Alt+Right / Alt+Up / Alt+Home |
| Enter location / root location / home location | Ctrl+L / / / ~ |
| List / grid / Flea columns view | Ctrl+1 / Ctrl+2 / Ctrl+3 |
| Refresh | F5 or Ctrl+R |
| Search current folder / home | Ctrl+F / Ctrl+Shift+F |
| Bookmark current folder | Ctrl+D |
| Open terminal here | Ctrl+. |
| Copy / cut / paste | Ctrl+C / Ctrl+X / Ctrl+V |
| Undo / redo | Ctrl+Z / Ctrl+Shift+Z |
| New folder | Ctrl+Shift+N |
| Trash / confirmed permanent deletion | Delete / Shift+Delete |
| Select all / invert / toggle current item | Ctrl+A / Ctrl+Shift+I / Ctrl+Space |
| Text zoom in / out / reset | Ctrl++ / Ctrl+- / Ctrl+0 |
| Preferences / shortcut sheet / context menu | Ctrl+, / Ctrl+? / Shift+F10 |

Ctrl+V pastes text into an active search or filter query (Shift+Insert also works).
Pasting appends to the query; Enter still runs the search.

Typing starts Flea's current-folder search; press Enter to run it. Plain Vim-style letter
bindings are disabled in this preset, so typing `d` cannot arm Trash. Editors and dialogs retain
their own key handling.

This maps shared features rather than adding every Nautilus feature. Flea does not yet provide
Nautilus's inline tree expansion, sidebar toggle, tab overview/restoration/reordering, pattern
selection dialog, or link-creation operations, so their shortcuts are not advertised here.
Flea's zoom changes text size, and Ctrl+Q closes the current window (each window has its own backend).
Other presets and the TUI keep their existing behavior apart from the shared Ctrl+H alias.

`keys.toml` remains the source of truth. `tools/flea-keymap-gen` emits the binding data in
`ui/js/KeyBindings.js` and its lookup and help-sheet logic in `ui/js/Keymap.js`.
