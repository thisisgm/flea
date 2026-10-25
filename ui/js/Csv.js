.pragma library

.import "Format.js" as Format

// The rows a CSV or TSV preview reads its records from, and the words its count line uses.
var ROW_ONE = "1 row"
var ROW_MANY = "%1 rows"

// The field separator for a table by extension, in any case; "" for any other name. Sample input: "/home/gm/bench.csv" gives ",", "/home/gm/DATA.Tsv" gives a tab, "notes.txt" gives "".
function delimiter(path) {
    var name = String(path)
    var dot = name.lastIndexOf(".")
    if (dot < 0)
        return ""
    var ext = name.substring(dot + 1).toLowerCase()
    return ext === "csv" ? "," : ext === "tsv" ? "\t" : ""
}

// Records of whole lines, each {cells, span, open}. Sample input: ["h", "1,\"two", "lines\""] with "," gives two records.
function records(lines, delim) {
    var out = []
    var at = 0
    while (at < lines.length) {
        var rec = record(lines, at, delim)
        out.push(rec)
        at += rec.span
    }
    return out
}

// One record from lines[at]. A quoted field may run on into the next line, its line end kept as a newline. Sample input: ["1,\"two", "lines\",3"] gives cells ["1", "two\nlines", "3"] over span 2, and open true when the quote never closes.
function record(lines, at, delim) {
    var cells = []
    var field = ""
    var quoting = false
    var fieldStart = true
    var i = at
    while (true) {
        var text = lines[i]
        for (var p = 0; p < text.length; p++) {
            var c = text.charAt(p)
            if (quoting) {
                if (c !== "\"")
                    field += c
                else if (text.charAt(p + 1) === "\"") {
                    field += "\""
                    p += 1
                } else
                    quoting = false
            } else if (c === delim) {
                cells.push(field)
                field = ""
                fieldStart = true
                continue
            } else if (c === "\"" && fieldStart) {
                quoting = true
            } else {
                field += c
            }
            fieldStart = false
        }
        if (!quoting) {
            cells.push(field)
            return { cells: cells, span: i - at + 1, open: false }
        }
        i += 1
        if (i >= lines.length)
            return { cells: cells.concat([field]), span: i - at, open: true }
        field += "\n"
    }
}

// The row padded with empty cells to the table's width; a longer row keeps every cell. Sample input: padded(["c"], 3) gives ["c", "", ""].
function padded(cells, width) {
    var out = cells.slice()
    while (out.length < width)
        out.push("")
    return out
}

// The widest row's cell count, which is the table's column count.
function columnCount(rows) {
    var n = 0
    for (var i = 0; i < rows.length; i++)
        n = Math.max(n, rows[i].length)
    return n
}

// A non-empty cell that parses as a finite number once its spaces are trimmed.
function isNumber(text) {
    var value = String(text).trim()
    return value !== "" && isFinite(Number(value))
}

// Whether each column is right-aligned: every non-empty body cell in it is a number. Rows[0] is the header and is not read. Sample input: [["a", "n"], ["x", "1"], ["y", ""]] with 2 columns gives [false, true].
function numericColumns(rows, columns) {
    var out = []
    for (var c = 0; c < columns; c++) {
        var seen = false
        var numeric = true
        for (var r = 1; r < rows.length && numeric; r++) {
            var text = rows[r][c] === undefined ? "" : String(rows[r][c])
            if (text.trim() === "")
                continue
            seen = true
            numeric = isNumber(text)
        }
        out.push(seen && numeric)
    }
    return out
}

// Each whole-record block carries its starting ordinal. Sample input: {1:{t:"h"},5:{t:"5",record:3}} keeps record 3 at line 5.
function table(held, total, delim) {
    var last = 0
    for (var k in held)
        last = Math.max(last, Number(k))
    var all = []
    var open = []
    var i = 1
    while (i <= last) {
        if (!held[i]) {
            i += 1
            continue
        }
        var start = i
        var lines = []
        while (i <= last && held[i])
            lines.push(held[i++].t)
        if (start !== 1 && held[start].record === undefined)
            continue
        var at = start
        var ordinal = held[start].record || 1
        var recs = records(lines, delim)
        for (var r = 0; r < recs.length; r++) {
            if (recs[r].open && start + lines.length - 1 < total) {
                open.push({ line: at, to: start + lines.length - 1, record: ordinal })
                break
            }
            all.push({ line: at, record: ordinal++, span: recs[r].span, cells: recs[r].cells })
            at += recs[r].span
        }
    }
    if (all.length === 0 || all[0].line !== 1)
        return { header: null, rows: [], byLine: {}, byRecord: {}, cols: 0, open: open }
    var cols = columnCount(all.map(function (rec) { return rec.cells }))
    var rows = all.slice(1).map(function (rec) { return { line: rec.line, record: rec.record, span: rec.span, cells: padded(rec.cells, cols) } })
    var byLine = {}
    var byRecord = {}
    for (var j = 0; j < rows.length; j++) {
        byLine[rows[j].line] = rows[j]
        byRecord[rows[j].record] = rows[j]
    }
    return { header: padded(all[0].cells, cols), rows: rows, byLine: byLine, byRecord: byRecord, cols: cols, open: open }
}

// First-screen readiness rejects any unfinished record that overlaps its ordinals; whole aligned replies leave open empty.
function ready(t, to) {
    return t.open.every(function (rec) { return rec.record > to })
}

// Match the backend's raw lines: "a,b\n1,2\n" has two lines, and a trailing CR is removed from each.
function plainLines(text) {
    var lines = text.length > 0 ? text.split("\n") : []
    if (lines.length > 0 && lines[lines.length - 1] === "")
        lines.pop()
    return lines.map(function (line) { return line.endsWith("\r") ? line.slice(0, -1) : line })
}

// A cell drawn on one line: a quoted newline shows as a space, so the row keeps its height. Sample input: oneLine("two\nlines") gives "two lines".
function oneLine(text) {
    return String(text).split("\n").join(" ")
}

// The column's drawn boxes, each {x, w, right}: the first flexes to fill the frame, or to its header when the columns do not fit, and a column past the frame's right edge is not drawn while the last drawn one elides. Sample input: widths [40, 30], heads [20, 10], rights [false, true], gap 6, avail 100 gives x 0 w 64, then x 70 w 30.
function fit(widths, heads, rights, gap, avail) {
    var n = widths.length
    var out = []
    if (n === 0)
        return out
    var total = widths.reduce(function (sum, w) { return sum + w }, 0) + gap * (n - 1)
    var first = total <= avail ? widths[0] + (avail - total) : Math.min(widths[0], heads[0])
    var x = 0
    for (var c = 0; c < n && x < avail; c++) {
        var w = c === 0 ? first : widths[c]
        out.push({ x: x, w: Math.min(w, avail - x), right: rights[c] })
        x += w + gap
    }
    return out
}

// Quick Look's boxes: each column is its widest cell plus the padding on both sides, after the row numbers. Sample input: grid([40], [false], 42, 14) gives [{x: 42, w: 68, right: false}].
function grid(widths, rights, numberWidth, pad) {
    var out = []
    var x = numberWidth
    for (var c = 0; c < widths.length; c++) {
        var w = widths[c] + 2 * pad
        out.push({ x: x, w: w, right: rights[c] })
        x += w
    }
    return out
}

// Records after the header, used by the model and count line. Sample input: dataRows(25) gives 24, and dataRows(0) gives 0.
function dataRows(total) {
    return Math.max(0, total - 1)
}

// The count line in the bar. Sample input: rowCount(24) gives "24 rows" and rowCount(1) gives "1 row".
function rowCount(n) {
    return n === 1 ? ROW_ONE : Format.fill(ROW_MANY, Format.count(n))
}

// The data row number is its record ordinal less the header. Sample input: rowNumber(2) gives 1.
function rowNumber(record) {
    return record - 1
}
