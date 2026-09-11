.pragma library

// Sample input: { transient: "Copy failed", transientIsError: true, searching: true, searchKeys: "esc cancels", stickyHere: true, sticky: "Copying 2 of 5", fsText: "btrfs" }
function errorHere(slot) {
    return slot.transient.length > 0 && slot.transientIsError
}

// GM's ordering: acknowledged errors leave the slot; activity cannot displace them.
function rightText(slot) {
    if (errorHere(slot))
        return slot.transient
    if (slot.stickyHere)
        return slot.sticky
    if (slot.searching)
        return slot.searchKeys
    return slot.transient.length > 0 ? slot.transient : slot.fsText
}

function rightRole(slot) {
    return errorHere(slot) ? "error" : "foreground"
}

// Each backend numbers its own transfers, so an id is meaningful only with its pane owner.
function activityChanged(activities, owner, text, transfer) {
    var next = activities.slice()
    var at = next.findIndex(function (activity) { return activity.owner === owner })
    if (!text) {
        if (at >= 0) next.splice(at, 1)
        return next
    }
    var previous = at >= 0 ? next[at] : null
    var activity = { owner: owner, text: text, transfer: transfer,
                     cancelling: !!(previous && previous.transfer.id === transfer.id && previous.cancelling) }
    if (at >= 0) next[at] = activity
    else next.push(activity)
    return next
}

function cancelActivity(activities) {
    if (!activities.length || !activities[0].transfer.running || activities[0].cancelling)
        return activities
    var next = activities.slice()
    next[0] = Object.assign({}, next[0], { cancelling: true })
    return next
}
