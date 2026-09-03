.import "../../ui/js/QuickConnect.js" as QuickConnect

function run(check) {
    check("user host colon path means SFTP", QuickConnect.parse("pi@box:/home/pi").uri, "sftp://pi@box/home/pi")
    check("host colon path means SFTP", QuickConnect.parse("box:/srv/media").uri, "sftp://box/srv/media")
    check("host slash share means SMB", QuickConnect.parse("nas/photos").uri, "smb://nas/photos")
    check("a bare host defaults to SFTP", QuickConnect.parse("server.local").uri, "sftp://server.local/")
    check("an IPv4 defaults to SFTP", QuickConnect.parse("100.64.0.8").uri, "sftp://100.64.0.8/")
    check("a bracketed IPv6 colon path survives", QuickConnect.parse("[fd7a::1]:/srv").uri, "sftp://[fd7a::1]/srv")
    check("an explicit protocol wins", QuickConnect.parse("davs://docs.example/path").protocol, "WebDAV")
    check("an unsupported scheme is refused", QuickConnect.parse("gopher://host/x"), null)
    check("an embedded password is refused", QuickConnect.parse("sftp://pi:secret@host/home"), null)
    check("a query token is refused rather than bookmarked", QuickConnect.parse("davs://host/path?token=secret"), null)
    check("a fragment is refused rather than silently changing identity", QuickConnect.parse("sftp://host/path#private"), null)
    check("shorthand cannot smuggle a query token", QuickConnect.parse("host/path?token=secret"), null)
    check("newline input is refused", QuickConnect.parse("host\nother"), null)
}
