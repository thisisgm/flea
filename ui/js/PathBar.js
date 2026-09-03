.pragma library

// What a typed path line means, and nothing about the field that carries it: ui/ChromeBar.qml owns
// the field and ui/shell.qml owns the navigation, the same split ui/js/Filter.js keeps with its
// strip. Every function here is pure, so tests/js/pathbar.js drives the whole of it with no window.
//
// The bar reads what the chrome already draws: a tilde is the home directory because that is how
// the path is written above the listing, and a bare name is relative to the directory the pane is
// on, because with the line selected on open that is what one word and Enter has to mean.

// The one place a path is made canonical. Interior "." and ".." are resolved here rather than sent
// to the backend, so the listing is opened on the path the bar will draw and the two never disagree;
// "" is not a path and is what every caller checks for.
function normalize(path) {
    var text = String(path)
    if (text.length === 0) {
        return ""
    }
    var parts = text.split("/")
    var out = []
    for (var i = 0; i < parts.length; i++) {
        var part = parts[i]
        // An empty part is a doubled or a trailing slash, and neither names a directory of its own.
        if (part.length === 0 || part === ".") {
            continue
        }
        if (part === "..") {
            // The root has no parent, which is where climbing stops, exactly as ui/js/Nav.js parent does.
            out.pop()
            continue
        }
        out.push(part)
    }
    return "/" + out.join("/")
}

// A file:// URI is what a paste from another application carries, and Flea's own --select takes one
// too. A URI that will not decode is left as it stands rather than dropped: a literal percent in a
// filename is legal, and refusing the whole line over one would be worse than a listing that fails.
//
// The authority between the third slash and the fourth is part of the URI and not part of the path.
// RFC 8089 gives it two local spellings, the empty one and "localhost", and both mean this machine;
// file://localhost/etc is /etc. Any other authority names a file on another host, which is not a
// path this bar can open, so it answers "" and ui/ChromeBar.qml says so rather than opening the
// authority as a directory of its own: stripping the scheme alone turned that line into a relative
// walk into <current>/localhost/etc, which is a directory nobody named.
var LOCAL_HOSTS = ["", "localhost"]

function unwrap(text) {
    var body = String(text)
    if (body.indexOf("file://") !== 0) {
        return body
    }
    body = body.substring("file://".length)
    // Everything up to the next slash is the authority; with none, the whole remainder is one.
    var cut = body.indexOf("/")
    var authority = cut < 0 ? body : body.substring(0, cut)
    if (LOCAL_HOSTS.indexOf(authority.toLowerCase()) < 0) {
        return ""
    }
    body = cut < 0 ? "/" : body.substring(cut)
    try {
        return decodeURIComponent(body)
    } catch (e) {
        return body
    }
}

// The typed line as an absolute path. Answers "" for a line that names nothing, which is what the
// field checks before it navigates: an empty commit closes the bar and leaves the pane where it is.
function resolve(text, current, home) {
    var line = String(text).trim()
    var body = unwrap(line)
    // Empty either because nothing was typed or because unwrap refused a URI on another host; both
    // answer "" here, and refused() below is what tells the two apart for the sentence.
    if (body.length === 0) {
        return ""
    }
    // The tilde only expands against a home the shell actually published; with none, it is a name.
    if (home.length > 0 && (body === "~" || body.indexOf("~/") === 0)) {
        body = home + body.substring(1)
    } else if (body.charAt(0) !== "/") {
        // Relative to where the pane is, so "Downloads" and Enter is a directory down.
        body = current + "/" + body
    }
    return normalize(body)
}

// The line cut at its last slash: head is what names a directory and leaf is the part being typed,
// which is the only part a completion may rewrite. A line with no slash at all is all leaf.
function split(text) {
    var body = String(text)
    var cut = body.lastIndexOf("/")
    if (cut < 0) {
        return { head: "", leaf: body }
    }
    return { head: body.substring(0, cut + 1), leaf: body.substring(cut + 1) }
}

// Which directory a completion has to read to answer the line. The head alone, because the leaf is
// half a name and would resolve to a directory that does not exist; an empty head is the pane's own.
function completionDir(text, current, home) {
    var head = split(String(text).trim()).head
    if (head.length === 0) {
        return current
    }
    return resolve(head, current, home)
}

// A leaf that starts with a dot is asking for hidden entries whatever the listing is showing, so the
// peek behind Tab is told to include them rather than completing against rows it cannot see.
function wantsHidden(text, showHidden) {
    return showHidden || split(String(text).trim()).leaf.indexOf(".") === 0
}

// What a completion request and its reply are matched on. The peek wire carries no request id, and
// it does not need one: two peeks of the same directory with the same hidden flag answer the same
// rows, so the pair is identity enough, and a reply that differs in either is another asker's.
// ui/ColumnsArea.qml peeks the pane's ancestors on the same signal, with the listing's own hidden
// flag, so without this a Tab on ".conf" could complete against rows that carried no dotfiles at all.
function requestKey(dir, hidden) {
    return String(dir) + "\u0000" + (hidden ? "hidden" : "visible")
}

function commonPrefix(names) {
    if (names.length === 0) {
        return ""
    }
    var prefix = names[0]
    for (var i = 1; i < names.length; i++) {
        var j = 0
        while (j < prefix.length && j < names[i].length && prefix.charAt(j) === names[i].charAt(j)) {
            j++
        }
        prefix = prefix.substring(0, j)
    }
    return prefix
}

function startsWith(name, leaf, fold) {
    var a = fold ? name.toLowerCase() : name
    var b = fold ? leaf.toLowerCase() : leaf
    return a.indexOf(b) === 0
}

// What Tab makes of the names one directory holds. text is the whole line, names are that
// directory's entries; the answer is the line as it should now read and how many entries stand
// behind it, which is what the field uses to decide whether a slash goes on the end.
//
// Case-sensitively first, and only case-insensitively when that matched nothing: the filesystem is
// case sensitive, so "Dev" and "dev" are two directories and a fold must never quietly pick one,
// but a lone "dev" typed at a directory holding only "Dev" is a typist and not an ambiguity.
function complete(text, names) {
    var line = String(text)
    var parts = split(line)
    var matched = []
    var i
    for (i = 0; i < names.length; i++) {
        if (startsWith(names[i], parts.leaf, false)) {
            matched.push(names[i])
        }
    }
    if (matched.length === 0) {
        for (i = 0; i < names.length; i++) {
            if (startsWith(names[i], parts.leaf, true)) {
                matched.push(names[i])
            }
        }
    }
    if (matched.length === 0) {
        return { text: line, matches: 0 }
    }
    var grown = commonPrefix(matched)
    // The fold only runs when nothing matched exactly, so the matches can disagree in case inside
    // the leaf and their common prefix be shorter than it: "re" against README and ReadMe.bak
    // shares only "R", and returning that would delete a character the user typed.
    if (grown.length < parts.leaf.length)
        grown = parts.leaf
    // One match is a whole name, so the line goes on with the separator already typed: Tab, Tab,
    // Tab walks a tree. Several share a prefix, which is as far as the line can honestly go.
    var tail = matched.length === 1 ? grown + "/" : grown
    return { text: parts.head + tail, matches: matched.length }
}

// A line that named something and still resolved to nothing, which is only ever a file:// URI on
// another host: the bar owes that a sentence, where an empty line owes silence.
function refused(text) {
    var line = String(text).trim()
    return line.length > 0 && unwrap(line).length === 0
}

// The sentence Tab answers with when it changed nothing, written for the user and never an action
// name. An empty string means the line moved and the bar says nothing, which is the usual case.
function completionMessage(before, after, dir) {
    if (after.matches === 0) {
        return "Nothing in " + dir + " starts with that."
    }
    if (after.text === before) {
        return after.matches + " names share that prefix."
    }
    return ""
}
