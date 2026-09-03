.import "../../ui/js/Remote.js" as Remote

function run(check) {
    var remote = "/run/user/1000/gvfs/sftp:host=box/home/pi"
    check("a GVFS path is remote", Remote.isRemotePath(remote), true)
    check("a local lookalike is local", Remote.isRemotePath("/tmp/gvfs/box"), false)
    check("two mounted endpoints are remote to remote", Remote.transferKind([remote + "/a"], "/run/user/1000/gvfs/smb-share:server=nas,share=data"), "remote-to-remote")
    check("a local source to a mount is named separately", Remote.transferKind(["/tmp/a"], remote), "local-to-remote")
    check("a remote destination line is explicit", Remote.transferPrefix("remote-to-remote", false), "Copying between remote hosts")
    var peer = { group:"network", kind:"peer", uri:"sftp://pi@box/", health:"online", origin:"tailnet", taildrop:true, mac:"" }
    check("a Tailnet peer gets safe remote actions", Remote.menu(peer).map(function (r) { return r.action }).join("|"), "copy-address|open-ssh|taildrop-peer")
    check("the terminal argv keeps the host one argument", Remote.terminalArgv(peer).join("|"), "omarchy-launch-terminal|ssh|pi@box")
    var lan = { group:"network", kind:"discovered", uri:"sftp://lan/", health:"offline", origin:"lan", mac:"aa:bb:cc:dd:ee:ff" }
    check("an offline LAN host offers reconnect and Wake but not SSH", Remote.menu(lan).map(function (r) { return r.action }).join("|"), "reconnect|copy-address|wake")
    check("an offline Taildrop peer never offers a doomed send", Remote.menu({group:"network",kind:"peer",uri:"sftp://box/",health:"offline",origin:"tailnet",taildrop:true}).map(function (r) { return r.action }).join("|"), "reconnect|copy-address")
    check("invalid MACs never offer Wake", Remote.validMac("aa:bb"), false)
    check("a custom SSH port stays in separate argv slots", Remote.terminalArgv({uri:"sftp://pi@host:2222/"}).join("|"), "omarchy-launch-terminal|ssh|-p|2222|pi@host")
    check("a bracketed IPv6 host becomes an SSH target without URI brackets", Remote.terminalArgv({uri:"sftp://pi@[fd7a::1]:2222/"}).join("|"), "omarchy-launch-terminal|ssh|-p|2222|pi@fd7a::1")
    check("a hostile SSH authority is refused", Remote.terminalArgv({uri:"sftp://host;touch/"}).length, 0)
    check("an invalid SSH port is refused", Remote.terminalArgv({uri:"sftp://host:99999/"}).length, 0)
}
