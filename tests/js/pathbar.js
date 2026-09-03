.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/PathBar.js" as PathBar

// The path bar's whole meaning is what a typed line resolves to, and every one of those lines is a
// navigation the user cannot see before it happens: a wrong tilde or a swallowed ".." opens the
// wrong directory silently. So the resolution is asserted here, with no window and no backend.

var HOME = "/home/gm"

// Only the members Focus.handleKey touches on its way to the path bar, and the counter for the one
// call it must make: the pane asks, and ui/shell.qml is what opens the field. The routing lives in
// this suite rather than in tests/js/focus.js, which is at its own hard cap.
function barPane(view) {
    return {
        focusView: view, viewMode: "list", searchMode: "", filterTyping: false,
        inputAt: 0, rowsAt: 0, asked: 0,
        preview: { active: false, isMedia: false, isPdf: false },
        shareBrowser: { active: false },
        keymapSheet: { open: function () {} },
        sidebar: { renameEditor: function () { return null } },
        renameEditor: function () { return null },
        pathBarRequested: function () { this.asked += 1 }
    }
}

function key(code, text, modifiers) {
    return { key: code, text: text, modifiers: modifiers }
}

function names(list) {
    var out = []
    for (var i = 0; i < list.length; i++) {
        out.push(list[i])
    }
    return out
}

function run(check) {
    // The fold only runs when nothing matched exactly, so its matches can disagree in case inside
    // the leaf. Before the guard this returned "R" for a typed "re" and said nothing about it.
    var folded = PathBar.complete("~/re", ["README", "ReadMe.bak"])
    check("a case-folded completion never hands back less than was typed", folded.text, "~/re")
    check("and it still reports how many share it", folded.matches, 2)
    check("so the sentence names the count rather than staying silent",
          PathBar.completionMessage("~/re", folded, "/home/gm"), "2 names share that prefix.")
    // The ordinary fold, where the common prefix is longer than the leaf, still grows the line.
    var grows = PathBar.complete("~/de", ["Desktop"])
    check("a lone case-folded match still completes to the real name", grows.text, "~/Desktop/")

    // The three shapes a line can take, which are the three the field's own prefill relies on.
    check("an absolute path is itself", PathBar.resolve("/etc", HOME, HOME), "/etc")
    check("a tilde is the home directory", PathBar.resolve("~", HOME, HOME), HOME)
    check("a tilde and a child", PathBar.resolve("~/Downloads", HOME, HOME), "/home/gm/Downloads")
    check("a bare name is relative to where the pane is",
          PathBar.resolve("Downloads", HOME, HOME), "/home/gm/Downloads")
    check("a relative line resolves against the pane and not against home",
          PathBar.resolve("src", "/usr", HOME), "/usr/src")

    // A tilde with no home published is a name, because expanding it to "" would open the root.
    check("no home leaves a tilde as the name it is",
          PathBar.resolve("~", "/etc", ""), "/etc/~")

    // Nothing typed is not a path, and the field is what turns that into "close and stay put".
    check("an empty line is not a path", PathBar.resolve("", HOME, HOME), "")
    check("whitespace alone is not a path either", PathBar.resolve("   ", HOME, HOME), "")

    // Trailing and doubled slashes come from typing and from completion both, and neither is a
    // directory of its own: the pane's path never carries one, so the bar's answer must not either.
    check("a trailing slash is dropped", PathBar.resolve("/etc/", HOME, HOME), "/etc")
    check("a doubled slash collapses", PathBar.resolve("/etc//apt", HOME, HOME), "/etc/apt")
    check("the root survives being the whole line", PathBar.resolve("/", HOME, HOME), "/")
    check("the root survives a trailing slash of its own", PathBar.normalize("//"), "/")

    // Interior dots are resolved here rather than sent on, or the chrome would draw a path the
    // backend had already read as another one.
    check("a dot segment is dropped", PathBar.resolve("/etc/./apt", HOME, HOME), "/etc/apt")
    check("a dot dot climbs", PathBar.resolve("/etc/apt/..", HOME, HOME), "/etc")
    check("a dot dot climbs from a relative line too",
          PathBar.resolve("../Music", "/home/gm/Pictures", HOME), "/home/gm/Music")
    check("climbing stops at the root, as the up key does",
          PathBar.resolve("/../../..", HOME, HOME), "/")

    // A paste from another application, and what --select already accepts on the command line.
    check("a file URI is a path", PathBar.resolve("file:///etc/apt", HOME, HOME), "/etc/apt")
    check("a percent-encoded URI decodes",
          PathBar.resolve("file:///home/gm/My%20Files", HOME, HOME), "/home/gm/My Files")
    // A literal percent is a legal filename, so a URI that will not decode is kept, not dropped.
    check("an undecodable URI is kept as it stands",
          PathBar.resolve("file:///tmp/100%", HOME, HOME), "/tmp/100%")

    // The authority is part of the URI and not part of the path. RFC 8089 spells the local one two
    // ways, and the empty one above is the common one; stripping the scheme alone read the other as
    // a relative walk into <current>/localhost/etc, which is a directory nobody named.
    check("a localhost authority is this machine",
          PathBar.resolve("file://localhost/etc", HOME, HOME), "/etc")
    check("and its case does not matter, as a host name's never does",
          PathBar.resolve("file://LOCALHOST/etc/apt", HOME, HOME), "/etc/apt")
    check("a bare file://localhost is the root it names",
          PathBar.resolve("file://localhost", HOME, HOME), "/")
    // Another host is not a path this bar can open, and answering the parent's own directory for it
    // would be worse than answering nothing: refused() is what turns that into a sentence.
    check("a URI on another host resolves to nothing",
          PathBar.resolve("file://otherbox/etc", HOME, HOME), "")
    check("and says it was refused rather than empty", PathBar.refused("file://otherbox/etc"), true)
    check("an empty line is not a refusal, so it stays silent", PathBar.refused("   "), false)
    check("an ordinary path is not a refusal either", PathBar.refused("/etc"), false)

    // Where a completion reads, which is the head of the line and never the half-typed leaf.
    check("the completion directory is the head of the line",
          PathBar.completionDir("/etc/ap", HOME, HOME), "/etc")
    check("a line with no slash completes in the pane's own directory",
          PathBar.completionDir("Dow", HOME, HOME), HOME)
    check("a line ending in a slash completes inside it",
          PathBar.completionDir("/etc/apt/", HOME, HOME), "/etc/apt")
    check("a tilde head resolves before the peek", PathBar.completionDir("~/Do", HOME, HOME), HOME)

    // A dotted leaf asks for hidden entries whatever the listing is set to, or Tab would answer off
    // rows the peek never returned and read as "nothing starts with that".
    check("a dotted leaf peeks hidden", PathBar.wantsHidden("~/.co", false), true)
    check("an ordinary leaf peeks as the listing is set", PathBar.wantsHidden("~/Do", false), false)
    check("and follows the listing when it shows hidden", PathBar.wantsHidden("~/Do", true), true)

    // What a completion reply is matched on. ui/ColumnsArea.qml peeks the pane's ancestors on the
    // same signal with the listing's own hidden flag, and a Tab that gains a dot asks the same
    // directory again with the other flag, so a reply matched on the path alone could answer a
    // hidden request off rows that carried no dotfiles.
    check("the same directory and flag is the same request",
          PathBar.requestKey("/home/gm", true), PathBar.requestKey("/home/gm", true))
    check("the flag alone tells two requests for one directory apart",
          PathBar.requestKey("/home/gm", true) === PathBar.requestKey("/home/gm", false), false)
    check("and the directory alone still tells two apart",
          PathBar.requestKey("/etc", false) === PathBar.requestKey("/home/gm", false), false)
    // The out-of-order case: the visible reply for the directory the hidden request is waiting on
    // must not be taken for that request's answer.
    var pending = PathBar.requestKey("/home/gm", true)
    check("a visible reply is not the hidden request's answer",
          PathBar.requestKey("/home/gm", false) === pending, false)
    check("the hidden reply is", PathBar.requestKey("/home/gm", true) === pending, true)

    var dirs = names(["Desktop", "Documents", "Downloads", "Dropbox", "Music"])

    // One match is a whole name, so the separator goes on and the next Tab walks a level deeper.
    var one = PathBar.complete("/home/gm/Mu", ["Music", "Pictures"])
    check("a single match completes the name", one.text, "/home/gm/Music/")
    check("and says it was the only one", one.matches, 1)

    // Several share a prefix, which is exactly as far as the line can honestly go.
    var many = PathBar.complete("/home/gm/D", dirs)
    check("several matches grow the line to their common prefix", many.text, "/home/gm/D")
    check("and count", many.matches, 4)
    check("the common prefix is taken from the names and not from the leaf",
          PathBar.complete("/home/gm/Do", dirs).text, "/home/gm/Do")
    check("a prefix that only two names share grows further",
          PathBar.complete("/home/gm/Doc", dirs).text, "/home/gm/Documents/")

    // An empty leaf with one entry behind it still completes, which is what makes Tab on a fresh
    // slash useful rather than silent.
    check("an empty leaf completes a lone entry",
          PathBar.complete("/srv/", ["www"]).text, "/srv/www/")

    check("nothing matching leaves the line exactly as it was",
          PathBar.complete("/home/gm/zz", dirs).text, "/home/gm/zz")
    check("and says nothing matched", PathBar.complete("/home/gm/zz", dirs).matches, 0)

    // The filesystem is case sensitive, so a fold may never quietly pick one of two real names;
    // it is only reached when the honest match found nothing at all.
    var folded = PathBar.complete("/home/gm/dow", dirs)
    check("a case-insensitive match is the fallback and not the rule", folded.text, "/home/gm/Downloads/")
    check("an exact-case match wins over one that needs a fold",
          PathBar.complete("/home/gm/D", ["Dev", "dev"]).text, "/home/gm/Dev/")

    // What Tab says when it changed nothing. Silence is the usual answer and has to stay silent.
    check("a line that moved says nothing",
          PathBar.completionMessage("/home/gm/Mu", one, HOME), "")
    check("a line that could not move names the directory it read",
          PathBar.completionMessage("/home/gm/zz", PathBar.complete("/home/gm/zz", dirs), HOME),
          "Nothing in /home/gm starts with that.")
    check("a line already at the common prefix says how many share it",
          PathBar.completionMessage("/home/gm/D", many, HOME), "4 names share that prefix.")

    // The key half. The bar is drawn in the chrome above both views, so it is global the way the
    // keymap sheet is: the rail has to reach it rather than dropping the key on the floor.
    var colon = key(Qt.Key_Colon, ":", Qt.ShiftModifier)
    var fromList = barPane("list")
    check("colon is consumed in the list", Focus.handleKey(colon, fromList, fromList.sidebar), true)
    check("and asks the shell to open the bar", fromList.asked, 1)
    var fromRail = barPane("rail")
    Focus.handleKey(key(Qt.Key_L, "l", Qt.ControlModifier), fromRail, fromRail.sidebar)
    check("ctrl-l opens it from the rail as well", fromRail.asked, 1)
}
