.import "../../ui/js/Crumbs.js" as Crumbs

// The chrome's breadcrumb, issue 45 and Chrome rule 2: the pieces a click can land on, and what
// fits when the strip is narrower than the path. Split out of tests/js/nav.js with the module.

// The two readings a crumb check makes: what the bar draws, and where each piece would take you.
function drawn(list) {
    return list.map(function (c) { return c.text }).join("")
}

function targets(list) {
    return list.map(function (c) { return c.path }).join(" ")
}

function run(check) {
    // Issue 45: the chrome's path as the pieces a click can land on. The pieces have to concatenate
    // to exactly the one line they replace, or the bar draws something nobody asked for, and each
    // has to name the directory ui/ChromeBar.qml would hand to pathEntered.
    var under = Crumbs.crumbs("/home/gm/Work/claude", "/home/gm")
    check("the crumbs read as the tilde path they replace", drawn(under), "~/Work/claude")
    check("and each one names the directory it would open",
          targets(under), "/home/gm /home/gm/Work /home/gm/Work/claude")
    check("and only the last is the directory the pane is already in",
          under.map(function (c) { return c.last }).join(","), "false,false,true")
    var atHome = Crumbs.crumbs("/home/gm", "/home/gm")
    check("home itself is one crumb, the bare tilde",
          drawn(atHome) + "|" + targets(atHome), "~|/home/gm")
    var outside = Crumbs.crumbs("/usr/share", "/home/gm")
    check("a path outside home keeps its leading separator, which is a crumb of its own",
          drawn(outside) + "|" + targets(outside), "/usr/share|/ /usr /usr/share")
    var root = Crumbs.crumbs("/", "/home/gm")
    check("the root is one crumb and it is the last one",
          drawn(root) + "|" + targets(root) + "|" + root.length, "/|/|1")
    var stack = Crumbs.crumbs("flea:stack", "/home/gm")
    check("The Stack is one crumb and it names the place",
          drawn(stack) + "|" + targets(stack) + "|" + stack.length, "The Stack|flea:stack|1")
    var noHome = Crumbs.crumbs("/home/gm/Work", "")
    check("with no home in the environment every component is its own crumb",
          drawn(noHome) + "|" + targets(noHome), "/home/gm/Work|/ /home /home/gm /home/gm/Work")
    // ui/js/Format.js tilde writes /home/gmx as "~x", which is a wrong label on a line nobody can
    // click and a wrong destination on one they can, so the crumbs test the separator themselves.
    var sibling = Crumbs.crumbs("/home/gmx/deep", "/home/gm")
    check("a sibling whose name merely starts with home's is outside it, and says so",
          drawn(sibling) + "|" + targets(sibling), "/home/gmx/deep|/ /home /home/gmx /home/gmx/deep")

    // Chrome rule 2: the strip collapses whole crumbs into one marker rather than cutting one in half.
    var deep = Crumbs.crumbs("/home/gm/one/two/three/four/five", "/home/gm")
    check("a path that fits keeps every crumb", drawn(Crumbs.fitCrumbs(deep, 80)), "~/one/two/three/four/five")
    check("a path too long for the strip reads as its root, a marker and the two nearest you",
          drawn(Crumbs.fitCrumbs(deep, 16)), "~/\u2026/four/five")
    // The marker names no directory, so ui/ChromeBar.qml has something to refuse a press on.
    check("and the marker it inserts carries no destination",
          Crumbs.fitCrumbs(deep, 16)[1].path + String(Crumbs.fitCrumbs(deep, 16)[1].elided), "true")
    check("room left over goes back to the crumbs nearest the root",
          drawn(Crumbs.fitCrumbs(deep, 24)), "~/one/two/\u2026/four/five")
    // Three crumbs have no middle to collapse, so the marker would cost a segment and save nothing.
    check("a path with nothing between its root and the two nearest is left whole",
          drawn(Crumbs.fitCrumbs(Crumbs.crumbs("/home/gm/one/two", "/home/gm"), 4)), "~/one/two")

    // V7: at the floor width the leaf was cut by the strip's own edge, with no marker of any kind.
    var long = Crumbs.crumbs("/home/gm/a-directory-with-a-deliberately-long-name", "/home/gm")
    check("a leaf too long for what is left of the strip takes the ellipsis itself",
          drawn(Crumbs.fitCrumbs(long, 20)), "~/a-directo\u2026ong-name")
    check("and it still names the directory it stands for",
          Crumbs.fitCrumbs(long, 20)[1].path, "/home/gm/a-directory-with-a-deliberately-long-name")
    check("the collapsed form elides its leaf too, once the marker leaves it no room",
          drawn(Crumbs.fitCrumbs(Crumbs.crumbs("/home/gm/one/two/three/a-very-long-leaf-name", "/home/gm"), 18)),
          "~/\u2026/three/a-ve\u2026ame")
    check("a leaf that exactly fills what is left of the strip is drawn whole",
          drawn(Crumbs.fitCrumbs(Crumbs.crumbs("/home/gm/one/two/three/four", "/home/gm"), 14)), "~/\u2026/three/four")
    // A parent that leaves the leaf less than its floor is elided itself, rather than the leaf being
    // cut to nothing: rule 2 elides whole crumbs and the leaf is the one that may not be dropped.
    var wide = Crumbs.crumbs("/home/gm/one/two/a-parent-with-a-very-long-name-indeed/leaf-name", "/home/gm")
    check("a parent that leaves the leaf no room is elided with the middle",
          drawn(Crumbs.fitCrumbs(wide, 16)), "~/\u2026/leaf-name")
    // The floor's own case: a leaf shorter than it gives up more to the ellipsis than that saves.
    check("and a budget too small for even that keeps the leaf whole rather than cutting it to an ellipsis",
          drawn(Crumbs.fitCrumbs(Crumbs.crumbs("/home/gm/one/two/three/four", "/home/gm"), 6)), "~/\u2026/four")
}
