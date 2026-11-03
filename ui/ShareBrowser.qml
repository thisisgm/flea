import QtQuick
import "." as Flea

// Shows a bare smb://host/'s own shares as ordinary pane rows, the same instantiated-from-shell
// overlay pattern as ui/EmptyState.qml; see ui/NetworkMounts.qml "listShares" for the source data.
Item {
    id: root

    property bool active: false
    property string baseUri: ""
    property string baseLabel: ""
    property var shares: []
    property int cursorIndex: 0
    // The ＋'s wait has its own page: discovery can take seconds to answer, and a blank pane
    // reads as broken. The crawl says the looking is ours, not the network's silence.
    property bool scanning: false
    // The crawl's one word: "looking" while the LAN is asked, "reading" while a host's own
    // shares are — ui/LoadingState.qml's word for a slow source, the same continuity.
    property string scanWord: "looking"
    // The hosts page the shares page came from, so ".." steps back to it without re-asking avahi.
    property var hostsPage: null

    signal closed()
    // The resolved share uri and name, the same shape openShare() already takes for a bookmarked share; ui/WindowBody.qml routes this through the window-long host's openChildShare.
    signal activated(string uri, string label)
    // The hosts page's own last row, so the form stays one click away from discovery.
    signal manualRequested()

    // The hosts page rides the bare workgroup root and ends in the form: the row is a name like
    // any other so navigation, hover and keys treat it identically, and activation is the one
    // special case below.
    readonly property string manualRow: "Add manually…"
    // The shares page's first row: back to the server list it came from, the one level up the
    // pane's own navigation cannot express, because the pane is already inside the share.
    readonly property string backRow: ".."

    // active flips instantly (open()/close() below), so shareBrowserOpen()'s IPC read never races
    // the close fade; visible only stays true a little longer, until content's opacity finishes it.
    visible: root.active || content.opacity > 0

    // The fade and the rise are the shared reveal of ui/Reveal.qml, built by the first open so a window that never browses shares holds none.
    property var fade: null
    onActiveChanged: {
        if (root.active && root.fade === null)
            root.fade = Qt.createComponent("Reveal.qml").createObject(root, { target: content, shown: Qt.binding(function () { return root.active }), lift: content })
    }

    function open(uri, label, names) {
        root.baseUri = uri
        root.baseLabel = label
        // A shares page that arrived from the hosts page carries the way back as its first row;
        // the hosts page itself and a bare-root place opened without one do not.
        if (uri !== "smb://" && root.hostsPage !== null && (names.length === 0 || names[0] !== root.backRow))
            names = [root.backRow].concat(names)
        root.shares = names
        root.cursorIndex = 0
        root.scanning = false
        root.active = true
    }

    // The ＋'s own first page: opened the instant the browse is asked for, replaced by the hosts
    // rows when avahi answers (openHosts below) or by the form when the LAN stays quiet.
    function openScanning() {
        root.scanWord = "looking"
        root.openCrawl("Network")
    }

    // A host row's activation opens its page the same instant: mount, info and the listing can
    // take seconds on a cold contact, and the crawl holds the pane for every one of them. The
    // shares (open below) or the sign-in form replaces it.
    function openReading(label) {
        root.scanWord = "reading"
        root.openCrawl(label || "")
    }

    function openCrawl(label) {
        root.baseUri = "smb://"
        root.baseLabel = label
        root.shares = []
        root.cursorIndex = 0
        root.scanning = true
        root.active = true
    }

    // Discovery's first page: the LAN's hosts, one row each, with the form behind the last row.
    function openHosts(names) {
        var rows = names.slice(0)
        rows.push(root.manualRow)
        root.hostsPage = rows
        root.open("smb://", "Network", rows)
    }

    // Idempotent, so wiring this to every Pane navigation (including one this overlay itself just requested) costs nothing once already shut.
    function close() {
        if (!root.active)
            return
        root.active = false
        // The hosts page's way back dies with the session that asked for it: a later unrelated
        // server root must not carry a ".." row into a list that ended.
        root.hostsPage = null
        root.closed()
    }

    function moveCursor(delta) {
        if (root.shares.length === 0)
            return
        root.cursorIndex = Math.max(0, Math.min(root.shares.length - 1, root.cursorIndex + delta))
    }

    function activateCursor() {
        var name = root.shares[root.cursorIndex]
        if (name === undefined)
            return
        if (name === root.manualRow) {
            root.manualRequested()
            return
        }
        // The way back to the server list: the cached hosts page reopens as it stood, no second
        // ask of the LAN for a list it just gave.
        if (name === root.backRow && root.hostsPage !== null) {
            root.open("smb://", "Network", root.hostsPage)
            return
        }
        // A row that is itself a URI — an SSH-only host's root, or an NFS export read beside a
        // host's SMB shares — activates as itself; a plain name is a share of the open base.
        if (name.indexOf("://") >= 0) {
            root.activated(name, name)
            return
        }
        var base = root.baseUri.charAt(root.baseUri.length - 1) === "/" ? root.baseUri : root.baseUri + "/"
        root.activated(base + name + "/", name)
    }

    // Everything visible sits under this one Item so open/close can animate it as a unit; root
    // itself stays plain (no x/y/opacity) so anchors.fill callers of ShareBrowser are unaffected.
    Item {
        id: content
        x: 0
        width: root.width
        height: root.height
        // Closed means no input, even while the fade-out is still drawn; ui/Reveal.qml owns the fade and the rise.
        enabled: root.active
        opacity: 0

        // Opaque so the real listing behind it, still rendering, never shows or flickers through.
        Rectangle {
            anchors.fill: parent
            color: Theme.color.background
        }

        // Swallows clicks in the gap below the rows so they cannot reach the hidden list beneath.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        // The ＋'s wait page, LoadingState's crawl on the same terms: the spiral mark and one
        // word, centered, shown while discovery owes an answer. Rows and the crawl never share
        // the page — the answer replaces the looking.
        Column {
            anchors.centerIn: parent
            visible: root.scanning
            spacing: Theme.spacing.gap

            Spinner {
                anchors.horizontalCenter: parent.horizontalCenter
                width: Theme.heroMarkSize
                height: Theme.heroMarkSize
            }

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.scanWord
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }
        }

        Column {
            anchors.fill: parent
            visible: !root.scanning

            Repeater {
                model: root.shares

                // Network.dc.html draws the sidebar's own server mark and the share name, and no
                // columns: nothing is known about a share until it is mounted, so a "-" size and a
                // "--" date were columns of nothing. ui/MenuRow.qml is the mark-and-label row.
                delegate: Flea.MenuRow {
                    id: shareRow
                    required property int index
                    required property string modelData
                    width: root.width
                    entry: ({ label: shareRow.modelData, action: "", glyph: "server" })
                    // The keyboard cursor is the pick and takes the accent; the pointer only lifts
                    // the row it is over, which is what ui/Row.qml did here before.
                    picked: shareRow.index === root.cursorIndex
                    current: shareRow.hovered
                    onActivated: {
                        root.cursorIndex = shareRow.index
                        root.activateCursor()
                    }
                }
            }
        }
    }
}
