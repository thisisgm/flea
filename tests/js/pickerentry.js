.import "../../ui/js/PickerEntry.js" as PickerEntry

// The location field acts on what classify says without showing the user first: a local path is
// opened or picked, a URL is downloaded or mounted. So every shape of line is asserted here, with
// no window and no network, one check per rule of the contract.

var HOME = "/home/gm"
var CWD = "/home/gm/Documents"

function classify(text) {
    return PickerEntry.classify(text, CWD, HOME)
}

function run(check) {
    // Nothing typed, and nothing but whitespace, are the same nothing.
    check("an empty line is empty", classify("").kind, "empty")
    check("whitespace alone is empty", classify("   \t").kind, "empty")
    check("a NUL byte is refused", classify("/etc/pass\u0000wd").kind, "refused")
    check("and says the line is invalid", classify("/etc/pass\u0000wd").reason, "invalid")

    // The local shapes, which are the path bar's own and resolve the same way.
    check("an absolute path is local", classify("/etc/hosts").kind, "local")
    check("and is itself", classify("/etc/hosts").path, "/etc/hosts")
    check("a tilde is the home directory", classify("~").path, HOME)
    check("a tilde and a child", classify("~/Downloads").path, "/home/gm/Downloads")
    check("a bare name is relative to the current directory",
          classify("notes.txt").path, "/home/gm/Documents/notes.txt")
    check("a dot is the current directory", classify(".").path, CWD)
    check("a dotdot climbs", classify("../Music").path, "/home/gm/Music")
    check("interior dot segments are resolved", classify("./a/../b").path, "/home/gm/Documents/b")
    check("surrounding whitespace is trimmed", classify("  /etc  ").path, "/etc")
    check("a path without a trailing slash does not ask for a directory",
          classify("/etc/hosts").wantsDir, false)
    check("a trailing slash asks for a directory", classify("/etc/").wantsDir, true)
    check("and the path itself has no trailing slash", classify("/etc/").path, "/etc")
    check("the root is a directory", classify("/").wantsDir, true)
    check("a local answer carries no url", classify("/etc").url, "")

    // file:// is a local path in a coat. Both local spellings of the authority decode; another
    // host does not, because there is no path on this machine that names it.
    check("a file URI with an empty authority is local", classify("file:///etc/hosts").kind, "local")
    check("and decodes to the path", classify("file:///etc/hosts").path, "/etc/hosts")
    check("a file URI on localhost is local", classify("file://localhost/etc/hosts").path, "/etc/hosts")
    check("a file URI is percent-decoded",
          classify("file:///home/gm/My%20Docs/a%26b.txt").path, "/home/gm/My Docs/a&b.txt")
    check("a file URI with a trailing slash asks for a directory",
          classify("file:///home/gm/").wantsDir, true)
    check("an uppercase FILE scheme still unwraps", classify("FILE:///etc").path, "/etc")
    check("a file URI on another host is refused", classify("file://nas/share/x").kind, "refused")
    check("and says why", classify("file://nas/share/x").reason, "host")

    // The download schemes. The URL is handed on whole, with the scheme in lower case so the
    // fetch leg and the sentence agree on its name.
    check("http is remote", classify("http://example.com/a.pdf").kind, "remote")
    check("and names its scheme", classify("http://example.com/a.pdf").scheme, "http")
    check("and carries the url", classify("http://example.com/a.pdf").url, "http://example.com/a.pdf")
    check("a remote answer carries no path", classify("http://example.com/a.pdf").path, "")
    check("an uppercase scheme is the same scheme", classify("HTTP://example.com/a.pdf").scheme, "http")
    check("and the url is lowered with it",
          classify("HTTP://Example.com/A.pdf").url, "http://Example.com/A.pdf")
    check("https is remote", classify("https://example.com/a.pdf").scheme, "https")
    check("ftp is remote", classify("ftp://ftp.example.com/pub/a.tar").scheme, "ftp")
    check("ftps is remote", classify("ftps://ftp.example.com/pub/a.tar").scheme, "ftps")
    check("a url path ending in a slash asks for a directory",
          classify("http://example.com/pub/").wantsDir, true)
    check("a url file does not", classify("http://example.com/pub/a.tar").wantsDir, false)
    check("a query after the slash does not hide the directory",
          classify("http://example.com/pub/?sort=name").wantsDir, true)
    check("a bare host has no path to end in a slash", classify("http://example.com").wantsDir, false)

    // The mount schemes. ssh is what people type and sftp is what gvfs calls it.
    check("smb is a share", classify("smb://nas/media").kind, "share")
    check("and names its scheme", classify("smb://nas/media").scheme, "smb")
    check("and carries the url", classify("smb://nas/media").url, "smb://nas/media")
    check("sftp is a share", classify("sftp://gm@host/home/gm").scheme, "sftp")
    check("ssh is reported as sftp", classify("ssh://gm@host/home/gm").scheme, "sftp")
    check("and its url is rewritten to match",
          classify("ssh://gm@host/home/gm").url, "sftp://gm@host/home/gm")
    check("a share answer carries no path", classify("smb://nas/media").path, "")

    // Everything else with a scheme is refused rather than fetched blind or opened as a name.
    check("data: is refused", classify("data:text/plain,hi").kind, "refused")
    check("and says the scheme is the problem", classify("data:text/plain,hi").reason, "scheme")
    check("an unknown scheme with slashes is refused", classify("gopher://host/1").reason, "scheme")
    check("a known scheme without slashes is refused", classify("http:example.com").reason, "scheme")
    check("a url with no host is refused", classify("http://").reason, "host")
    check("a url with a path but no host is refused", classify("http:///file").reason, "host")
    check("a share with a path but no host is refused", classify("smb:///share").reason, "host")
    check("a host before the path is remote", classify("http://h/p").kind, "remote")
    check("a host before a query is remote", classify("http://h?x=1").kind, "remote")
    check("a host before a fragment is remote", classify("http://h#f").kind, "remote")
    check("a host before the path is a share", classify("sftp://h/dir").kind, "share")
    check("a file URI with a path and no authority is local", classify("file:///tmp").kind, "local")
    check("a bare file:// is the root", classify("file://").path, "/")
    check("a colon in the first segment reads as a scheme", classify("a:b").kind, "refused")
    check("which ./ escapes", classify("./a:b").path, "/home/gm/Documents/a:b")
}
