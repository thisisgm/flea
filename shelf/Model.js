.pragma library

// The pile as the bar reads it. Every function here is pure: the Service owns the file and this
// owns what its bytes mean, so a shape nobody has drawn yet can be tested without a shell at all.

// Whole sentences with "%1" for a value, filled by fill so each translates as one; one per count, because the noun changes with it.
var WORDS = {
  holdingOne: "Flea shelf is holding 1 item",
  holdingMany: "Flea shelf is holding %1 items",
  clearedOne: "Cleared %1 item \u00b7 z restores",
  clearedMany: "Cleared %1 items \u00b7 z restores",
  putBackOne: "Put %1 item back",
  putBackMany: "Put %1 items back"
}

// The shape the widget draws when nothing has been read, so no caller ever meets an undefined pile.
function empty() {
  return { items: [], count: 0, ok: false }
}

// Sample input: {"items":[{"path":"/home/gm/Work/a.txt","name":"a.txt","bytes":1024}]}
// A file that is missing, half written or not JSON at all answers the empty shape rather than
// throwing: the pile is written by another process and the bar has to survive reading it mid-write.
function parse(text) {
  var raw = String(text || "")
  if (raw.length === 0) {
    return empty()
  }
  var parsed
  try {
    parsed = JSON.parse(raw)
  } catch (e) {
    return empty()
  }
  if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.items)) {
    return empty()
  }
  var items = []
  for (var i = 0; i < parsed.items.length; i++) {
    var item = parsed.items[i]
    if (!item || typeof item.path !== "string" || item.path.length === 0) {
      continue
    }
    var folder = item.folder === true
    // A folder wears its own separator, the way every Flea listing draws one: rule 3's "reads as Flea".
    items.push({ path: item.path, name: leaf(item.path) + (folder ? "/" : ""), bytes: bytesOf(item),
          folder: folder, partial: item.partial === true, pinned: item.pinned === true })
  }
  return { items: items, count: items.length, ok: true }
}

// The row's own name, which is the path's last segment; a trailing slash names the folder before it.
function leaf(path) {
  var text = String(path)
  if (text.length > 1 && text.charAt(text.length - 1) === "/") {
    text = text.substring(0, text.length - 1)
  }
  var cut = text.lastIndexOf("/")
  return cut < 0 ? text : text.substring(cut + 1)
}

// A size the writer did not answer is not a zero, so it is carried as -1 and drawn as nothing.
// Number(null) and Number("") are both 0, which is why the type is checked before the value.
function bytesOf(item) {
  var n = typeof item.bytes === "number" ? item.bytes : -1
  return isFinite(n) && n >= 0 ? n : -1
}

// The shelf's own thousands separator. It mirrors ui/js/Format.js count, which this tree
// cannot import: the shelf moves with shelf/ into its own repository, so nothing here imports up.
function grouped(n) {
  var digits = String(n)
  var out = ""
  for (var i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 === 0) {
      out += ","
    }
    out += digits.charAt(i)
  }
  return out
}

// Fills %1 to %9 in one pass (a value prints as given, an unfilled one stays); mirrors ui/js/Format.js fill, which this tree cannot import.
function fill(template) {
  var values = arguments
  return String(template).replace(/%([1-9])/g, function (token, at) {
    return at < values.length ? String(values[at]) : token
  })
}

// BarMark rule 3: the count is never in the bar itself, so this is what the hover tooltip says. It
// counts the pile alone, as rule 12 does, because a pinned row is not an item anybody sent.
function tooltip(state) {
  var n = state ? loose(state.items).length : 0
  if (n === 0) {
    return "Flea shelf is empty"
  }
  return n === 1 ? WORDS.holdingOne : fill(WORDS.holdingMany, grouped(n))
}

// ---- what the card draws, Main board rules 4, 5 and 7 ----

// The two marks a row can take, on the Omarchy cut, the same paths ui/js/Icons.js draws them from.
// Flea's own mark, which is what an empty shelf draws rather than a stand-in for one.
var SHELF_GLYPH = "M21 21H3V3h18v14H7V7h10v6h-6"
// The strip's ink, at the names Flea's own menu rows use, copied here because a plugin cannot import
// the app's Icons.js. Send has no path: it carries Tailscale's own mark, which ShelfActionButton draws.
var ACTION_GLYPHS = {
  move: "M2 20V3h6l2 3h12v14H2z M12 10v6 M9 13h6",
  copy: "M9 8h12v13H9z M4 16V3h13",
  zip: "M2 3h20v5H2z M4 8v13h16V8 M10 12h4",
  send: "",
  paths: "M4 22V2h10l6 6v14H4z M14 2v6h6 M8 9h2 M8 13h8 M8 17h8",
  pin: "M9 3h6v6l3 3v2H6v-2l3-3z M12 14v7",
  clipboard: "M9 2h6v4H9z M6 4H3v18h18V4h-3 M8 12h8 M8 16h5",
  remove: "M6 6l12 12 M18 6 6 18"
}
var FOLDER_GLYPH = "M2 20V3h6l2 3h12v14H2z"
// A recording is drawn by its mark and never decoded: the Omarchy cut's play square.
var RECORDING_GLYPH = "M4 3h16v18H4z M10 9l6 3l-6 3z"
var FILE_GLYPH = "M4 22V2h10l6 6v14H4z M14 2v6h6"

// Rule 4: a folder shows bytes, never an item count, because that is what the backend answers. A
// size nobody has answered yet is one dot, and a walk that was cut short keeps its own > prefix.
function sizeText(item) {
  if (!item || item.bytes === undefined || item.bytes === null || item.bytes < 0) {
    return "\u00b7"
  }
  return (item.partial === true ? ">" : "") + size(item.bytes)
}

function glyphFor(item) {
  if (item && item.folder === true) {
    return FOLDER_GLYPH
  }
  return item && item.recording === true ? RECORDING_GLYPH : FILE_GLYPH
}

// A size in the same words Flea's own list uses, so the shelf reads as Flea and not as a second app.
function size(bytes) {
  var n = Number(bytes)
  if (!isFinite(n) || n < 0) {
    return ""
  }
  var units = ["B", "kB", "MB", "GB", "TB"]
  var at = 0
  while (n >= 1000 && at < units.length - 1) {
    n = n / 1000
    at += 1
  }
  return (at === 0 ? String(Math.round(n)) : n.toFixed(1)) + " " + units[at]
}

// Rule 4's budget: the next path the card is drawing that nobody has answered for, one at a time.
// A path already answered is never asked again, so nothing here polls, and a path the card is not
// drawing is never in this list at all.
function nextSize(list, sizes) {
  for (var i = 0; i < list.length; i++) {
    var item = list[i]
    if (item.bytes < 0 && sizes[item.path] === undefined) {
      return item.path
    }
  }
  return ""
}

// A size the card is no longer drawing is not kept: the map holds answers for the pile in front of
// the pointer and nothing else, so a path that leaves and comes back is measured again.
function keep(sizes, list) {
  var kept = {}
  for (var i = 0; i < list.length; i++) {
    var path = list[i].path
    if (sizes[path] !== undefined) {
      kept[path] = sizes[path]
    }
  }
  return kept
}


// ---- the recent captures tray, ShelfEmpty rules 2, 4, 5 and 7 ----

// Sample input, one line per capture, newest first, the mtime in milliseconds then the path:
// 1757890932000 /home/gm/Pictures/screenshot-2026-09-14_19-02-11.png
function parseCaptures(text) {
  var lines = String(text || "").split("\n")
  var out = []
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i]
    var cut = line.indexOf(" ")
    if (cut < 1) {
      continue
    }
    var at = Number(line.substring(0, cut))
    var path = line.substring(cut + 1)
    if (!isFinite(at) || path.length === 0) {
      continue
    }
    out.push({ path: path, at: at, recording: isRecording(path) })
  }
  return out
}

function isRecording(path) {
  return String(path).slice(-4).toLowerCase() === ".mp4"
}


function pad2(n) {
  return n < 10 ? "0" + n : String(n)
}


// A path becomes a file:// URI segment by segment: encodeURI leaves # and ? alone, and a name that
// carries one then arrives at the drop target cut off at that character.
function uriPath(path) {
  var parts = String(path).split("/")
  var out = []
  for (var i = 0; i < parts.length; i++) {
    out.push(encodeURIComponent(parts[i]))
  }
  return out.join("/")
}

// ---- EdgeRail: what a drag dropped on the wall is offering ----

// Sample input, one URI per line as every toolkit writes a text/uri-list, comments and all:
// file:///home/gm/Pictures/one.png\r\n
function pathsFromUris(text) {
  var lines = String(text || "").split("\n")
  var out = []
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].replace("\r", "").trim()
    if (line.length === 0 || line.charAt(0) === "#" || line.indexOf("file://") !== 0) {
      continue
    }
    out.push(decodedPath(line.substring("file://".length)))
  }
  return out
}

// A URI another toolkit wrote can carry a percent escape that is not one, and decodeURIComponent
// throws on it: the raw text is then the best answer this has, and the rest of the drop still lands.
function decodedPath(text) {
  try {
    return decodeURIComponent(text)
  } catch (e) {
    return text
  }
}

// Issue 255 (liri2006): where the rail sits on screen, so the card's mask can cut exactly its strip.
function railStrip(edge, screenW, screenH, w, h) {
  if (edge === "left") {
    return { x: 0, y: (screenH - h) / 2, width: w, height: h }
  }
  if (edge === "right") {
    return { x: screenW - w, y: (screenH - h) / 2, width: w, height: h }
  }
  if (edge === "bottom") {
    return { x: (screenW - w) / 2, y: screenH - h, width: w, height: h }
  }
  return { x: 0, y: 0, width: 0, height: 0 }
}

// ---- Summon: the bell, the cleared transient and the Recent piles rows ----

// Sample input, the whole of summon.json: {"summon":7}
// The bind writes a count, not a state: every write is one more ring, and a file that is missing,
// half written or not JSON answers -1, which is no reading rather than a count of zero.
function ringsOf(text) {
  var doc
  try {
    doc = JSON.parse(String(text || ""))
  } catch (e) {
    return -1
  }
  var n = doc ? Number(doc.summon) : -1
  return isFinite(n) && n >= 0 ? n : -1
}

// Summon: clearing is reversible, and the transient says so for the four seconds it lives.
function clearedText(count) {
  var n = Number(count)
  if (!isFinite(n) || n <= 0) {
    return ""
  }
  return fill(n === 1 ? WORDS.clearedOne : WORDS.clearedMany, grouped(n))
}

// Sample input, the one line `flea shelf undo` answers with: what it reversed, then how many items
// that was. `none 0` when there was nothing to reverse at all.
function undone(text) {
  var parts = String(text || "").trim().split(" ")
  var count = parseInt(parts[1], 10)
  return { kind: parts[0] || "none", count: isFinite(count) ? count : 0 }
}

// The pane says "Undid the move." for the same key, so the card says the same thing about the same
// reversal, and names the pile when that is what came back instead.
function undoneText(kind, count) {
  var n = Number(count)
  if (kind === "move" && n > 0) {
    return "Undid the move"
  }
  if (kind === "pile" && n > 0) {
    return fill(n === 1 ? WORDS.putBackOne : WORDS.putBackMany, grouped(n))
  }
  return "Nothing to undo"
}

// Sample input, one line per kept pile, newest first: the time it was cleared, then how many it held.
// 1789426925000 4
function parsePiles(text) {
  var lines = String(text || "").split("\n")
  var out = []
  for (var i = 0; i < lines.length; i++) {
    var parts = lines[i].split(" ")
    if (parts.length !== 2) {
      continue
    }
    var at = Number(parts[0])
    var count = Number(parts[1])
    if (!isFinite(at) || !isFinite(count)) {
      continue
    }
    out.push({ at: at, count: count })
  }
  return out
}

// The one date format this project draws, which is Flea's own: 2026-09-12 19:04.
function pileText(pile) {
  var n = Number(pile.count)
  return grouped(n) + (n === 1 ? " item" : " items") + " \u00b7 " + stamp(pile.at)
}

// The archive's own date, which is the stamp cut at its day: 2026-09-12.
function today() {
  return stamp(Date.now()).substring(0, DATE_CHARS)
}

var DATE_CHARS = 10

function stamp(at) {
  var when = new Date(Number(at))
  if (!isFinite(when.getTime())) {
    return ""
  }
  return when.getFullYear() + "-" + pad2(when.getMonth() + 1) + "-" + pad2(when.getDate())
         + " " + pad2(when.getHours()) + ":" + pad2(when.getMinutes())
}

// ---- Flea's own settings, which this plugin reads and never writes ----

// Sample input, the part of ~/.local/state/flea/ui.json this reads:
// {"keyHints":false,"shelf":{"screenshots":true,"recordings":true,"recent":3}}
function keyHintsOf(text) {
  var doc = parsedOr(text)
  return doc.keyHints === true
}

function shelfDefaults() {
  return { enabled: false, bar: true, rail: "off", screenshots: true, recordings: true, recent: 3 }
}

var EDGES = ["off", "left", "right", "bottom"]
// The most the card will list, which is the largest stop Settings offers plus the one above it.
var MOST_CAPTURES = 6

// SettingsRest rules 1 to 3: Flea's Settings owns the shelf's switches and this plugin only reads
// them. A value this build cannot honour falls back to the same default a fresh ui.json holds.
function shelfOf(text) {
  var doc = parsedOr(text)
  var shelf = doc.shelf && typeof doc.shelf === "object" ? doc.shelf : {}
  var recent = Number(shelf.recent)
  return {
    enabled: shelf.enabled === true,
    bar: shelf.bar !== false,
    rail: EDGES.indexOf(String(shelf.rail)) >= 0 ? String(shelf.rail) : "off",
    screenshots: shelf.screenshots !== false,
    recordings: shelf.recordings !== false,
    recent: isFinite(recent) && recent >= 0 && recent <= MOST_CAPTURES ? recent : 3
  }
}

function parsedOr(text) {
  try {
    var doc = JSON.parse(String(text || ""))
    return doc && typeof doc === "object" ? doc : {}
  } catch (e) {
    return {}
  }
}

// Which kinds the captures listing is asked for, which is one word on the command line. With neither
// kind checked there is nothing to ask for, and the card draws no captures group at all.
function kindsArg(settings) {
  if (settings.screenshots && settings.recordings) {
    return "both"
  }
  return settings.screenshots ? "screenshots" : "recordings"
}

function wantsCaptures(settings) {
  return settings.recent > 0 && (settings.screenshots || settings.recordings)
}

// ---- Main rules 9 and 10: the card is one list of three sections ----

var PILE = "pile"
var PINNED = "pinned"
var CAPTURE = "capture"
var CAPTURES_CAPTION = "Screenshots & recordings"

// Every row the card draws, in drawn order, each carrying the section it belongs to: the pile, the
// pins that are always there, and the newest captures. A caption is drawn where the section changes.
function rows(pile, captures, sizes) {
  var out = []
  var items = (pile && pile.items) || []
  for (var i = 0; i < items.length; i++) {
    if (!items[i].pinned) {
      out.push(sectioned(items[i], PILE))
    }
  }
  for (var j = 0; j < items.length; j++) {
    if (items[j].pinned) {
      out.push(sectioned(items[j], PINNED))
    }
  }
  for (var k = 0; k < (captures || []).length; k++) {
    var capture = captures[k]
    out.push(sectioned({ path: capture.path, name: leaf(capture.path), bytes: -1, folder: false,
                         partial: false, pinned: false, recording: capture.recording === true,
                         at: capture.at }, CAPTURE))
  }
  return sizes === undefined ? out : sizedRows(out, sizes)
}

function sectioned(item, section) {
  return { path: item.path, name: item.name, bytes: item.bytes, folder: item.folder,
           partial: item.partial, pinned: item.pinned, recording: item.recording === true,
           at: item.at, section: section }
}

// The same answers the pile's rows take, applied to every drawn row: a capture has no size of its
// own until the card asks for one.
function sizedRows(list, sizes) {
  var out = []
  for (var i = 0; i < list.length; i++) {
    var row = list[i]
    var answer = row.bytes < 0 ? sizes[row.path] : undefined
    if (answer === undefined) {
      out.push(row)
      continue
    }
    var filled = sectioned(row, row.section)
    filled.bytes = answer.bytes
    filled.partial = answer.partial
    out.push(filled)
  }
  return out
}

// A caption is drawn above the first row of a section, and never above the pile's own.
function captionFor(list, index, kinds) {
  var row = list[index]
  if (!row || row.section === PILE) {
    return ""
  }
  if (index > 0 && list[index - 1].section === row.section) {
    return ""
  }
  return row.section === PINNED ? "Pinned" : capturesCaption(kinds)
}

// Rule 4: the rows the card is drawing that have no answer yet, so a path is asked for once and a
// closed card asks for nothing.
function thumbWanted(list, thumbs) {
  var out = []
  for (var i = 0; i < list.length; i++) {
    var path = list[i].path
    if (thumbs[path] === undefined && out.indexOf(path) < 0) {
      out.push(path)
    }
  }
  return out
}

// Sample input, one line per path asked for, the cache file or `none`, a tab, then the path:
// /home/gm/.cache/thumbnails/large/714c8a7d754b5cbc79c30b7ad0646785.png\t/home/gm/Pictures/shot.png
function thumbsFrom(text, current) {
  var next = {}
  for (var path in current) {
    next[path] = current[path]
  }
  var lines = String(text || "").split("\n")
  for (var i = 0; i < lines.length; i++) {
    var at = lines[i].indexOf("\t")
    if (at <= 0) {
      continue
    }
    var file = lines[i].substring(0, at)
    next[lines[i].substring(at + 1)] = file === "none" ? "" : file
  }
  return next
}

// Every path of a batch the run did not name, answered as no thumbnail: a run that failed outright
// would otherwise leave them all wanted and the caller would ask again at once.
function thumbsNone(asked, thumbs) {
  var next = {}
  for (var path in thumbs) {
    next[path] = thumbs[path]
  }
  for (var i = 0; i < asked.length; i++) {
    if (next[asked[i]] === undefined) {
      next[asked[i]] = ""
    }
  }
  return next
}

// A path that answered with nothing is asked once more on the next open: the cache records its own
// failures, so a second ask is a lookup, and a thumbnail produced meanwhile is then drawn.
function thumbsFound(thumbs) {
  var kept = {}
  for (var path in thumbs) {
    if (thumbs[path]) {
      kept[path] = thumbs[path]
    }
  }
  return kept
}

// A thumbnail path is not a thumbnail: the cache file can be evicted between the answer and the
// decode, and Row.qml marks such a row by its kind instead. This says which file to try.
function thumbFor(thumbs, item) {
  var file = item && thumbs ? thumbs[item.path] : ""
  return file === undefined || file === null ? "" : file
}

// Rule 9: the caption names the kinds that are checked, which is Settings' own answer.
function capturesCaption(kinds) {
  if (!kinds || (kinds.screenshots && kinds.recordings)) {
    return CAPTURES_CAPTION
  }
  return kinds.screenshots ? "Screenshots" : "Recordings"
}

// ---- Keys: the subset gesture, which every file action reads ----

// Chosen by path rather than by row, so a pile that changes underneath cannot leave a row chosen by
// position alone. The map is rebuilt on every change, so a binding that reads it re-evaluates.
function toggleChosen(chosen, path) {
  var next = {}
  for (var held in chosen) {
    next[held] = chosen[held]
  }
  if (next[path]) {
    delete next[path]
  } else {
    next[path] = true
  }
  return next
}

// shift-j and shift-k extend a range, which is the rows between where the cursor was and where it is.
function chooseRange(chosen, items, from, to) {
  var next = {}
  for (var held in chosen) {
    next[held] = chosen[held]
  }
  var first = Math.min(from, to)
  var last = Math.max(from, to)
  for (var i = first; i <= last; i++) {
    if (items[i]) {
      next[items[i].path] = true
    }
  }
  return next
}

// ctrl-a takes all, and a second ctrl-a clears: the same key both ways, as it is in the pane.
function chooseAll(chosen, items) {
  if (chosenCount(chosen, items) === items.length && items.length > 0) {
    return {}
  }
  var next = {}
  for (var i = 0; i < items.length; i++) {
    next[items[i].path] = true
  }
  return next
}

// Only the rows the pile still holds count: a chosen row that has left the shelf is not chosen.
function chosenCount(chosen, items) {
  var n = 0
  for (var i = 0; i < items.length; i++) {
    if (chosen[items[i].path]) {
      n += 1
    }
  }
  return n
}

// Every file action is chosen-or-whole, with no exceptions: the chosen rows, or the whole pile when
// nothing is chosen. Order is the pile's own, never the order they were chosen in. Rule 9: a capture
// row is reached by choosing it, and is never part of "the whole pile", which is what Keys says the
// five actions take when nothing is chosen.
function actionPaths(chosen, items) {
  var picked = []
  for (var i = 0; i < items.length; i++) {
    if (chosen[items[i].path]) {
      picked.push(items[i].path)
    }
  }
  if (picked.length > 0) {
    return picked
  }
  var all = []
  for (var j = 0; j < items.length; j++) {
    if (items[j].section !== CAPTURE) {
      all.push(items[j].path)
    }
  }
  return all
}

// Rule 11: a drag carries the chosen rows, or the one row it was started from when none are chosen.
// A keyboard action still takes the whole pile, because an action names no row and a grab does.
function carryPaths(chosen, items, index) {
  var picked = []
  for (var i = 0; i < items.length; i++) {
    if (chosen[items[i].path]) {
      picked.push(items[i].path)
    }
  }
  if (picked.length > 0) {
    return picked
  }
  return items[index] ? [items[index].path] : []
}

// Whether every path named is already pinned, which is what makes the strip's Pin an unpin.
function allPinned(paths, items) {
  var seen = 0
  for (var i = 0; i < items.length; i++) {
    if (paths.indexOf(items[i].path) >= 0) {
      if (!items[i].pinned) {
        return false
      }
      seen += 1
    }
  }
  return seen > 0
}

// How many rows "none chosen" would take, which is the pile and its pinned rows and no capture.
function wholeCount(items) {
  var n = 0
  for (var i = 0; i < items.length; i++) {
    if (items[i].section !== CAPTURE) {
      n += 1
    }
  }
  return n
}

// Rule 8: an action's tooltip is its name, and its key rides along only while Flea's hints are on.
function actionTip(label, key, hints) {
  return hints ? label + "  " + key : label
}

// Rules 9 and 10: a capture row and a pinned row leave as a copy, so a drag carrying either one is
// a copy however it was started, and only a set of plain pile rows can be a move.
function dragMoves(paths, items, wantsMove) {
  if (!wantsMove) {
    return false
  }
  for (var i = 0; i < items.length; i++) {
    if (items[i].section !== PILE && paths.indexOf(items[i].path) >= 0) {
      return false
    }
  }
  return true
}




// Main rule 12: the header counts the pile alone, because pinned rows and captures are not items
// anybody sent to the shelf.
function loose(items) {
  var out = []
  for (var i = 0; i < (items || []).length; i++) {
    if (!items[i].pinned) {
      out.push(items[i])
    }
  }
  return out
}
