.pragma library

.import "Format.js" as Format

// The backend sends a freedesktop icon name per row; this is the only place Flea maps one to a mark.
// Counts re-derived 2026-08-31 from /usr/share/mime/generic-icons on this box, see AGENTS.md "Icons in the row":
// x-office-document 106, package-x-generic 89, application-x-executable 57, text-x-generic 46,
// image-x-generic 40, x-office-spreadsheet 38, text-x-script 36, font-x-generic 23, x-office-presentation 21,
// video-x-generic 15, text-html 11, then a tail of five or fewer each.
var GLYPHS = {
    "folder": "folder",
    "inode-directory": "folder",
    "text-x-generic": "file-text",
    "text-plain": "file-text",
    // text-x-script, text-html and application-xml are 49 types a generic file mark would hide.
    "text-x-script": "code",
    "text-html": "code",
    "application-xml": "code",
    "image-x-generic": "image",
    "video-x-generic": "film",
    "audio-x-generic": "music",
    "package-x-generic": "archive",
    "font-x-generic": "type",
    "application-x-executable": "terminal",
    "application-x-generic": "file",
    "x-office-document": "file-text",
    "x-office-spreadsheet": "table",
    "x-office-presentation": "presentation"
}

var FALLBACK = "file"

function glyphFor(iconName) {
    var g = GLYPHS[String(iconName)]
    return g ? g : FALLBACK
}

// A row's mark. The backend resolves a symlink's icon to its target's, so a link to a directory
// arrives as "folder"; every board draws such a row with the link mark instead, so the mode decides
// here and the icon name only answers for what the row really is.
function glyphForRow(iconName, mode) {
    if (Format.isSymlink(mode))
        return "symlink"
    return glyphFor(iconName)
}

// The sidebar's own set, keyed on the favourite's label (Places.js leaf() or bookmark label),
// the operator's overnight ask; any bookmark outside this set draws the plain folder mark.
var SIDEBAR_GLYPHS = {
    "Home": "house",
    "Downloads": "download",
    "Documents": "file-text",
    "Pictures": "image",
    "Videos": "film",
    "Music": "music",
    "Projects": "folder-git-2"
}

var SIDEBAR_FALLBACK = "folder"

function sidebarGlyphFor(label) {
    var g = SIDEBAR_GLYPHS[String(label)]
    return g ? g : SIDEBAR_FALLBACK
}

// Lucide-static 1.38.0 geometry, ISC licence, https://github.com/lucide-icons/lucide, one "d"
// per mark on its native 24 unit grid, recut to the Omarchy edge: baked rounded corners became
// hard corners, real curves stayed; see AGENTS.md "Lucide path data", "The Omarchy cut".
var PATHS = {
    "info": "M12 11v6 M12 7h.01 M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0",
    "star": "M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3l-6.2 3.6 1.6-7L2 9.2l7.1-.6z",
    "globe": "M2 12h20 M12 2a15 15 0 0 1 0 20 M12 2a15 15 0 0 0 0 20 M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0",

    "file": "M4 22V2h10l6 6v14H4z M14 2v6h6",
    "folder": "M2 20V3h6l2 3h12v14H2z",
    "folders-first": "M2 12V4h6l2 2h12v6H2z M2 17h20 M2 21h20",
    "file-text": "M4 22V2h10l6 6v14H4z M14 2v6h6 M8 9h2 M8 13h8 M8 17h8",
    "code": "M16 18l6-6-6-6 M8 6l-6 6 6 6",
    "image": "M3 3h18v18H3z M7 9A2 2 0 1 0 11 9A2 2 0 1 0 7 9Z M21 15l-3-3L6 21",
    "film": "M3 3h18v18H3z M7 3v18 M3 7.5h4 M3 12h18 M3 16.5h4 M17 3v18 M17 7.5h4 M17 16.5h4",
    // Note heads squared instead of lucide's circles: the set's brand tell.
    "music": "M9 18V5l12-2v13 M3 15h6v6H3z M15 13h6v6h-6z",
    "archive": "M2 3h20v5H2z M4 8v13h16V8 M10 12h4",
    "type": "M12 4v16 M4 7V4h16v3 M9 20h6",
    "terminal": "M12 19h8 M4 17l6-6-6-6",
    "table": "M3 3h18v18H3z M12 3v18 M3 9h18 M3 15h18",
    "presentation": "M2 3h20 M3 3v13h18V3 M7 21l5-5 5 5",
    "house": "M3 21V10l9-7 9 7v11h-6v-8H9v8H3z",
    "download": "M12 15V3 M7 10l5 5 5-5 M3 15v6h18v-6",
    "folder-git-2": "M18 19a5 5 0 0 1-5-5v8 M9 20H2V3h6l2 2h12v5 M11 12A2 2 0 1 0 15 12A2 2 0 1 0 11 12Z M18 19A2 2 0 1 0 22 19A2 2 0 1 0 18 19Z",
    // The two near-zero-length lines are lucide's own technique for the rack unit's LED dots; SquareCap draws them as square dots, matching the cut.
    "server": "M2 2h20v8H2z M2 14h20v8H2z M6 6L6.01 6 M6 18L6.01 18",
    // The Network group's add mark, replacing a Text "+" the operator read as a Christian cross.
    "plus": "M5 12h14 M12 5v14",
    // The picker rail's Recent mark, lucide rotate-ccw, the one SendPicker.html itself draws; the
    // circle is a real curve and stays, and the arrow head is already square on lucide's own grid.
    "history": "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5",
    // Lucide's hard-drive with its four baked 2 unit corner arcs cut square, which lands the body on
    // (6,4) (18,4) (22,12) (22,20) (2,20) (2,12); the divider and the two LED dots are lucide's own.
    "drive": "M6 4h12l4 8v8H2v-8z M2 12h20 M6 16L6.01 16 M10 16L10.01 16",
    // The rail menu's release mark, the shelf's "for: unmount": lucide's triangle corner arcs
    // extend to (12, 2.09) (22.32, 13) (1.68, 13) and snap to grid, its rx=1 bar rect squares off.
    "eject": "M12 2 22 13H2z M3 17h18v4H3z",
    // Task 22's transport strip, recut sharp like the rest of the set.
    "play": "M6 4l14 8-14 8z",
    "pause": "M14 3h5v18h-5z M5 3h5v18H5z",
    // The search strip's mark; the lens is a real curve, so it stays a circle under the cut.
    "search": "M16.7 16.7 21 21 M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z",
    // The filter strip's mark, the funnel the icon spec already shelves and the Main board draws.
    "filter": "M3 4h18l-7 8v7l-4 2v-9z",
    // The context menu's own seven marks, added with the operations surface that draws them.
    // The canvas's own open folder, which keeps the closed folder's back panel byte for byte.
    "folder-open": "M2 20V3h6l2 3h12v3 M22 11l-2.5 9H2l2.5-9z",
    // OpenWith.html draws this window for the Open with row, its tail row and an app the theme cannot name.
    "app-window": "M3 4h18v16H3z M3 9h18 M6 6.5h.01 M9 6.5h.01",
    "undo": "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5",
    "rename": "M3 21l1-4L17 4l3 3L7 20l-4 1z M14 7l3 3",
    "file-plus": "M6 22V2h8l4 4v16H6z M14 2v4h4 M12 11v6 M9 14h6",
    // lucide's own folder-plus is the folder body byte for byte plus these two strokes, so the recut
    // body is reused verbatim and only the plus is new; the specimen sheet's 5 unit plus is not it.
    "folder-plus": "M2 20V3h6l2 3h12v14H2z M12 10v6 M9 13h6",
    "trash": "M3 6h18 M8 6V3h8v3 M6 6l1.2 15h9.6L18 6 M10 10v7 M14 10v7",
    // A diamond eye with a square pupil: lucide's own eye is two arcs meeting at points, which the cut squares off.
    "eye": "M12 5 22 12 12 19 2 12z M10 10h4v4h-4z",
    "eye-off": "M12 5 22 12 12 19 2 12z M10 10h4v4h-4z M4 4l16 16",
    // Replaces the "▸" Text the menu used to draw, so it stops mixing a font dingbat into a path language.
    "chevron-right": "M9 18l6-6-6-6",
    "chevron-left": "M15 18l-6-6 6-6",
    // The window chrome's own five marks, added with the bar that draws them.
    "arrow-left": "M19 12H5 M12 19l-7-7 7-7",
    "arrow-up": "M12 19V5 M5 12l7-7 7 7",
    // The bullets are lucide's own zero-length-line technique, which SquareCap draws as square dots.
    "list": "M8 6h13 M8 12h13 M8 18h13 M3 6L3.01 6 M3 12L3.01 12 M3 18L3.01 18",
    "columns": "M3 3h18v18H3z M9 3v18 M15 3v18",
    "preview": "M3 3h18v18H3z M15 3v18",
    "dual": "M3 4h8v16H3z M13 4h8v16h-8z",
    "grid": "M3 3h8v8H3z M13 3h8v8h-8z M3 13h8v8H3z M13 13h8v8h-8z",
    // The preview column's own two, taken from the canvas's icon table rather than recut from lucide.
    "symlink": "M7 17L17 7 M8 7h9v9",
    // The dot is the zero-length-line technique again, which SquareCap draws square.
    "alert": "M12 2 23 21H1z M12 10v4 M12 18L12.01 18",
    // The Locked pane state draws this beside it: the shackle is a real curve so it stays an arc, and the keyhole is the same dot technique.
    "lock": "M4 11h16v10H4z M8 11V7a4 4 0 0 1 8 0v4 M12 15h.01",
    // The operations rows the archive and convert subsystems draw, from the spec's own glyph shelf.
    "archive-out": "M2 3h20v5H2z M4 8v13h16V8 M12 18v-6 M9 15l3-3 3 3",
    // The Move to Dropbox row, which the canvas draws with the network mark.
    "network": "M9 2h6v6H9z M2 16h6v6H2z M16 16h6v6h-6z M12 8v4 M5 16v-4h14v4",
    // The convert popup's checkbox mark.
    "check": "M4 12l6 6L20 6",
    // The PDF viewer's own three, recut sharp like the rest of the set; "maximize" is lucide's
    // name for the mark the canvas calls expand.
    "minus": "M5 12h14",
    "maximize": "M8 3H3v5 M16 3h5v5 M8 21H3v-5 M16 21h5v-5",
    "x": "M6 6l12 12 M18 6 6 18",
    "sliders": "M4 21v-7 M4 10V3 M12 21v-9 M12 8V3 M20 21v-5 M20 12V3 M2 14h4 M10 8h4 M18 16h4",
    // The background menu's Sort by row, Menus.html's own geometry: three rules shortening to the
    // right of a bare down arrow. IconSets.html names this mark when it caps the corner radius.
    "sort": "M11 5h10 M11 9h7 M11 13h4 M7 5v14 M7 19l-3-3 M7 19l3-3",
    // The three basic clipboard rows the context menu grew with its visibility settings. Lucide's
    // scissors and clipboard both carry arcs; these are the cut's own square handles and square tray.
    "scissors": "M5 3l14 14 M19 3L5 17 M2 17h5v5H2z M17 17h5v5h-5z",
    "copy": "M9 8h12v13H9z M4 16V3h13",
    "clipboard": "M9 2h6v4H9z M6 4H3v18h18V4h-3 M8 12h8 M8 16h5",
    // The Keys section's rail mark: the key caps are the same zero-length-line dots the list mark uses.
    "keyboard": "M2 6h20v12H2z M6 10L6.01 10 M10 10L10.01 10 M14 10L14.01 10 M18 10L18.01 10 M8 14h8"
}

function pathFor(name) {
    return PATHS[name] || PATHS[FALLBACK]
}
