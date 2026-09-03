.import "../../ui/js/Columns.js" as Columns

// Below about 659 px of window the four fixed columns claimed the whole row and the filename had a
// negative slot, so ui/Row.qml drew every column except the one a file manager exists for. These
// are the floors that stop it: the name never loses, the metadata drops instead.

// This box's own resolved tokens, read off the running app's tokens() seam at base-size 14:
//   rowPaddingX=14 gap=9 iconSize=23 columnMode=70 columnSize=70 columnDate=125 columnKind=130
// nameMin is 20 characters of the same 7.8125 px advance the fixed columns are sized from.
var BOX = { rowPaddingX: 14, gap: 9, iconSize: 23, nameMin: 156, mode: 70, size: 70, date: 125, kind: 130 }

// A second set that shares no number with the first, so nothing here can pass on a constant.
var OTHER = { rowPaddingX: 6, gap: 4, iconSize: 16, nameMin: 100, mode: 40, size: 50, date: 80, kind: 60 }

// The anchor chain in ui/Row.qml, walked here independently of ui/js/Columns.js: the row, less its
// padding either side, the mark and the gap after it, and every drawn column with its own gap.
function nameSlot(width, s, t) {
    var used = t.rowPaddingX + t.iconSize + t.gap + t.rowPaddingX
    if (s.mode) used += t.mode + t.gap
    if (s.size) used += t.size + t.gap
    if (s.date) used += t.date + t.gap
    if (s.kind) used += t.kind + t.gap
    return width - used
}

function run(check) {
    runHidden(check)
    var f = Columns.floors(BOX)
    // 216 is the name at its floor with no metadata at all: 14 + 23 + 9 + 156 + 14.
    check("mode needs the name's floor plus its own column and gap", f.mode, 295)
    check("size needs mode's floor plus its own", f.size, 374)
    check("date needs size's floor plus its own", f.date, 508)
    check("kind needs date's floor plus its own", f.kind, 647)
    // A wider column can never outlive a narrower one, which is what makes the drop order an order.
    check("the four floors nest, widest last",
          f.mode < f.size && f.size < f.date && f.date < f.kind, true)

    // 732 is the list area of the 900 px window Flea asks for, beside this box's 168 px rail.
    check("the default window draws every column",
          Columns.names(Columns.set(732, BOX)), "name,mode,size,date,kind")
    check("a column is kept at exactly its floor",
          Columns.set(647, BOX).kind, true)
    check("and dropped one pixel under it",
          Columns.set(646, BOX).kind, false)
    check("kind goes first and the other three stay",
          Columns.names(Columns.set(646, BOX)), "name,mode,size,date")
    check("date goes second",
          Columns.names(Columns.set(507, BOX)), "name,mode,size")
    check("size goes third",
          Columns.names(Columns.set(373, BOX)), "name,mode")
    check("mode goes last, and the last layout is the mark and the name",
          Columns.names(Columns.set(294, BOX)), "name")
    // 453 is the list area at the 621 px window Hyprland handed Flea beside three terminals.
    check("the width that drew no name at all now draws the name, mode and size",
          Columns.names(Columns.set(453, BOX)), "name,mode,size")

    // The whole point: at no width does a column survive that would put the name under its floor.
    var everyWidthKeepsTheName = true
    var neverGrowsAsItNarrows = true
    var previous = null
    for (var w = 2000; w >= 216; w--) {
        var s = Columns.set(w, BOX)
        if (nameSlot(w, s, BOX) < BOX.nameMin)
            everyWidthKeepsTheName = false
        if (previous !== null) {
            if ((s.mode && !previous.mode) || (s.size && !previous.size)
                || (s.date && !previous.date) || (s.kind && !previous.kind))
                neverGrowsAsItNarrows = false
        }
        previous = s
    }
    check("every width from the name's own floor up keeps the name at or above it",
          everyWidthKeepsTheName, true)
    check("no column ever comes back as the row narrows",
          neverGrowsAsItNarrows, true)

    // Under the name's own floor there is nothing left to drop, so the name takes what is left
    // rather than the layout inventing a column to lose. ui/MatchText.qml clamps the rest.
    check("under the last rung the set is empty rather than undefined",
          Columns.names(Columns.set(100, BOX)), "name")
    check("a zero width answers rather than throwing", Columns.names(Columns.set(0, BOX)), "name")
    check("a negative width answers the same", Columns.names(Columns.set(-500, BOX)), "name")

    // Nothing above is a constant: the same arithmetic on a token set sharing none of those numbers.
    var g = Columns.floors(OTHER)
    check("another token set moves every floor with it",
          g.mode + "|" + g.size + "|" + g.date + "|" + g.kind, "176|230|314|378")
    check("and keeps them nested",
          g.mode < g.size && g.size < g.date && g.date < g.kind, true)
    check("and keeps the name above its own floor there too",
          nameSlot(g.kind, Columns.set(g.kind, OTHER), OTHER) >= OTHER.nameMin, true)

    // The seam ui/Ipc.qml reads is this string, and the header and a row must produce the same one.
    check("the set names the columns left to right, not in drop order",
          Columns.names({ mode: true, size: true, date: true, kind: true }),
          "name,mode,size,date,kind")
    check("the name is in the set even when everything else is gone",
          Columns.names({ mode: false, size: false, date: false, kind: false }), "name")
}

// The user's own hidden set, subtracted from what the width affords: a hidden column never draws,
// and width still wins, so a column shown while the pane is too narrow stays dropped. The keys are
// the same "mode"/"size"/"date"/"kind" the header menu's col:<key> actions carry.

function runHidden(check) {
    var none = Columns.set(2000, BOX, [])
    check("an empty hidden set draws every column the width affords",
          [none.mode, none.size, none.date, none.kind].join(","), "true,true,true,true")

    var hid = Columns.set(2000, BOX, ["size", "kind"])
    check("a hidden column does not draw at a width that would afford it",
          [hid.mode, hid.size, hid.date, hid.kind].join(","), "true,false,true,false")

    var narrow = Columns.set(200, BOX, ["kind"])
    check("width still wins over a column the user wants back, Mode's own floor included",
          [narrow.mode, narrow.size, narrow.date, narrow.kind].join(","), "false,false,false,false")

    var undefinedSet = Columns.set(2000, BOX)
    check("a caller that passes no hidden set draws as before",
          [undefinedSet.mode, undefinedSet.size, undefinedSet.date, undefinedSet.kind].join(","), "true,true,true,true")
}
