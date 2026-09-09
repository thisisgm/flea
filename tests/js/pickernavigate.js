.import "../../ui/js/PickerEntry.js" as PickerEntry
.import "../../ui/js/PickerNavigate.js" as Navigate

// The location field acts on a typed path without showing the user first, so every step it can take
// is asserted here with no window: what is asked of the backend before anything opens, what the
// peek's answer decides, and which row a landed listing puts the cursor on.

var HOME = "/home/gm"
var CWD = "/home/gm/Documents"

function plan(text, marks, folderMode) {
    var answer = PickerEntry.classify(text, CWD, HOME)
    return Navigate.plan(answer, CWD, marks || [], folderMode === true)
}

function verdict(target, wantsDir, folderMode, peeked) {
    return Navigate.verdict(target, wantsDir, folderMode, peeked)
}

var ETC = { total: 3, readFailed: false,
            rows: [{ n: "ssh", d: true }, { n: "hostname", d: false }, { n: "passwd", d: false }] }

function run(check) {
    // Before the backend is asked: nothing typed does nothing, a refusal has a sentence, a URL is
    // handed on, and the two paths that need no peek are the root and the directory on screen.
    check("an empty line is nothing", plan("").step, "nothing")
    check("a NUL is not a local path", plan("/etc/pass\u0000wd").message, Navigate.NOT_LOCAL)
    check("an unknown scheme says so", plan("mailto:gm@example.org").message, Navigate.NO_SCHEME)
    check("a URL with no host says so", plan("http://").message, Navigate.NO_HOST)
    check("a remote is handed on", plan("https://example.org/a.pdf").step, "remote")
    check("with the classified answer", plan("https://example.org/a.pdf").answer.url, "https://example.org/a.pdf")
    check("a share is handed on", plan("smb://nas/media").step, "share")
    check("the current directory settles", plan(".").step, "settle")
    check("and so does its absolute spelling", plan(CWD + "/").step, "settle")
    check("the root is peeked itself", plan("/").parent, "/")
    check("and opens as itself", plan("/").path, "/")
    check("a path is peeked at its parent", plan("/etc/hostname").parent, "/etc")
    check("and carries itself", plan("/etc/hostname").path, "/etc/hostname")
    check("a child of the root is peeked at the root", plan("/etc").parent, "/")
    check("a relative name resolves before the peek", plan("notes.txt").parent, CWD)

    // The second Return: the marked file typed again accepts, a trailing slash on it does not, and
    // in a folder request a mark is a folder and the typed path still walks.
    var marked = [{ path: "/etc/hostname", bytes: 7 }]
    check("a marked file typed again accepts", plan("/etc/hostname", marked).step, "accept")
    check("a marked file with a slash is peeked", plan("/etc/hostname/", marked).step, "peek")
    check("an unmarked file is peeked", plan("/etc/passwd", marked).step, "peek")
    check("a marked folder in folder mode is peeked",
          plan("/etc/hostname", marked, true).step, "peek")

    // After the peek: a directory opens, a file selects in its parent, and each refusal has its line.
    check("a directory opens", verdict("/etc/ssh", false, false, ETC).step, "open")
    check("as itself", verdict("/etc/ssh", false, false, ETC).path, "/etc/ssh")
    check("a directory asked for as one opens too", verdict("/etc/ssh", true, false, ETC).step, "open")
    check("a file selects", verdict("/etc/hostname", false, false, ETC).step, "select")
    check("in its parent", verdict("/etc/hostname", false, false, ETC).parent, "/etc")
    check("by its path", verdict("/etc/hostname", false, false, ETC).path, "/etc/hostname")
    check("a file with a trailing slash is not a folder",
          verdict("/etc/hostname", true, false, ETC).message, Navigate.NOT_FOLDER)
    check("a file in a folder request is not a choice",
          verdict("/etc/hostname", false, true, ETC).message, Navigate.CHOOSE_FOLDER)
    check("a directory in a folder request opens", verdict("/etc/ssh", false, true, ETC).step, "open")
    check("a missing leaf is not found", verdict("/etc/nothing", false, false, ETC).step, "say")
    check("and names the path", verdict("/etc/nothing", false, false, ETC).message, "Not found: /etc/nothing")
    var refused = { total: 0, readFailed: true, rows: [] }
    check("a failed peek is not found", verdict("/root/x", false, false, refused).message, "Not found: /root/x")
    check("a failed peek of the root is not found", verdict("/", true, false, refused).step, "say")
    check("a readable root opens", verdict("/", true, false, { total: 20, readFailed: false, rows: [] }).path, "/")

    // corner: the peek holds only the first rows, so a leaf past them is unknown and not absent.
    var capped = { total: 900, readFailed: false, rows: [{ n: "a", d: false }] }
    check("a leaf past the peeked rows opens the parent", verdict("/big/zz", false, false, capped).step, "openSay")
    check("at the parent", verdict("/big/zz", false, false, capped).parent, "/big")
    check("and says which rows were read", verdict("/big/zz", false, false, capped).message, Navigate.NOT_IN_WINDOW)
    var full = { total: 1, readFailed: false, rows: [{ n: "a", d: false }] }
    check("a leaf missing from a whole listing is absent", verdict("/big/zz", false, false, full).step, "say")

    // The landed listing: the target's row by the identity a mark holds, from the held offset.
    var rows = [{ n: "hostname", d: false }, { n: "passwd", d: false }]
    check("the target row is found", Navigate.indexOf(rows, 0, "/etc", "/etc/passwd"), 1)
    check("from the held offset", Navigate.indexOf(rows, 40, "/etc", "/etc/passwd"), 41)
    check("a row not held is -1", Navigate.indexOf(rows, 0, "/etc", "/etc/shadow"), -1)
    check("a root child joins without a double slash", Navigate.indexOf(rows, 0, "/", "/passwd"), 1)
    var recent = [{ n: "home/gm/a.txt", d: false }]
    check("a Recent row is its whole path", Navigate.indexOf(recent, 0, "flea:recent", "/home/gm/a.txt"), 0)

    // Where the row will sit in the listing, which shows no dotfile: the peek's order minus those.
    var peek = [{ n: ".git", d: true }, { n: "src", d: true }, { n: ".env", d: false },
                { n: "a.txt", d: false }, { n: "b.txt", d: false }]
    check("a select carries the listing index",
          verdict("/p/b.txt", false, false, { total: 5, readFailed: false, rows: peek }).at, 2)
    check("the dotfiles ahead do not count", Navigate.listingIndex(peek, "a.txt"), 1)
    check("the first row is 0", Navigate.listingIndex(peek, ".git"), 0)
    check("a name not peeked is -1", Navigate.listingIndex(peek, "zz"), -1)
    check("a row inside the first window needs no other", Navigate.windowStart(10, 80), 0)
    check("a row past it is asked for a quarter window ahead", Navigate.windowStart(200, 80), 180)
    check("the peek asks for the backend's whole cap", Navigate.PEEK_ROWS, 512)
}
