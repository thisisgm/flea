.import "../../ui/js/Taildrop.js" as Taildrop

function run(check) {
    // The `tailscale status --json` shape, five branches; every name and id here is synthetic.
    var status = JSON.stringify({
        Self: { UserID: 1000000000000001 },
        Peer: {
            "nodekey:offline": { HostName: "mediabox", Online: false, TaildropTarget: 5, UserID: 1000000000000001 },
            "nodekey:owned": { HostName: "laptop", Online: true, TaildropTarget: 1, UserID: 1000000000000001 },
            "nodekey:sameowner": { HostName: "DEVICE-A", Online: true, TaildropTarget: 1, UserID: 1000000000000001 },
            "nodekey:otherowner": { HostName: "OTHER-TENANT", Online: true, TaildropTarget: 9, UserID: 2000000000000002 },
            "nodekey:relay": { HostName: "", DNSName: "foo.mullvad.ts.net.", Online: true, TaildropTarget: 1, UserID: 1000000000000001 }
        }
    })
    var peers = Taildrop.parsePeers(status)
    check("only the reachable, eligible peers survive", peers.length, 2)
    check("sorted by label", peers[0].label + "," + peers[1].label, "DEVICE-A,laptop")
    check("the id is the peer's own map key", peers[0].id, "nodekey:sameowner")

    check("empty input parses to no peers", Taildrop.parsePeers("").length, 0)
    check("garbage input parses to no peers rather than throwing", Taildrop.parsePeers("not json").length, 0)
    check("a body with no Peer object parses to no peers", Taildrop.parsePeers('{"Self":{}}').length, 0)

    // isTaildropTarget's own two branches, read off the real OEM Model.js.
    check("TaildropTarget 1 is always a target", Taildrop.isTaildropTarget({ TaildropTarget: 1 }, "u"), true)
    check("a nonzero TaildropTarget other than 1 is never a target", Taildrop.isTaildropTarget({ TaildropTarget: 9 }, "u"), false)
    check("TaildropTarget 0 falls back to the same-owner check", Taildrop.isTaildropTarget({ TaildropTarget: 0, UserID: "u" }, "u"), true)
    check("a different owner with TaildropTarget 0 is not a target", Taildrop.isTaildropTarget({ TaildropTarget: 0, UserID: "u" }, "other"), false)

    check("DNSName wins as the send address", Taildrop.peerAddress({ DNSName: "host.ts.net." }), "host.ts.net")
    check("HostName is the fallback", Taildrop.peerAddress({ HostName: "host" }), "host")
    check("a tailnet IPv4 is the last resort", Taildrop.peerAddress({ TailscaleIPs: ["100.1.2.3", "fd7a::1"] }), "100.1.2.3")
    check("no usable address at all is empty", Taildrop.peerAddress({}), "")

    check("a localhost HostName falls back to the DNS name", Taildrop.displayHostName("localhost", "example.ts.net."), "example")

    var found = Taildrop.byId(peers, "nodekey:owned")
    check("byId finds the right peer", found.label, "laptop")
    check("byId misses cleanly", Taildrop.byId(peers, "nope"), null)
}
