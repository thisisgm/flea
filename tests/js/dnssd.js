.import "../../ui/js/Dnssd.js" as Dnssd

// Downstream (driveway-fm): the hosts layer of the network browser, as pure functions over
// avahi-browse's own answer. The sample body below is a real capture: four servers, one of them
// advertising twice, plus the same LAN's SSH answers, which is the shape the SMB-wins rule and
// the sftp-only row exist for.

var LIVE = [
    "+   wlo1 IPv4 AIBox                                        Microsoft Windows Network local",
    "=   wlo1 IPv4 AIBox                                        Microsoft Windows Network local",
    "   hostname = [AIBox.local]",
    "   address = [192.168.1.41]",
    "--",
    "=   wlo1 IPv4 WaterTower                                   Microsoft Windows Network local",
    "   hostname = [WaterTower.local]",
    "   address = [192.168.1.249]",
    "--",
    "=   wlo1 IPv4 Homeserver                                   Microsoft Windows Network local",
    "   hostname = [Homeserver.local]",
    "   address = [192.168.1.61]",
    "--",
    "=   wlo1 IPv4 Node                                         Microsoft Windows Network local",
    "   hostname = [Node.local]",
    "--",
    "=   wlo1 IPv4 Homeserver                                   Microsoft Windows Network local",
    "   hostname = [Homeserver.local]",
    "--"
].join("\n")

var BOTH_KINDS = [
    "=   wlo1 IPv4 AIBox    SFTP File Transfer   local",
    "   hostname = [AIBox.local]",
    "--",
    "=   wlo1 IPv4 AIBox    Microsoft Windows Network   local",
    "   hostname = [AIBox.local]",
    "--",
    "=   wlo1 IPv4 AITower    SFTP File Transfer   local",
    "   hostname = [AITower.local]",
    "--"
].join("\n")

// The live browse of every service on the wire, trimmed: a printer this does not read sits
// between two hosts it does, and its own hostname line must not be filed under either.
var WIRED = [
    "=   wlo1 IPv4 Inkjet        _ipp._tcp   local",
    "   hostname = [Inkjet.local]",
    "   address = [192.168.1.77]",
    "--",
    "=   wlo1 IPv4 AIBox    Microsoft Windows Network   local",
    "   hostname = [AIBox.local]",
    "--",
    "+   wlo1 IPv4 Mop          Printer   local",
    "   hostname = [Mop.local]",
    "--",
    "=   wlo1 IPv4 NASbox   SFTP File Transfer   local",
    "   hostname = [NASbox.local]",
    "--"
].join("\n")

function parse(check) {
    var hosts = Dnssd.parse(LIVE)
    check("one row per hostname, duplicates dropped", hosts.length, 4)
    check("rows sort by their own labels", [hosts[0].label, hosts[1].label, hosts[2].label, hosts[3].label].join("|"),
        "AIBox|Homeserver|Node|WaterTower")
    check("the row carries its service's name and its host's address", hosts[3].host, "WaterTower.local")
    check("a resolved service with no hostname line names no row", Dnssd.parse("=   wlo1 IPv4 Ghost   Microsoft Windows Network local\n--").length, 0)
    check("an empty body reads as no hosts", Dnssd.parse("").length, 0)
    check("no body at all reads as no hosts", Dnssd.parse(null).length, 0)
    check("garbage reads as no hosts", Dnssd.parse("hostname = [x.local]\nrandom text").length, 0)
}

function kinds(check) {
    var hosts = Dnssd.parse(BOTH_KINDS)
    check("a host advertising both services keeps one row", hosts.length, 2)
    check("SMB wins the target when a host offers both", hosts[0].kind, "smb")
    check("an SSH-only host keeps its SSH target", hosts[1].kind, "sftp")
    check("an SSH-only host names its own row", hosts[1].label, "AITower")
}

function targets(check) {
    check("a host row targets the bare server root", Dnssd.hostUri({ label: "WaterTower", host: "WaterTower.local", kind: "smb" }), "smb://WaterTower.local/")
    check("an SSH-only host targets its own root", Dnssd.hostUri({ label: "AITower", host: "AITower.local", kind: "sftp" }), "sftp://AITower.local/")
    check("no host targets nothing", Dnssd.hostUri(null), "smb:///")
}

function wired(check) {
    var hosts = Dnssd.parse(WIRED)
    check("a service this does not read names no row", hosts.length, 2)
    check("the readable host beside it keeps its own row", hosts[0].host, "AIBox.local")
    check("an unresolved service names no row either", hosts[1].host, "NASbox.local")
    check("the SSH-only host keeps the SSH target", hosts[1].kind, "sftp")
}

function exports(check) {
    var body = "Export list for WaterTower.local:\n/mnt/user/media 192.168.1.0/24\n/mnt/user/backup *\n"
    check("export rows are the exported paths", Dnssd.parseExports(body).join("|"), "/mnt/user/media|/mnt/user/backup")
    check("the header line is not an export", Dnssd.parseExports("Export list for x:\n").length, 0)
    check("no answer reads as no exports", Dnssd.parseExports(null).length, 0)
}

function dnssd(check) {
    parse(check)
    kinds(check)
    wired(check)
    targets(check)
    exports(check)
}

function run(check) {
    parse(check)
    kinds(check)
    wired(check)
    targets(check)
    exports(check)
}
