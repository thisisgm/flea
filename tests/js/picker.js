.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/Filter.js" as Filter

function run(check) {
    // The shape tools/flea-portal writes for an OpenFile with two filters, taken from its request_for().
    var asked = JSON.stringify({
        mode: "open", title: "Send to unraid", app: "", accept: "Send", multiple: true,
        directory: false, folder: "/home/gm/Pictures", file: "", name: "", files: [],
        filters: [{ label: "Images", globs: ["*.png", "*.jpg"], mimes: [] },
                  { label: "Any text", globs: [], mimes: ["text/plain"] }],
        current: "Images"
    })
    var req = Picker.request(asked)
    check("the mode is read", req.mode, "open")
    check("the caller's title is read", req.title, "Send to unraid")
    check("multiple is read", req.multiple, true)
    check("the starting folder is read", req.folder, "/home/gm/Pictures")

    var empty = Picker.request("not json")
    check("a request that is not JSON still opens a window", empty.mode, "open")
    check("a request with nothing in it has no filters", empty.filters.length, 0)
    check("a request with nothing in it is not multiple", empty.multiple, false)
    check("an absent mode is never save", Picker.request('{"mode":"nonsense"}').mode, "open")

    check("the caller's title wins", Picker.title(req), "Send to unraid")
    check("a titleless open says what the window is for", Picker.title(Picker.request("{}")), "Choose file")
    check("a titleless directory request says folder", Picker.title(Picker.request('{"directory":true}')), "Choose folder")
    check("a titleless save says save", Picker.title(Picker.request('{"mode":"save"}')), "Save file")

    // A host application has no portal identity, and nothing else may be shown as one.
    check("no app id draws no requester line", Picker.subtitle(req), "")
    check("an app id is named", Picker.subtitle(Picker.request('{"app":"org.gnome.gedit"}')), "Requested by org.gnome.gedit")

    check("the accept label carries the count", Picker.acceptLabel(req, 3), "Send 3")
    check("one file does not carry a count", Picker.acceptLabel(req, 1), "Send")
    check("no accept label falls back to Open", Picker.acceptLabel(Picker.request("{}"), 0), "Open")
    check("a folder request falls back to Choose folder", Picker.acceptLabel(Picker.request('{"directory":true}'), 0), "Choose folder")

    // Recent is a location and not a directory: no path this window ever holds can equal its token,
    // and a row inside it is identified by its own path under the listing base "/".
    check("Recent is not a path", Picker.RECENT.charAt(0) === "/", false)
    check("the location is recognised", Picker.isRecent(Picker.RECENT), true)
    check("a real directory is not Recent", Picker.isRecent("/home/gm"), false)
    check("a row in a directory joins onto it", Picker.rowPath("/home/gm", "a.png"), "/home/gm/a.png")
    check("a row in Recent is its own path", Picker.rowPath(Picker.RECENT, "home/gm/Pictures/a.png"), "/home/gm/Pictures/a.png")
    check("a row at the root still joins once", Picker.rowPath("/", "etc"), "/etc")

    check("nothing checked says so", Picker.statusLine(0, 0), "0 selected")
    check("what is checked and what it weighs", Picker.statusLine(3, 2100000), "3 selected · 2.1 MB")
    check("the open hints name Space and Enter", Picker.hints(req), "Space select · Enter open/send · : location · Esc cancel")
    check("a folder request names the location key too", Picker.hints(Picker.request('{"mode":"open","directory":true}')),
          "Enter open · Space mark folder · : location · Esc cancel")
    check("the save hints name neither", Picker.hints(Picker.request('{"mode":"save"}')), "Enter save · Esc cancel")

    var chips = Picker.chips(req)
    check("every filter gets a chip, and All files after them", chips.length, 3)
    check("the first chip is the caller's first filter", chips[0].label, "Images")
    check("All files is last and is not a filter", chips[2].label + " " + chips[2].index, "All files -1")
    check("no filters means no chip row at all", Picker.chips(Picker.request("{}")).length, 0)
    check("current_filter chooses the active chip", Picker.currentChip(req), 0)
    check("an unknown current_filter falls back to the first", Picker.currentChip(Picker.request('{"filters":[{"label":"A"}],"current":"Z"}')), 0)
    check("no filters has no active chip", Picker.currentChip(Picker.request("{}")), -1)

    // The user's own pills from filters.toml, which only a caller that sent no filters ever sees.
    var config = [{ name: "", globs: ["*.jpg", "*.jpeg"], mimes: [] }, { name: "Documents", globs: ["*.odt"], mimes: [] }]
    var own = Picker.chips(Picker.request("{}"), config)
    check("no app filters and a config draws All files first", own[0].label + " " + own[0].index, "All files -1")
    check("then one pill per config filter, labelled by extension", own[1].label + "|" + own[2].label, ".jpg (.jpg, .jpeg)|Documents")
    check("config pills index the config list", own[1].index + "," + own[2].index, "0,1")
    check("All files starts active over config pills", Picker.currentChip(Picker.request("{}")), -1)
    check("the config list is what those indices read", Picker.filterList(Picker.request("{}"), config)[1].name, "Documents")
    var withApp = Picker.chips(req, config)
    check("app filters present: the config is ignored", withApp.length, 3)
    check("and today's chip list is unchanged", withApp[0].label + "|" + withApp[2].label + " " + withApp[2].index, "Images|All files -1")
    check("the app's own list is what its indices read", Picker.filterList(req, config), req.filters)
    check("neither app nor config filters is no chip row", Picker.chips(Picker.request("{}"), []).length, 0)
    check("no config at all reads as none", Picker.chips(Picker.request("{}"), undefined).length, 0)

    // A filter takes the whole row: globs read its name and mime rules its icon name.
    function file(name, icon) { return { n: name, d: false, i: icon || "text-x-generic" } }
    var images = req.filters[0]
    check("a glob matches", Picker.matchesFilter(file("shot.png", "image-x-generic"), images), true)
    check("a glob matches whatever the case is", Picker.matchesFilter(file("SHOT.PNG", "image-x-generic"), images), true)
    check("a name outside the globs does not match", Picker.matchesFilter(file("notes.md"), images), false)
    check("a two part suffix matches", Picker.matchesFilter(file("x.tar.gz"), { globs: ["*.tar.gz"] }), true)
    // The glob is the caller's, so its punctuation is escaped rather than compiled into a class.
    check("a bracket in a glob is literal", Picker.matchesFilter(file("a[b].png"), { globs: ["a[b].png"] }), true)
    check("a bracket glob does not become a character class", Picker.matchesFilter(file("ab.png"), { globs: ["a[b].png"] }), false)
    check("a dot is not any character", Picker.matchesFilter(file("axpng"), { globs: ["*.png"] }), false)
    check("a question mark is one character", Picker.matchesFilter(file("ab.png"), { globs: ["a?.png"] }), true)
    // Mime rules narrow by the class the icon name confirms; an exact subtype no row can confirm
    // is unmatched, so the caller's text/plain rule lets no file through.
    check("a class rule matches an image row", Picker.matchesFilter(file("shot.png", "image-x-generic"), { globs: [], mimes: ["image/*"] }), true)
    check("a class rule does not match a text row", Picker.matchesFilter(file("notes.md"), { globs: [], mimes: ["image/*"] }), false)
    check("an exact subtype rule does not falsely match", Picker.matchesFilter(file("notes.txt"), req.filters[1]), false)
    check("globs plus mimes narrow to the intersection", Picker.matchesFilter(file("shot.png"), { globs: ["*.png"], mimes: ["image/*"] }), false)
    check("a directory stands under every filter", Picker.matchesFilter({ n: "sub", d: true, i: "folder" }, req.filters[1]), true)

    var rows = [{ n: "sub", d: true, s: 0, i: "folder" }, { n: "a.png", d: false, s: 10, i: "image-x-generic" },
                { n: "b.md", d: false, s: 20, i: "text-x-generic" }, { n: "c.png", d: false, s: 30, i: "text-x-generic" }]
    check("no chip narrows nothing at all", Picker.shownRows(rows, 0, null), null)
    var shown = Picker.shownRows(rows, 4, images)
    check("the held offset is what the listing rows are numbered from", shown.join(","), "4,5,7")
    check("a directory always stands, so the way out is never hidden", shown[0], 4)
    check("a mime-only chip keeps the directory and the rows of its class", Picker.shownRows(rows, 0, { globs: [], mimes: ["image/*"] }).join(","), "0,1")
    check("both legs keep only the rows both confirm", Picker.shownRows(rows, 0, { globs: ["*.png"], mimes: ["image/*"] }).join(","), "0,1")

    // The chip's rows and the typed query's rows meet in narrow: a subsequence of the chip list,
    // strictly ascending, so the backend's order and its directories-first grouping hold.
    var byChip = Picker.shownRows(rows, 4, images)
    var byQuery = Filter.shown(rows, 4, "c")
    check("the query alone would keep the rows named with a c", byQuery.join(","), "7")
    var both = Picker.narrow(byChip, byQuery)
    check("what both keep is what is shown", both.join(","), "7")
    check("an empty query shows the chip list unchanged", Picker.narrow(byChip, Filter.shown(rows, 4, "")), byChip)
    check("no chip shows the query's list unchanged", Picker.narrow(null, byQuery), byQuery)
    check("neither narrows nothing", Picker.narrow(null, null), null)
    var pics = [{ n: "pics", d: true, s: 0, i: "folder" }, { n: "a.png", d: false, s: 10, i: "image-x-generic" },
                { n: "pic.md", d: false, s: 20, i: "text-x-generic" }, { n: "pic.png", d: false, s: 30, i: "image-x-generic" }]
    var picChip = Picker.shownRows(pics, 4, images)
    var wide = Picker.narrow(picChip, Filter.shown(pics, 4, "pic"))
    check("the meeting is a subsequence of the chip list", wide.join(",") + "|" + picChip.join(","), "4,7|4,5,7")
    check("and strictly ascending", wide.every(function (r, i) { return i === 0 || r > wide[i - 1] }), true)
    check("so the directory still comes first", pics[wide[0] - 4].d, true)
    check("a query the chip's rows never match shows nothing, and that is still a filter",
          Picker.narrow(byChip, Filter.shown(rows, 4, "b.md")).length, 0)

    // The save name is a client string, and the answer it builds must stay inside the folder the
    // user was shown. Same cases as src/backend/ops.rs's own valid_name test, so a drift shows here.
    check("an ordinary name is a name", Picker.validName("ordinary.txt"), true)
    check("a dotfile is a name", Picker.validName(".bashrc"), true)
    check("spaces are a name", Picker.validName("a name with spaces"), true)
    check("an empty name would answer with the directory", Picker.validName(""), false)
    check("a single dot is the directory itself", Picker.validName("."), false)
    check("two dots climb out of the directory", Picker.validName(".."), false)
    check("a separator moves the answer out of the folder", Picker.validName("../escape"), false)
    check("a leading separator is an absolute path", Picker.validName("/etc/passwd"), false)
    check("a subdirectory is still a separator", Picker.validName("sub/child"), false)
    check("the reviewer's own exploit is refused", Picker.validName("../../.config/autostart/pwn.desktop"), false)
    check("an interior NUL truncates the path at the syscall", Picker.validName("nul\0byte"), false)
    // tools/flea-portal reads current_name verbatim, so a traversal reaches Picker.request() intact
    // and the window has to be the thing that refuses it.
    check("a traversal survives the request unchanged", Picker.request('{"name":"../../pwn"}').name, "../../pwn")
    check("and the request's own name is then refused", Picker.validName(Picker.request('{"name":"../../pwn"}').name), false)

    check("a path joins under its directory", Picker.join("/home/gm", "a.txt"), "/home/gm/a.txt")
    check("the root does not double its slash", Picker.join("/", "etc"), "/etc")
    check("the parent of a path is its directory", Picker.parentOf("/home/gm/a.txt"), "/home/gm")
    check("the root is its own parent, so Parent stops there", Picker.parentOf("/"), "/")

    check("a picked path leaves as a file URI", Picker.uris(["/home/gm/a.txt"])[0], "file:///home/gm/a.txt")
    check("a space is encoded, because GLib refuses a bad one", Picker.uris(["/home/gm/my file.txt"])[0], "file:///home/gm/my%20file.txt")
    check("a hash is encoded rather than read as a fragment", Picker.uris(["/home/gm/a#b.txt"])[0], "file:///home/gm/a%23b.txt")

    check("a pick answers with its URIs", Picker.reply(0, ["/home/gm/a.txt"]),
          '{"response":0,"uris":["file:///home/gm/a.txt"]}')
    check("a refusal answers with no URI at all", Picker.reply(1, ["/home/gm/a.txt"]), '{"response":1}')

    // The caller learns which of its filters held; it learns nothing from All files or a refusal.
    var images = { label: "Images", globs: ["*.png"], mimes: ["image/jpeg"] }
    check("a pick under a caller's filter echoes it", Picker.reply(0, ["/home/gm/a.png"], images),
          '{"response":0,"uris":["file:///home/gm/a.png"],"current_filter":{"label":"Images","globs":["*.png"],"mimes":["image/jpeg"]}}')
    check("a refusal under a filter still says nothing", Picker.reply(1, ["/home/gm/a.png"], images), '{"response":1}')
    check("All files echoes no filter", Picker.reply(0, ["/home/gm/a.png"], null),
          '{"response":0,"uris":["file:///home/gm/a.png"]}')
    check("a config pill is echoed under the label it showed",
          JSON.parse(Picker.reply(0, ["/home/gm/a.jpg"], { name: "", globs: ["*.jpg", "*.jpeg"], mimes: [] })).current_filter.label,
          ".jpg (.jpg, .jpeg)")
    check("a filter with no rule is dropped",
          Picker.reply(0, ["/home/gm/a.png"], { label: "Nothing", globs: [], mimes: [] }),
          '{"response":0,"uris":["file:///home/gm/a.png"]}')
}
