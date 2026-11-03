.import "../../ui/js/PinDrop.js" as PinDrop

// Downstream (driveway-fm): the FAVORITES heading's pin drop, as pure functions. The DropArea
// itself (ui/PinHeadingDrop.qml) is covered by the pointer suites; what must never drift is the
// decision set: one local folder, not already pinned, labelled by its own name.

function normalisation(check) {
    check("a trailing slash names the same folder", PinDrop.normalised("/home/u/Projects/", ""), "/home/u/Projects")
    check("many trailing slashes name the same folder", PinDrop.normalised("/home/u/Projects///", ""), "/home/u/Projects")
    check("the root's slashes collapse to the root", PinDrop.normalised("///", ""), "/")
    check("a tilde names home", PinDrop.normalised("~", "/home/u"), "/home/u")
    check("a tilde prefix names inside home", PinDrop.normalised("~/Work", "/home/u"), "/home/u/Work")
    check("a tilde without home stays a tilde", PinDrop.normalised("~", ""), "~")
    check("an empty path names nothing", PinDrop.normalised("", "/home/u"), "")
    check("a name that starts like a tilde is not one", PinDrop.normalised("~other", "/home/u"), "~other")
}

function duplicates(check) {
    var records = [ { label: "Projects", path: "/home/u/Projects" }, { label: "Work", path: "/home/u/Work/" } ]
    check("the same path is already pinned", PinDrop.alreadyPinned(records, "/home/u/Projects", "/home/u"), true)
    check("a trailing slash on the saved side still matches", PinDrop.alreadyPinned(records, "/home/u/Work", "/home/u"), true)
    check("a trailing slash on the asked side still matches", PinDrop.alreadyPinned(records, "/home/u/Projects/", "/home/u"), true)
    check("a tilde form still matches", PinDrop.alreadyPinned(records, "~/Projects", "/home/u"), true)
    check("a sibling folder is not pinned", PinDrop.alreadyPinned(records, "/home/u/Archive", "/home/u"), false)
    check("no favourites pin anything", PinDrop.alreadyPinned([], "/home/u/Projects", "/home/u"), false)
    check("a null list pins nothing", PinDrop.alreadyPinned(null, "/home/u/Projects", "/home/u"), false)
}

function candidates(check) {
    var one = ["file:///home/u/Work"]
    var two = ["file:///home/u/Work", "file:///home/u/Play"]
    check("one local folder is a candidate", PinDrop.candidate("", one, "", ""), "/home/u/Work")
    check("two paths name no favourite", PinDrop.candidate("", two, "", ""), "")
    check("no paths name nothing", PinDrop.candidate("", [], "", ""), "")
    check("a shelf token is a transfer, not a pin", PinDrop.candidate("", one, "", "tok copy"), "")
    check("a plain-text path is a candidate", PinDrop.candidate("", [], "/home/u/Work", ""), "/home/u/Work")
}

function arming(check) {
    check("a directory arms the drop", PinDrop.armed("dir", "/home/u/Work"), true)
    check("a file does not arm the drop", PinDrop.armed("file", "/home/u/notes.txt"), false)
    check("a missing path does not arm the drop", PinDrop.armed("", "/home/u/gone"), false)
    check("no path never arms", PinDrop.armed("dir", ""), false)
    check("the kind line answers by path", PinDrop.kindAnswer({ "/home/u/Work": "dir" }, "/home/u/Work"), "dir")
    check("an unanswered path reads empty", PinDrop.kindAnswer({ "/home/u/Work": "dir" }, "/home/u/Other"), "")
    check("no kind line reads empty", PinDrop.kindAnswer(null, "/home/u/Work"), "")
}

function labels(check) {
    check("the label is the folder's own name", PinDrop.label("/home/u/Projects"), "Projects")
    check("the root labels itself", PinDrop.label("/"), "/")
    check("a trailing slash changes no label", PinDrop.label("/home/u/Projects/"), "Projects")
}

function run(check) {
    normalisation(check)
    duplicates(check)
    candidates(check)
    arming(check)
    labels(check)
}
