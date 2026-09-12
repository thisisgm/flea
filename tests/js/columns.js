.import "../../ui/js/Columns.js" as Columns
.import "../../ui/js/Picker.js" as Picker

// Below about 659 px of window the four fixed columns claimed the whole row and the filename had a
// negative slot, so ui/Row.qml drew every column except the one a file manager exists for. These
// are the floors that stop it: the name never loses, the metadata drops instead.

// This box's own resolved tokens, read off the running app's tokens() seam at base-size 14:
//   rowPaddingX=14 gap=9 iconSize=23 columnMode=70 columnSize=70 columnDate=125 columnKind=130 columnAge=55
// nameMin is 20 characters of the same 7.8125 px advance the fixed columns are sized from.
var BOX = { rowPaddingX: 14, gap: 9, iconSize: 23, nameMin: 156, mode: 70, size: 70, date: 125, kind: 130, age: 55 }

// A second set that shares no number with the first, so nothing here can pass on a constant.
var OTHER = { rowPaddingX: 6, gap: 4, iconSize: 16, nameMin: 100, mode: 40, size: 50, date: 80, kind: 60, age: 60 }

// The chooser's own tokens: BOX with Theme.column.pickerDate in place of the window's date, which
// the seam resolves to 80 at base-size 14, SendPicker.html's own slot.
var PICKER = { rowPaddingX: 14, gap: 9, iconSize: 23, nameMin: 156, mode: 70, size: 70, date: 80, kind: 130, age: 55 }

// The chooser's list area on this box: Hyprland floats the picker at 875 px and ui/PickerPlaces.qml
// takes Theme.space(150), 175 px of it, measured off the window Hyprland reported for flea --pick.
var PICKER_SLOT = 700

// The anchor chain ui/RowCells.qml walks, here independently of ui/js/Columns.js: the row, less
// its padding either side, the mark and the gap after it, and every drawn column with its own gap.
function nameSlot(width, s, t) {
    var used = t.rowPaddingX + t.iconSize + t.gap + t.rowPaddingX
    if (s.mode) used += t.mode + t.gap
    if (s.size) used += t.size + t.gap
    if (s.date) used += t.date + t.gap
    if (s.kind) used += t.kind + t.gap
    if (s.age) used += t.age + t.gap
    return width - used
}

function run(check) {
    var dual = {rowPaddingX: 14, gap: 9, iconSize: 13 * 1.45, nameMin: 180, size: 70, date: 125, age: 55}
    check("dual date fits the board's 431px floor", Columns.names(Columns.dualSet(431, dual, [])), "name,size,date")
    check("dual date drops below its name floor", Columns.names(Columns.dualSet(430, dual, [])), "name,size")
    check("dual age turns on at its own floor, date's beside it",
          Columns.names(Columns.dualSet(486, dual, [])), "name,size,date,age")
    check("and one pixel under it does not", Columns.names(Columns.dualSet(485, dual, [])), "name,size,date")
    check("dual age never outlives date", Columns.names(Columns.dualSet(430, dual, [])), "name,size")
    check("hidden dual size releases its actual width", Columns.names(Columns.dualSet(361, dual, ["size"])), "name,date")
    check("hidden dual age stays off even at a width that affords it",
          Columns.names(Columns.dualSet(600, dual, ["age"])), "name,size,date")
    check("dual minimum still keeps Name", Columns.names(Columns.dualSet(200, dual, [])), "name")
    runHidden(check)
    runPicker(check)
    var f = Columns.floors(BOX)
    // 216 is the name at its floor with no metadata at all: 14 + 23 + 9 + 156 + 14.
    // Age drops first -- the first casualty as the row narrows -- so its floor is the widest
    // and carries every column that outlives it, while every older floor keeps exactly the
    // value it was pinned at before the fifth column joined.
    check("age's floor carries every column that outlives it", f.age, 711)
    check("mode's floor is unchanged by the column that drops before it", f.mode, 295)
    check("size's floor is unchanged", f.size, 374)
    check("date's floor is unchanged", f.date, 508)
    check("kind's floor is unchanged", f.kind, 647)
    // A wider column can never outlive a narrower one, which is what makes the drop order an order.
    check("the five floors nest, age's the widest",
          f.mode < f.size && f.size < f.date && f.date < f.kind && f.kind < f.age, true)

    // 732 is the list area of the 900 px window Flea asks for, beside this box's 168 px rail.
    check("the default window draws every column",
          Columns.names(Columns.set(732, BOX)), "name,mode,size,date,kind,age")
    check("age is kept at exactly its floor",
          Columns.set(711, BOX).age, true)
    check("and dropped one pixel under it",
          Columns.set(710, BOX).age, false)
    check("age goes first and the other four stay",
          Columns.names(Columns.set(710, BOX)), "name,mode,size,date,kind")
    check("a column is kept at exactly its floor",
          Columns.set(647, BOX).kind, true)
    check("and dropped one pixel under it",
          Columns.set(646, BOX).kind, false)
    check("kind goes second",
          Columns.names(Columns.set(646, BOX)), "name,mode,size,date")
    check("date goes third",
          Columns.names(Columns.set(507, BOX)), "name,mode,size")
    check("size goes fourth",
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
                || (s.date && !previous.date) || (s.kind && !previous.kind)
                || (s.age && !previous.age))
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
          g.age + "|" + g.mode + "|" + g.size + "|" + g.date + "|" + g.kind, "442|176|230|314|378")
    check("and keeps them nested",
          g.mode < g.size && g.size < g.date && g.date < g.kind && g.kind < g.age, true)
    check("and keeps the name above its own floor there too",
          nameSlot(g.kind, Columns.set(g.kind, OTHER), OTHER) >= OTHER.nameMin, true)

    // drawnWidth walks the same anchor chain ui/RowCells.qml draws, so its gap count must
    // survive hidden cells: a hidden cell keeps its anchor slot and the margin beside it
    // still draws, so the count is of drawn cells other than mode, not of predecessor flags.
    // The default set (kind and mode hidden) is the case the row ships in.
    check("drawnWidth counts the gap a hidden kind leaves between date and age",
          Columns.drawnWidth({ mode: false, size: false, date: true, kind: false, age: true }, BOX, false), 212)
    check("and the default set, kind and mode hidden, counts every drawn boundary",
          Columns.drawnWidth({ mode: false, size: true, date: true, kind: false, age: true }, BOX, false), 291)
    check("a lone age still draws the margin beside its hidden anchor",
          Columns.drawnWidth({ mode: false, size: false, date: false, kind: false, age: true }, BOX, false), 78)
    check("every column shown is the sum of the widths and four gaps",
          Columns.drawnWidth({ mode: true, size: true, date: true, kind: true, age: true }, BOX, false), 500)
    check("dual draws no gaps at all",
          Columns.drawnWidth({ mode: false, size: true, date: true, kind: false, age: true }, BOX, true), 264)

    // The seam ui/Ipc.qml reads is this string, and the header and a row must produce the same one.
    check("the set names the columns left to right, not in drop order",
          Columns.names({ mode: true, size: true, date: true, kind: true, age: true }),
          "name,mode,size,date,kind,age")
    check("the name is in the set even when everything else is gone",
          Columns.names({ mode: false, size: false, date: false, kind: false, age: false }), "name")
}

// SendPicker.html draws a chooser row as the name, a 70 px size and an 80 px date, and nothing
// else, so ui/PickerList.qml hands ui/Row.qml Picker.HIDDEN_COLS instead of the window's own set.
// Without it the chooser inherited whatever the header menu had switched on for the browser window.

function runPicker(check) {
    // The negative control: the chooser's slot affords all six, which is what it drew with the
    // window's set and Mode, Kind and Age switched on.
    check("the chooser's own slot is wide enough for every column",
          Columns.names(Columns.set(PICKER_SLOT, PICKER)), "name,mode,size,date,kind,age")
    check("the chooser draws the board's three and nothing else",
          Columns.names(Columns.set(PICKER_SLOT, PICKER, Picker.HIDDEN_COLS)), "name,size,date")

    // Not only at that width: no width brings a column the chooser's board does not have.
    var everDrawn = false
    for (var w = 3000; w >= 0; w--) {
        var s = Columns.set(w, PICKER, Picker.HIDDEN_COLS)
        if (s.mode || s.kind || s.age)
            everDrawn = true
    }
    check("no width at all draws Mode, Kind or Age in the chooser", everDrawn, false)
}

// The user's own hidden set, subtracted from what the width affords: a hidden column never draws,
// and width still wins, so a column shown while the pane is too narrow stays dropped. The keys are
// the same "mode"/"size"/"date"/"kind"/"age" the header menu's col:<key> actions carry.

function runHidden(check) {
    var none = Columns.set(2000, BOX, [])
    check("an empty hidden set draws every column the width affords",
          [none.mode, none.size, none.date, none.kind, none.age].join(","), "true,true,true,true,true")

    var hid = Columns.set(2000, BOX, ["size", "kind"])
    check("a hidden column does not draw at a width that would afford it",
          [hid.mode, hid.size, hid.date, hid.kind, hid.age].join(","), "true,false,true,false,true")

    var narrow = Columns.set(200, BOX, ["kind"])
    check("width still wins over a column the user wants back, Mode's own floor included",
          [narrow.mode, narrow.size, narrow.date, narrow.kind, narrow.age].join(","), "false,false,false,false,false")

    var undefinedSet = Columns.set(2000, BOX)
    check("a caller that passes no hidden set draws as before",
          [undefinedSet.mode, undefinedSet.size, undefinedSet.date, undefinedSet.kind, undefinedSet.age].join(","), "true,true,true,true,true")
}
