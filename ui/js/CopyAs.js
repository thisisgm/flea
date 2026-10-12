.pragma library

.import "Format.js" as Format

// Sample input: "/home/gm/archive.tar.gz"; every Copy as variant covers the
// whole selection, one path per line, through wl-copy.
function leaf(path) {
    var text = String(path)
    var cut = text.lastIndexOf("/")
    return cut < 0 ? text : text.substring(cut + 1)
}

// Sample input: "/home/gm/archive.tar.gz" -> "archive.tar"; a dotfile keeps
// its whole name, so ".bashrc" never becomes an empty stem.
function stem(path) {
    var name = leaf(path)
    var start = name.charAt(0) === "." ? 1 : 0
    var dot = name.lastIndexOf(".")
    if (dot <= start)
        return name
    return name.substring(0, dot)
}

// Sample input: "/home/gm/a.txt" -> "/home/gm"; "/" stays "/".
function dirpath(path) {
    var text = String(path)
    var cut = text.lastIndexOf("/")
    if (cut < 0)
        return text
    return cut === 0 ? "/" : text.substring(0, cut)
}

// Sample input: "/home/gm/a b.txt"; the file URI a clipboard reader expects.
function uri(path) {
    return Format.fileUri(String(path))
}

// Sample input: path "/home/gm/proj/src/a.rs", base "/home/gm/proj" -> "src/a.rs".
// The listed directory is the base, so a search hit keeps the folders under it.
function relative(path, base) {
    var p = String(path)
    var b = String(base || "")
    if (!b)
        return p
    while (p.length > 1 && p.charAt(p.length - 1) === "/")
        p = p.substring(0, p.length - 1)
    while (b.length > 1 && b.charAt(b.length - 1) === "/")
        b = b.substring(0, b.length - 1)
    if (p === b)
        return "."
    if (b === "/")
        return p.charAt(0) === "/" ? p.substring(1) : p
    if (p.indexOf(b + "/") === 0)
        return p.substring(b.length + 1)
    var pParts = p.split("/")
    var bParts = b.split("/")
    if (pParts[pParts.length - 1] === "")
        pParts.pop()
    if (bParts[bParts.length - 1] === "")
        bParts.pop()
    var i = 0
    while (i < pParts.length && i < bParts.length && pParts[i] === bParts[i])
        i++
    var out = []
    var u
    for (u = i; u < bParts.length; u++)
        if (bParts[u] !== "")
            out.push("..")
    for (u = i; u < pParts.length; u++)
        if (pParts[u] !== "")
            out.push(pParts[u])
    return out.length ? out.join("/") : "."
}

function one(path, kind, base) {
    if (kind === "name")
        return leaf(path)
    if (kind === "stem")
        return stem(path)
    if (kind === "dirpath")
        return dirpath(path)
    if (kind === "uri")
        return uri(path)
    if (kind === "quoted")
        return "'" + String(path).split("'").join("'\\''") + "'"
    if (kind === "relative")
        return relative(path, base)
    return String(path)
}

function lines(paths, kind, base) {
    var out = []
    for (var i = 0; i < paths.length; i++)
        out.push(one(paths[i], kind, base))
    return out.join("\n")
}
