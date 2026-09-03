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
