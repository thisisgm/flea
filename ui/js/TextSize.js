.pragma library

// Flea's text size, which is the Display section's one control. The SettingsScale board rules the
// shape: Omarchy owns the size by default, and an override pins one stop from Omarchy's own range
// rather than a free number or a multiplier of its own. The monitor scale beside it belongs to the
// compositor, which is why nothing here steps or cycles one.

// The board's seven documented stops, in px, read off its own layout table. They are not
// contiguous: 13, 15, 17, 18 and 19 are not stops. The range is the OEM one, 9 to 20, so Flea
// offers no size Omarchy does not.
var STOPS = [9, 10, 11, 12, 14, 16, 20]

// Omarchy's own type ladder, from Commons/Style.qml, whose bodySmall is fontPx(0.917) and caption
// fontPx(0.833) over the base size. Running the same two ratios at a stop is what makes an override
// the size Omarchy itself would have drawn, rather than a second ladder that can disagree with it.
var BODY_SMALL_RATIO = 0.917
var CAPTION_RATIO = 0.833

var FOLLOW = "system"

function follow() {
    return { mode: FOLLOW }
}

// Follow Omarchy is the default, and the board names its stored shape: {"mode":"system"}. An
// override is the same key holding a number instead, which is src/uischema.rs's own display.textSize
// rule: "system", or one stop the OEM panel could have produced. There is no second stored shape.
function following(stored) {
    return !stored || !(Number(stored.mode) > 0)
}

// The nearest stop, so a base Omarchy invented that is not on the list still enters the override on
// a real stop, and a hand-edited file cannot pin a free number. A tie takes the smaller stop, which
// is what the strict comparison below does: 13 enters at 12 and 15 at 14, so nothing grows unasked.
function nearest(px) {
    var best = STOPS[0]
    for (var i = 1; i < STOPS.length; i++) {
        if (Math.abs(STOPS[i] - px) < Math.abs(best - px))
            best = STOPS[i]
    }
    return best
}

// The size Flea actually draws at: Omarchy's own while following, the stored stop while overriding.
function effective(stored, omarchyBase) {
    return following(stored) ? omarchyBase : nearest(Number(stored.mode))
}

// Switching to Override starts on the stop beside the size already on screen, so the switch itself
// changes nothing and only a step does.
function pin(stored, omarchyBase) {
    return { mode: nearest(effective(stored, omarchyBase)) }
}

// One stop along the list, clamped at both ends. Stepping while following becomes an override,
// which is what the first press of Ctrl+Shift+Plus should do.
function stepped(stored, omarchyBase, direction) {
    var at = STOPS.indexOf(pin(stored, omarchyBase).mode)
    var to = Math.max(0, Math.min(STOPS.length - 1, at + direction))
    return { mode: STOPS[to] }
}

// A stored value another hand wrote, and 0.1.3's own {"mode":"override","px":N}: anything this
// cannot read as a size follows Omarchy, which is both the default and the state deleting ui.json
// restores, and a number that is not a stop lands on the nearest one rather than pinning a size the
// OEM panel could not produce. src/uistate.rs refuses the same shapes on the way in.
function parse(stored) {
    var px = stored ? Number(stored.mode) : NaN
    return px > 0 ? { mode: nearest(px) } : follow()
}

function bodySmall(base) {
    return Math.max(1, Math.round(base * BODY_SMALL_RATIO))
}

function caption(base) {
    return Math.max(1, Math.round(base * CAPTION_RATIO))
}

// The sentence the status bar shows, because a size changed by a chord with the panel shut is
// otherwise a state with no readout and no named way back.
function announce(stored, omarchyBase) {
    var px = effective(stored, omarchyBase) + "px"
    return following(stored)
        ? "Text size follows Omarchy, " + px + "."
        : "Text size " + px + ". Ctrl+Shift+0 follows Omarchy again."
}
