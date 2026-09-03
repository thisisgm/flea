.import "../../ui/js/Protocols.js" as Protocols

// Every expectation here is read off the canvas's Network board, which draws five dialogs with the
// exact URI each one would hand to gio mount.
function run(check) {
    check("the five protocols are the canvas's own, in its order",
          Protocols.PROTOCOLS.join("|"), "SMB|SFTP|FTPS|WebDAV|NFS")
    check("and each prefills the port the canvas names",
          [445, 22, 21, 443, 2049].map(function (p, i) {
              return Protocols.defaultPort(Protocols.PROTOCOLS[i]) === p
          }).join(","),
          "true,true,true,true,true")

    // The TLS box flips two schemes and touches no others.
    check("the TLS box flips dav and ftp, and nothing else",
          Protocols.scheme("WebDAV", true) + "|" + Protocols.scheme("WebDAV", false)
          + "|" + Protocols.scheme("FTPS", true) + "|" + Protocols.scheme("FTPS", false)
          + "|" + Protocols.scheme("SFTP", true) + "|" + Protocols.scheme("SMB", false),
          "davs|dav|ftps|ftp|sftp|smb")

    // The canvas's own SFTP dialog, field for field.
    check("the SFTP dialog builds the URI the canvas draws",
          Protocols.uri({ protocol: "SFTP", host: "username.servername.example.com", port: "22",
                          path: "/downloads", user: "username", tls: false }),
          "sftp://username@username.servername.example.com:22/downloads")
    // Its own WebDAV dialog.
    check("and the WebDAV one, TLS on",
          Protocols.uri({ protocol: "WebDAV", host: "username.servername.example.com", port: "443",
                          path: "/webdav", user: "username", tls: true }),
          "davs://username@username.servername.example.com:443/webdav")
    // Its own FTPS dialog, which has no path at all.
    check("and the FTPS one, which names no path",
          Protocols.uri({ protocol: "FTPS", host: "username.servername.example.com", port: "21",
                          path: "", user: "username", tls: true }),
          "ftps://username@username.servername.example.com:21/")

    check("a domain is the smb form the design names",
          Protocols.uri({ protocol: "SMB", host: "nas", port: "445", path: "isos",
                          user: "gm", domain: "WORKGROUP", tls: false }),
          "smb://WORKGROUP;gm@nas:445/isos")
    check("and without one it is just the user",
          Protocols.uri({ protocol: "SMB", host: "nas", port: "445", path: "isos", user: "gm", tls: false }),
          "smb://gm@nas:445/isos")
    // NFS is authorised by the server, so the form asks for no credentials and the URI carries none.
    check("NFS carries no credentials even when a user is somehow set",
          Protocols.uri({ protocol: "NFS", host: "nas", port: "2049", path: "/export", user: "gm", tls: false }),
          "nfs://nas:2049/export")
    check("and its field set says so",
          Protocols.fieldsFor("NFS").credentials + "|" + Protocols.fieldsFor("SMB").credentials,
          "false|true")

    // The row labels differ per protocol because the thing they name differs.
    check("each protocol names its own path row",
          ["SMB", "SFTP", "NFS"].map(function (p) { return Protocols.fieldsFor(p).pathLabel }).join("|"),
          "Share|Path|Export")
    check("and only SMB asks for a domain",
          ["SMB", "SFTP", "WebDAV"].map(function (p) { return Protocols.fieldsFor(p).domain }).join("|"),
          "true|false|false")
    check("while only the two schemes that have a plaintext twin carry a TLS box",
          ["SMB", "SFTP", "FTPS", "WebDAV", "NFS"].map(function (p) { return Protocols.fieldsFor(p).tls }).join("|"),
          "false|false|true|true|false")

    // A path with no leading separator still produces one, and no path at all is the root.
    check("a path is joined with exactly one separator",
          Protocols.uri({ protocol: "SFTP", host: "h", port: "22", path: "downloads", user: "u" })
          + " / " + Protocols.uri({ protocol: "SFTP", host: "h", port: "22", path: "", user: "u" }),
          "sftp://u@h:22/downloads / sftp://u@h:22/")

    check("a form with no host builds nothing rather than a half-formed address",
          Protocols.uri({ protocol: "SMB", host: "", port: "445", user: "gm" }) + "|"
          + Protocols.complete({ host: "" }) + "|" + Protocols.complete({ host: "nas" }),
          "|false|true")

    check("the label the operator typed wins",
          Protocols.label({ label: "archive", host: "h", path: "/downloads" }), "archive")
    check("and without one it is the last part of the path",
          Protocols.label({ label: "", host: "h", path: "/media/isos" }), "isos")
    check("falling back to the host when there is no path either",
          Protocols.label({ label: "", host: "nas", path: "/" }), "nas")

    function roundtrip(form) {
        var built = Protocols.uri(form)
        var parsed = Protocols.parse(built)
        return Protocols.uri(parsed)
    }
    check("parse inverts the SFTP URI the canvas draws",
          roundtrip({ protocol: "SFTP", host: "username.servername.example.com", port: "22",
                      path: "/downloads", user: "username", tls: false }),
          "sftp://username@username.servername.example.com:22/downloads")
    check("and the SMB domain form",
          roundtrip({ protocol: "SMB", host: "nas", port: "445", path: "isos",
                      user: "gm", domain: "WORKGROUP", tls: false }),
          "smb://WORKGROUP;gm@nas:445/isos")
    check("and NFS, which carries no credentials",
          roundtrip({ protocol: "NFS", host: "nas", port: "2049", path: "/export", user: "gm", tls: false }),
          "nfs://nas:2049/export")
    check("and WebDAV with TLS on",
          roundtrip({ protocol: "WebDAV", host: "h", port: "443", path: "/webdav", user: "u", tls: true }),
          "davs://u@h:443/webdav")
    check("ftp is the TLS-off twin of FTPS",
          Protocols.parse("ftp://u@h:21/").protocol + "|" + Protocols.parse("ftp://u@h:21/").tls,
          "FTPS|false")
    check("a bookmark whose path is a literal tilde keeps it, gio does not expand ~",
          Protocols.parse("sftp://tom@omv.example:22/~").path, "~")
    check("a missing scheme is not a location", Protocols.parse("nas/share") === null, true)
    check("an unknown scheme is refused", Protocols.parse("afp://h/share") === null, true)
    check("a host with a non-numeric colon is refused rather than split",
          Protocols.parse("sftp://u@[::1]/home") === null, true)
    check("an @ in the path is not stolen as a username",
          roundtrip({ protocol: "SFTP", host: "h", port: "22", path: "inbox@2026", user: "u" }),
          "sftp://u@h:22/inbox@2026")
    check("and parse keeps that path and the real user",
          Protocols.parse("sftp://u@h:22/inbox@2026").path + "|"
          + Protocols.parse("sftp://u@h:22/inbox@2026").user, "inbox@2026|u")
}
