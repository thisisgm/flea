.import "../../ui/js/Discovery.js" as Discovery

function run(check) {
    var body = [
        '=;eth0;IPv4;Office\\032SSH;_ssh._tcp;local;workstation.local;192.168.1.8;22;"mac=aa:bb:cc:dd:ee:ff"',
        '=;eth0;IPv6;IPv6 SSH;_ssh._tcp;local;;fe80::1;22;',
        '=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.168.1.9;445;',
        '=;eth0;IPv4;Secure Docs;_webdavs._tcp;local;docs.local;192.168.1.10;8443;',
        '+;eth0;IPv4;Unresolved;_ssh._tcp;local',
        '=;eth0;IPv4;Ignored;_printer._tcp;local;print.local;192.168.1.11;631;',
        '=;eth0;IPv4;Bad;_ssh._tcp;local;bad host;192.168.1.12;22;'
    ].join('\n')
    var rows = Discovery.parse(body)
    check("only supported resolved services survive", rows.length, 4)
    check("Avahi decimal escapes decode", rows.map(function (r) { return r.label }).indexOf("Office SSH") >= 0, true)
    check("default ports normalize away", rows.filter(function (r) { return r.label === "NAS" })[0].uri, "smb://nas.local/")
    check("non-default ports survive", rows.filter(function (r) { return r.label === "Secure Docs" })[0].uri, "davs://docs.local:8443/")
    check("IPv6 is bracketed", rows.filter(function (r) { return r.uri.indexOf("fe80") >= 0 })[0].uri, "sftp://[fe80::1]/")
    check("a TXT MAC is normalized for Wake", rows.filter(function (r) { return r.label === "Office SSH" })[0].mac, "aa:bb:cc:dd:ee:ff")
    check("empty and malformed discovery are harmless", Discovery.parse("garbage\n").length, 0)
}
