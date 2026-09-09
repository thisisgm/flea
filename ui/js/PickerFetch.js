.pragma library
.import "Format.js" as Format

// What the chooser says while it downloads a URL its location field reported, and nothing about
// the window or the wire: ui/PickerFetch.qml runs the fetch against the backend and reads its
// words from here. The rule is the Windows file dialog's for a URL: the dialog downloads the file
// and the application gets a local path, never the URL. Every function is pure, so
// tests/js/pickerfetch.js drives it with no window.

var FOLDER_ONLY = "Choose a local folder"
var SAVE_LOCAL = "Save needs a local path"
var NOT_FILE = "Not a file URL"
var CANCELLED = "Cancelled"
var FAILED = "Fetch failed"

// Why a remote URL is turned down before any request goes out, or "" when it is fetched. A folder
// request has no use for one file, a save answers a path it will write and a URL is not one, and a
// trailing slash names a listing that gio copy cannot fetch as a file.
function refusal(answer, folderMode, saving) {
    if (folderMode) {
        return FOLDER_ONLY
    }
    if (saving) {
        return SAVE_LOCAL
    }
    if (answer.wantsDir) {
        return NOT_FILE
    }
    return ""
}

// The name the footer fetches by, src/backend/fetchreq.rs's own rule for the file on disk: the
// URL's last segment without query or fragment, percent-decoded, and "download" when there is none.
function leafOf(url) {
    var rest = url.substring(url.indexOf("//") + 2)
    var stop = rest.search(/[?#]/)
    if (stop >= 0) {
        rest = rest.substring(0, stop)
    }
    var parts = rest.split("/").filter(function (part) { return part.length > 0 })
    if (parts.length < 2) {
        return "download"
    }
    var leaf = parts[parts.length - 1]
    try {
        return decodeURIComponent(leaf)
    } catch (e) {
        return leaf
    }
}

// The footer's line: what is being fetched and how far along it is. total is 0 while the server has
// not said, so the line counts bytes alone and the bar beside it runs indeterminate.
function line(leaf, bytes, total) {
    var head = "Fetching " + leaf
    if (bytes <= 0) {
        return head
    }
    if (total > 0) {
        return head + " · " + Format.size(bytes) + " of " + Format.size(total)
    }
    return head + " · " + Format.size(bytes)
}

// The bar's fill, 0 while the total is unknown: an unknown total is not an empty one, and the
// bar says so by moving rather than by standing at zero.
function fraction(bytes, total) {
    if (total <= 0) {
        return 0
    }
    var at = bytes / total
    return at < 0 ? 0 : (at > 1 ? 1 : at)
}

// What a fetch that did not answer a file says in the footer: the backend's own reason, which is
// gio's last line, or one word for the cancel the user asked for.
function failure(err) {
    if (err === "cancelled") {
        return CANCELLED
    }
    return err.length > 0 ? err : FAILED
}
