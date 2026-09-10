.pragma library
.import "PathBar.js" as PathBar

// What a line typed into the picker's location field means, and nothing about the field or the
// window: this is the picker's twin of ui/js/PathBar.js, which it leans on for the path half. The
// line behaves like the filename box of the Windows file dialog: a path navigates or selects, a URL
// is fetched or mounted. Every function is pure, so tests/js/pickerentry.js drives it with no window.
//
// classify answers one of five kinds. "empty" is nothing typed. "local" carries a path the caller
// peeks before it decides between a directory and a file. "remote" is an http, https, ftp or ftps
// URL to download. "share" is an smb or sftp URL to mount and browse. "refused" carries a reason
// the field can turn into a sentence: "invalid", "scheme" or "host".

var REMOTE = ["http", "https", "ftp", "ftps"]
var SHARE = ["smb", "sftp", "ssh"]

// A scheme is a word and a colon at the very start of the line. RFC 3986 allows one letter, so "c:"
// is a scheme here too; a local name with a colon in its first segment is typed as "./a:b".
var SCHEME = /^([a-z][a-z0-9+.\-]*):/i

function answer(kind, fields) {
    var out = { kind: kind, path: "", url: "", scheme: "", wantsDir: false, reason: "" }
    for (var key in fields) {
        out[key] = fields[key]
    }
    return out
}

function refused(reason) {
    return answer("refused", { reason: reason })
}

// The typed line as an absolute path. PathBar.resolve reads "~", a relative name and dot segments
// the way the main window's bar does; "~user/x" is not expanded, it stays a name under the current
// directory, the same as the bar. The trailing slash is read before resolve drops it: it is how the
// user says "this is a directory", and the caller opens it rather than picking it.
function local(body, current, home) {
    var path = PathBar.resolve(body, current, home)
    if (path.length === 0) {
        return answer("empty")
    }
    return answer("local", { path: path, wantsDir: body.charAt(body.length - 1) === "/" })
}

// The path part of a URL, cut before any query or fragment, so "?dir=1" never reads as a directory.
function urlPath(rest) {
    var cut = rest.indexOf("/")
    var path = cut < 0 ? "" : rest.substring(cut)
    var stop = path.search(/[?#]/)
    return stop < 0 ? path : path.substring(0, stop)
}

function classify(text, current, home) {
    var line = String(text).trim()
    if (line.length === 0) {
        return answer("empty")
    }
    // No path and no URL carries a NUL; one is a paste gone wrong, not a name to look up.
    if (line.indexOf("\u0000") >= 0) {
        return refused("invalid")
    }
    var match = SCHEME.exec(line)
    if (match === null) {
        return local(line, current, home)
    }
    var scheme = match[1].toLowerCase()
    var rest = line.substring(match[0].length)
    // "data:" and "mailto:" have no authority and name nothing this dialog can open.
    if (rest.indexOf("//") !== 0) {
        return refused("scheme")
    }
    rest = rest.substring(2)
    // The authority is what stands before the first "/", "?" or "#". A URL with none names no host,
    // so "http:///file" is refused with "http://"; "file://" alone is the root and stays.
    var stop = rest.search(/[\/?#]/)
    var authority = stop < 0 ? rest : rest.substring(0, stop)
    if (authority.length === 0 && scheme !== "file") {
        return refused("host")
    }
    if (scheme === "file") {
        // PathBar.unwrap knows the two local spellings of the authority and decodes the rest. It
        // answers "" only for another host, because a local URI with no path decodes to "/".
        var body = PathBar.unwrap("file://" + rest)
        if (body.length === 0) {
            return refused("host")
        }
        return local(body, current, home)
    }
    if (REMOTE.indexOf(scheme) >= 0) {
        var path = urlPath(rest)
        return answer("remote", { scheme: scheme, url: scheme + "://" + rest,
                                  wantsDir: path.charAt(path.length - 1) === "/" })
    }
    if (SHARE.indexOf(scheme) >= 0) {
        // gvfs reads "ssh" as an alias of "sftp"; the answer says the name gio mount will report.
        var canonical = scheme === "ssh" ? "sftp" : scheme
        return answer("share", { scheme: canonical, url: canonical + "://" + rest })
    }
    return refused("scheme")
}
