.pragma library

// Paths a foreign drop may copy, and the one sentence a refused drop says.
// A drag this window started keeps its own marker and never re-reads text/plain:
// a newline in a file name would otherwise become a second source.

function usePlain(marker, shelf) {
    return !marker && !shelf
}

function filePath(url) {
    var text = String(url)
    if (text.indexOf("file://") !== 0) return null
    var rest = text.substring(7)
    var path
    if (rest.charAt(0) === "/") {
        path = rest
    } else {
        var slash = rest.indexOf("/")
        if (slash < 0 || rest.substring(0, slash) !== "localhost") return null
        path = rest.substring(slash)
    }
    try {
        return decodeURIComponent(path)
    } catch (error) {
        return null
    }
}

function filePaths(urls) {
    var paths = []
    if (!urls) return paths
    for (var i = 0; i < urls.length; i++) {
        var path = filePath(urls[i])
        if (path) paths.push(path)
    }
    return paths
}

function plainPaths(plain) {
    var paths = []
    if (!plain) return paths
    var lines = String(plain).split("\n")
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i]
        if (line.charAt(line.length - 1) === "\r") line = line.substring(0, line.length - 1)
        if (line.charAt(0) === "/") paths.push(line)
    }
    return paths
}

function sources(urls, plain, marker, shelf) {
    var paths = filePaths(urls)
    if (!usePlain(marker, shelf)) return paths
    var extra = plainPaths(plain)
    for (var i = 0; i < extra.length; i++) {
        if (paths.indexOf(extra[i]) < 0) paths.push(extra[i])
    }
    return paths
}

// Checked in this order: a folder inside the drag, a file already in the destination, then nothing local.
function refusal(paths, dest, marker, shelf) {
    var i
    for (i = 0; i < paths.length; i++) {
        if (dest === paths[i] || dest.indexOf(paths[i] + "/") === 0)
            return "That folder is inside the drag."
    }
    for (i = 0; i < paths.length; i++) {
        var slash = paths[i].lastIndexOf("/")
        var parent = slash <= 0 ? "/" : paths[i].substring(0, slash)
        if (parent === dest) return "Already in this folder."
    }
    if (paths.length === 0 && usePlain(marker, shelf)) return "That drag has no local files."
    return ""
}

function askAllowed(pathsPending, clipPending) {
    return !pathsPending && clipPending === null
}

// The floor refuses while the listing is out. A hovered tab keeps the folder it already stored.
// The tab whose own listing has not landed refuses with the floor.
function refuseLoading(listInFlight, hoverTab, currentTab) {
    if (listInFlight !== true) return false
    if (hoverTab !== true) return true
    return currentTab === true
}

function searchTabEnabled(searchMode, current) {
    return !(searchMode && current)
}
