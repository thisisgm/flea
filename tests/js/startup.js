.import "../../ui/js/Startup.js" as Startup

// Settings > View > Opening, which decides where a window opens and where a new tab opens. Before
// 0.2.1 a window always opened on $HOME unless the command line named a path, and a new tab always
// cloned the folder the pane rested on; both are still the defaults, and both are settings now.

var HOME = "/home/gm"

function run(check) {
    // The shipped defaults, which are also what a ui.json holding none of these keys reads as.
    check("a fresh install opens in home", Startup.startPath({}, HOME, ""), HOME)
    check("and no state at all still opens in home", Startup.startPath(null, HOME, ""), HOME)
    check("a fresh install opens a new tab where the pane already is",
          Startup.newTabPath({}, "/home/gm/Pictures", HOME), "/home/gm/Pictures")

    // A path the caller named outranks every setting, the precedence --select already has.
    var chosen = { startIn: "folder", startFolder: "/home/gm/Work" }
    check("a path named on the command line wins over the chosen folder",
          Startup.startPath(chosen, HOME, "/tmp/asked"), "/tmp/asked")
    check("and over home", Startup.startPath({}, HOME, "/tmp/asked"), "/tmp/asked")
    check("the chosen folder opens when nothing was named",
          Startup.startPath(chosen, HOME, ""), "/home/gm/Work")

    // "Last folder" reads the path ui/shell.qml records as the pane moves.
    var last = { startIn: "last", lastPath: "/home/gm/Pictures/2026" }
    check("last folder opens where the pane was left", Startup.startPath(last, HOME, ""), "/home/gm/Pictures/2026")

    // A mode with nothing behind it falls back to home rather than to "", which would be no path at
    // all: a first run with "Last folder" already selected has recorded nothing yet.
    check("last folder with nothing recorded falls back to home",
          Startup.startPath({ startIn: "last" }, HOME, ""), HOME)
    check("a chosen folder that was never chosen falls back to home",
          Startup.startPath({ startIn: "folder", startFolder: "" }, HOME, ""), HOME)
    check("a mode this build does not know falls back to home",
          Startup.startPath({ startIn: "fromANewerFlea" }, HOME, ""), HOME)

    // Where a new tab lands, which is its own setting and not the window's.
    var here = "/home/gm/Downloads"
    check("a new tab can open in home instead", Startup.newTabPath({ newTab: "home" }, here, HOME), HOME)
    check("a new tab can open where the window would",
          Startup.newTabPath({ newTab: "start", startIn: "folder", startFolder: "/srv" }, here, HOME), "/srv")
    check("and the start mode it follows is resolved, not copied",
          Startup.newTabPath({ newTab: "start", startIn: "last", lastPath: "/mnt/vault" }, here, HOME), "/mnt/vault")
    check("a start mode with nothing behind it still lands a new tab on home",
          Startup.newTabPath({ newTab: "start", startIn: "folder" }, here, HOME), HOME)
    check("a tab mode this build does not know clones the current folder",
          Startup.newTabPath({ newTab: "fromANewerFlea" }, here, HOME), here)

    // The command line names the window, never a tab opened later inside it.
    check("a new tab never inherits the path the window was asked for",
          Startup.newTabPath({ newTab: "start" }, here, HOME), HOME)

    // ui.json is a file the operator can edit. src/uistate.rs Rule::Place is the trust boundary and
    // refuses anything but a place or "", so what reaches here is a string or an absent key.
    check("a null recorded path falls back to home",
          Startup.startPath({ startIn: "last", lastPath: null }, HOME, ""), HOME)
}
