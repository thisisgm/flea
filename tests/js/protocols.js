.import "../../ui/js/Protocols.js" as Protocols

// The form follows the canvas, while URI output follows the canonical form each GVFS backend accepts.
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
          "sftp://username@username.servername.example.com/downloads")
    // Its own WebDAV dialog.
    check("and the WebDAV one, TLS on",
          Protocols.uri({ protocol: "WebDAV", host: "username.servername.example.com", port: "443",
                          path: "/webdav", user: "username", tls: true }),
          "davs://username@username.servername.example.com/webdav")
    check("TLS-off WebDAV omits port 80 and keeps an explicit 443",
          Protocols.uri({ protocol: "WebDAV", host: "h", port: "80", path: "", tls: false })
          + "|" + Protocols.uri({ protocol: "WebDAV", host: "h", port: "443", path: "", tls: false }),
          "dav://h/|dav://h:443/")
    check("TLS-off WebDAV reparses an omitted port as 80 while the canvas still starts at 443",
          Protocols.defaultPort("WebDAV", false) + "|" + Protocols.defaultPort("WebDAV"),
          "80|443")
    // Its own FTPS dialog, which has no path at all.
    check("and the FTPS one, which names no path",
          Protocols.uri({ protocol: "FTPS", host: "username.servername.example.com", port: "21",
                          path: "", user: "username", tls: true }),
          "ftps://username@username.servername.example.com/")

    check("a domain is the smb form the design names",
          Protocols.uri({ protocol: "SMB", host: "nas", port: "445", path: "isos",
                          user: "gm", domain: "WORKGROUP", tls: false }),
          "smb://WORKGROUP;gm@nas/isos")
    check("and without one it is just the user",
          Protocols.uri({ protocol: "SMB", host: "nas", port: "445", path: "isos", user: "gm", tls: false }),
          "smb://gm@nas/isos")
    // NFS is authorised by the server, so the form asks for no credentials and the URI carries none.
    check("NFS carries no credentials even when a user is somehow set",
          Protocols.uri({ protocol: "NFS", host: "nas", port: "2049", path: "/export", user: "gm", tls: false }),
          "nfs://nas/export")
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
          "sftp://u@h/downloads / sftp://u@h/")

    check("a non-default port remains explicit",
          Protocols.uri({ protocol: "SFTP", host: "h", port: "2222", path: "downloads", user: "u" }),
          "sftp://u@h:2222/downloads")
    check("a password never enters the canonical URI",
          Protocols.uri({ protocol: "SFTP", host: "h", port: "22", path: "downloads",
                          user: "u", password: "not-a-real-secret" }),
          "sftp://u@h/downloads")
    check("valid default and non-default ports both complete the form",
          ["22", "2222"].map(function (port) {
              return Protocols.complete({ host: "h", port: port })
          }).join(","),
          "true,true")

    check("URI user information escapes reserved characters",
          Protocols.uri({ protocol: "SMB", host: "nas", port: "445", path: "share",
                          user: "g m@x", domain: "WORK GROUP", tls: false }),
          "smb://WORK%20GROUP;g%20m%40x@nas/share")
    check("URI paths escape each segment without hiding separators",
          Protocols.uri({ protocol: "SFTP", host: "h", port: "22",
                          path: "/team files/a#b%/caf\u00e9", user: "u" }),
          "sftp://u@h/team%20files/a%23b%25/caf%C3%A9")
    check("a host containing URI delimiters is refused",
          Protocols.uri({ protocol: "SMB", host: "nas/share", port: "445", path: "isos" })
          + "|" + Protocols.complete({ host: "nas/share", port: "445" }),
          "|false")
    check("a non-numeric or out-of-range port is refused",
          ["22/path", "0", "65536"].map(function (port) {
              return Protocols.complete({ host: "nas", port: port })
          }).join(","),
          "false,false,false")

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
}
