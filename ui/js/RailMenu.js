.pragma library

.import "Menu.js" as Menu
.import "Mounts.js" as Mounts
.import "PlaceMenu.js" as PlaceMenu

// The rail's own menu: which one a row opens, what a chosen row does, and the Edit that rewrites a
// saved place. Split out of ui/js/Mounts.js, which parses and identifies rail rows and nothing more.

// Which menu a rail row opens, and with which handle: the trash and a favourite carry their own,
// every other row is a mount or a share and ui/js/Menu.js never sees it. Split out of ui/Sidebar.qml,
// which sits at its recorded line count, and this file already decides what a row offers.
function railMenuFor(sidebar, entry, scenePosition, hidden) {
    if (entry.kind === "trash") {
        sidebar.menu.openForRail("trash", Menu.trashEntries(sidebar.trashCount, false), scenePosition)
        return
    }
    // MenuAdditions rule 3: a Places or Favorites row opens the folder menu for its path while that
    // Extras row is on, and with it off a favourite keeps the one Remove it has offered since 0.2.1.
    if (entry.kind === "favourite" || entry.kind === "home") {
        var index = entry.kind === "favourite" ? entry.favouriteIndex : -1
        var rows = PlaceMenu.entries(entry, index, hidden)
        if (rows.length > 0) {
            sidebar.menu.openForRail(PlaceMenu.key(entry, index), rows, scenePosition)
            return
        }
        if (entry.kind !== "favourite")
            return
        sidebar.menu.openForRail("favourite:" + entry.favouriteIndex + ":" + JSON.stringify(entry.original),
            [{ label: "Remove", action: "removeFavourite", glyph: "minus" }], scenePosition)
        return
    }
    sidebar.menu.openForRail(Mounts.railKey(entry), Mounts.rowMenu(entry), scenePosition)
}

// Issue 21: Edit opens D11's dialog over the saved place, and the address that finally mounts is
// written back over that place's own line, so a share gio could not reach is corrected where it sits.
function editPlace(sidebar, share) {
    var entry = sidebar.networkEntries[share]
    if (!entry)
        return
    sidebar.editingPlace = Mounts.normalize(entry.uri)
    sidebar.editingRequest = ""
    sidebar.networkRetryRequested(entry.uri, entry.label, "", "Edit this address, then connect and save.", false, sidebar.navigationPane)
}

// Each attempt this dialog makes, so a mount that was already in flight when the rail armed cannot
// answer for the edit: a refusal only re-binds, because the next attempt is the one that counts.
function placeSubmitted(sidebar, requestId) {
    if (sidebar.editingPlace.length > 0)
        sidebar.editingRequest = requestId
}

// The answer to that edit: a refused connect keeps the arm, because correcting an address gio could
// not reach is what the dialog stays open for; ui/shell.qml disarms it when that dialog closes.
function placeSaved(sidebar, mounts, requestId, uri, success) {
    if (!success || sidebar.editingPlace.length === 0 || requestId !== sidebar.editingRequest)
        return
    var was = sidebar.editingPlace
    sidebar.editingPlace = ""
    sidebar.editingRequest = ""
    if (Mounts.normalize(uri) !== was)
        mounts.replacePlace(was)
}

// The release a row's own menu offers: Eject when it is there, else the row's Unmount, whatever
// position Open holds. Ctrl+E dispatches this through ui/js/Eject.js, so the key means release on
// every row, where reading the menu's first row dispatched Open on any menu that leads with it.
function releaseOf(entry) {
    var rows = Mounts.railMenu(entry)
    var unmount = ""
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].action === "eject")
            return "eject"
        if (String(rows[i].action).indexOf("unmount") === 0 && unmount.length === 0)
            unmount = rows[i].action
    }
    return unmount
}

// Issue 76: the release above, drawn on the row itself by ui/SidebarRow.qml. Two rows keep theirs
// in the menu alone, which is why the cut is made here and not in railMenu. NFS is the issue's own
// carve-out, because an NFS export is not something to disconnect from a rail click. And a mounted
// internal volume is /home and its siblings: rule 1's switch gives it an Unmount row, and a stray
// click at the rail's edge must not be an invitation to it, so only a volume somebody pulls out or
// attached themselves draws the mark, which is also exactly the set Nautilus draws it on.
function releaseMark(entry) {
    if (!entry || entry.mounted !== true)
        return ""
    if (entry.group === "network" && /^nfs:/i.test(String(entry.uri || "")))
        return ""
    if (entry.group === "device" && entry.kind === "volume"
            && entry.removable !== true && entry.attached !== true)
        return ""
    return releaseOf(entry)
}

// The rail menu's chosen row, handed the row's key rather than its position: the rail rebuilds on
// a five second poll, so the index the menu opened over can name a different row by now. A key
// that no longer names a row does nothing, because the row it named has left the rail already.
// Both Services re-check the kind themselves; this only resolves which row was meant, and the rail
// itself owns the two rows that need no mount at all.
function release(action, key, devices, mounts, sidebar) {
    if (action === "eject") {
        var volume = Mounts.rowByKey(sidebar.deviceEntries, key)
        if (volume >= 0)
            devices.eject(volume)
        return
    }
    // RailAdditions rule 2: Mount and Open are the row's own activation, which mounts when it has to
    // and opens either way, so the menu row and Enter cannot drift apart.
    if (action === "mountVolume" || action === "openVolume" || action === "unmountVolume") {
        var row = Mounts.rowByKey(sidebar.deviceEntries, key)
        if (row < 0)
            return
        if (action === "unmountVolume") devices.unmount(row)
        else devices.activate(row)
        return
    }
    // The phone Service is the sidebar's own child, so the sidebar resolves the key against it; a
    // phone unmounts rather than ejects, because gvfs answers can_eject=0 for the MTP monitor.
    if (action === "unmountPhone") {
        sidebar.releasePhone(key)
        return
    }
    // Mount and Open are the row's own activation, which mounts when it has to and opens either way.
    if (action === "mountPhone" || action === "openPhone") {
        sidebar.openPhone(key)
        return
    }
    var share = Mounts.rowByKey(sidebar.networkEntries, key)
    if (share < 0)
        return
    if (action === "unmount")
        mounts.unmount(share)
    else if (action === "editPlace")
        editPlace(sidebar, share)
    else if (action === "rename")
        sidebar.startRename(sidebar.placesEntries.length + share)
    else if (action === "remove")
        mounts.forget(sidebar.networkEntries[share].uri)
}
