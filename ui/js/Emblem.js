.pragma library

// The sync badge a row draws over its icon, from the row's "e" field (docs/protocol.md).
// The backend sends only these five words, see src/backend/emblem.rs.

// Sample input: "synced" answers "check"; "" answers "", which draws no badge.
function glyph(status) {
    return { synced: "check", syncing: "sync", partial: "minus", conflict: "alert", excluded: "cloud" }[status] || ""
}

// The Theme.color role that fills the badge, so it follows the Omarchy theme like every other colour.
function role(status) {
    return { synced: "executable", syncing: "accent", conflict: "error" }[status] || "muted"
}

// The badge exists only on a tagged row, so an untagged delegate keeps its object count.
// Answers the item to hold: the old one updated, a new one, or null once the tag is gone.
function sync(item, url, parent, props) {
    if (glyph(props.status) === "") {
        if (item) item.destroy()
        return null
    }
    if (item) {
        item.status = props.status
        return item
    }
    return Qt.createComponent(url).createObject(parent, props)
}
