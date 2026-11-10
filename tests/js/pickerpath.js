.import "../../ui/js/Format.js" as Format
.import "../../ui/js/PathBar.js" as PathBar
.import "sourcefixture.js" as Source

// Picker path entry: the shared ui/PathEntry.qml field, and a typed file answers through its row's own accept, so the checks are the backend's.
function run(check) {
    var entrySrc = Source.source("ui/PathEntry.qml")
    var chromeSrc = Source.source("ui/ChromeBar.qml")
    var barSrc = Source.source("ui/PickerPathBar.qml")
    var logicSrc = Source.source("ui/PickerChrome.qml")
    var crumbsSrc = Source.source("ui/js/Crumbs.js")

    // One shared field: both hosts hold it, neither copies it; the main chrome holds its half in a synchronous Loader.
    var entryBlock = Source.slice(chromeSrc, "id: entryLoader", "onLoaded:")
    check("the main chrome holds the shared entry",
        entryBlock.indexOf('source: "PathEntry.qml"') >= 0 && entryBlock.indexOf("asynchronous: false") >= 0, true)
    check("the picker holds the shared entry", logicSrc.indexOf("Flea.PathEntry") >= 0, true)
    check("the entry carries the frame", entrySrc.indexOf("id: editFrame") >= 0, true)
    check("the entry carries the field", entrySrc.indexOf("id: field") >= 0, true)
    check("the entry carries the Jump dropdown", entrySrc.indexOf('source: "PathJump.qml"') >= 0, true)
    check("the entry carries Tab completion", entrySrc.indexOf("function completeEdit()") >= 0, true)
    // The chrome keeps its commit: the same resolve, the same retry gate, the same sentence.
    check("the chrome keeps its resolve",
        chromeSrc.indexOf("PathBar.resolve(typed, root.path, root.home)") >= 0, true)
    check("the chrome keeps its retry gate",
        chromeSrc.indexOf("PathBar.shouldNavigate(target, root.path, root.pathFailed)") >= 0, true)
    check("the chrome says the shared refusal sentence", chromeSrc.indexOf("root.said(PathBar.REFUSED_SENTENCE)") >= 0, true)
    check("the picker says the shared refusal sentence", barSrc.indexOf("root.picker.say(PathBar.REFUSED_SENTENCE, false)") >= 0, true)
    // The chrome draws through the entry and nothing of its own; its first open builds it, and calls before that do nothing.
    var openBody = Source.slice(chromeSrc, "function startEdit()", "function closeEdit()")
    var built = openBody.indexOf("root.entryBuilt = true")
    check("the chrome delegates its open", built >= 0 && openBody.indexOf("entryLoader.item.startEdit()") > built, true)
    check("the chrome delegates its peek reply", chromeSrc.indexOf("entryLoader.item.completeWith(dir, hidden, rows)") >= 0, true)
    check("the chrome stays shut until built",
        chromeSrc.indexOf("if (entryLoader.item !== null) entryLoader.item.closeEdit()") >= 0, true)
    check("the chrome draws no field of its own", chromeSrc.indexOf("id: editFrame") < 0, true)
    check("the chrome loads no Jump of its own", chromeSrc.indexOf('"PathJump.qml"') < 0, true)
    check("the picker draws no field of its own", barSrc.indexOf("TextInput") < 0, true)
    check("the picker loads no Jump of its own", barSrc.indexOf('"PathJump.qml"') < 0, true)
    check("the picker holds no ranking of its own",
        barSrc.indexOf("Jump.rows") < 0 && barSrc.indexOf("Jump.prepare") < 0 && barSrc.indexOf("rowsPrepared") < 0, true)
    // The resolver is called directly; no wrapper stands between the bar and it.
    check("the picker resolves through PathBar", barSrc.indexOf("PathBar.resolve(text, root.basePath, root.picker.home)") >= 0, true)
    check("no resolver wrapper remains", barSrc.indexOf("PickerPath.") < 0, true)
    // Peek first, so a refused line lists nothing: the backend's peek route says missing, folder or file.
    check("a committed line is peeked before anything lists",
        barSrc.indexOf("root.listing.peek(target, 0, root.picker.showHidden)") >= 0, true)
    check("the peek reply of a line is told from Tab's by its zero count", barSrc.indexOf("first === 0") >= 0, true)
    check("a folder lists through the picker's own open", barSrc.indexOf("root.picker.open(path)") >= 0, true)
    check("a file's parent lists through the picker's own open", barSrc.indexOf("root.picker.open(root.leafParent)") >= 0, true)
    check("no listing is asked for before the peek answers", barSrc.indexOf("openWithoutHistory") < 0, true)
    check("a refused line restores nothing", barSrc.indexOf("restore") < 0, true)
    // A file's parent lists, the listing names the leaf's row, the cursor lands on it and the row's own accept runs.
    check("a typed file is located by name in the listing", barSrc.indexOf("root.listing.locate(") >= 0, true)
    check("no window is walked to find a typed file", barSrc.indexOf("root.listing.window(") < 0, true)
    check("the cursor lands on the located row", barSrc.indexOf("root.picker.cursorIndex = message.index") >= 0, true)
    check("the located row's window is the view's own", barSrc.indexOf("root.picker.viewItem().reshow(message.index)") >= 0, true)
    check("the row's own accept answers", barSrc.indexOf("root.picker.activate(index)") >= 0, true)
    check("the only mark names the folder type", barSrc.indexOf("directory: true") >= 0
        && barSrc.indexOf("directory: false") < 0, true)
    check("the probe's reply reaches the selection's own handler",
        Source.source("ui/PickerWindow.qml").indexOf("message.id === win.markRequest || message.id === win.pathProbe") >= 0, true)
    // Whole sentences with a placeholder, filled through one helper that keeps a dollar in a name literal.
    check("the missing sentence is whole", Format.fill(PathBar.MISSING_SENTENCE, "/x/y z"), "/x/y z does not exist or cannot be reached.")
    check("the filter sentence is whole", Format.fill(PathBar.FILTERED_SENTENCE, "a.png"),
        "a.png is hidden by the file filter or the hidden files setting.")
    check("a dollar in a name stays literal", Format.fill(PathBar.MISSING_SENTENCE, "/a$&b"), "/a$&b does not exist or cannot be reached.")
    check("each sentence has one placeholder",
        PathBar.MISSING_SENTENCE.split("%1").length === 2 && PathBar.FILTERED_SENTENCE.split("%1").length === 2, true)
    check("the picker says both through the helper",
        barSrc.indexOf("Format.fill(PathBar.MISSING_SENTENCE") >= 0 && barSrc.indexOf("Format.fill(PathBar.FILTERED_SENTENCE") >= 0, true)
    // What a peek says a line names: a readable folder, a folder that refused the read, a file, or nothing.
    check("a readable folder", PathBar.kindOfPeek(false, 0), "folder")
    check("a folder that refused the read", PathBar.kindOfPeek(true, 16576), "folder")
    check("a file fails its scan and carries its mode", PathBar.kindOfPeek(true, 33188), "file")
    check("a failed stat names nothing", PathBar.kindOfPeek(true, 0), "missing")
    // The board's strip: the chrome's own 27 px at base size 14, so the field and its dropdown sit as in ChromeBar.
    var strip = logicSrc.substring(logicSrc.indexOf("id: where"), logicSrc.indexOf("id: where") + 600)
    check("the path strip takes the chrome's height", /^\s*height: Theme\.chromeHeight\s*$/m.test(strip), true)
    var slot = logicSrc.substring(logicSrc.indexOf("Flea.PathEntry {"), logicSrc.indexOf("Flea.PickerPathBar {"))
    check("the entry hangs from the strip's top and leaves its rule", slot.indexOf("anchors.top: parent.top") >= 0
        && slot.indexOf("anchors.bottomMargin: Theme.spacing.hairline") >= 0, true)
    check("the entry carries no height of its own", /^\s*height:/m.test(slot), false)
    // The keys reach the strip from both views, and keys.toml is not edited.
    check("the list routes the pathBar action",
        Source.source("ui/PickerList.qml").indexOf('"pathBar"') >= 0, true)
    check("the grid routes the pathBar action",
        Source.source("ui/PickerGrid.qml").indexOf('"pathBar"') >= 0, true)
    check("the picker strip builds individual directory crumbs", logicSrc.indexOf('import "js/Crumbs.js" as Crumbs') >= 0
        && logicSrc.indexOf("Crumbs.crumbs(root.picker.path, root.picker.home)") >= 0, true)
    check("the picker uses the shared crumb input and visual conventions", logicSrc.indexOf("delegate: Flea.Crumb") >= 0
        && logicSrc.indexOf("onChosen: function (path) { root.picker.open(path) }") >= 0, true)
    check("crumb navigation calls the existing history-preserving picker open", Source.source("ui/PickerWindow.qml").indexOf("function open(next)") >= 0, true)
    check("the collapsed marker has no destination", crumbsSrc.indexOf('path: "", last: false, elided: true') >= 0, true)
    check("Recent remains a single non-path label", logicSrc.indexOf("path: Picker.RECENT, last: true") >= 0, true)
}
