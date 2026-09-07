.pragma library

.import "Format.js" as Format

// The one drawer behind every reclaim view. It takes the shapes ui/js/ReclaimTree.js's layouts
// answer and puts them on a Canvas context, in the palette ui/ReclaimMap.qml builds from the
// theme: accent fills scaled by depth, the foreground for text, nothing hard-coded. Every draw
// clears its box first, because a Canvas keeps the previous view's pixels otherwise.

function draw(ctx, shapes, pal, picked, w, h) {
    ctx.clearRect(0, 0, w, h)
    for (var i = 0; i < shapes.length; i++) {
        var s = shapes[i]
        switch (s.k) {
        case "rect":
            // The scan root is the container, so it takes the background; every real tree takes
            // the accent at its depth's strength, and a background hairline separates the cells.
            ctx.fillStyle = (s.depth || 0) === 0 ? pal.bg : pal.accent
            ctx.globalAlpha = (s.depth || 0) === 0 ? 1 : 1 - Math.min(0.55, ((s.depth || 0) - 1) * 0.22)
            ctx.fillRect(s.x, s.y, s.w, s.h)
            ctx.globalAlpha = 1
            ctx.strokeStyle = pal.bg
            ctx.lineWidth = 1
            ctx.strokeRect(s.x + 0.5, s.y + 0.5, Math.max(1, s.w - 1), Math.max(1, s.h - 1))
            break
        case "arc":
            ctx.globalAlpha = 1 - Math.min(0.55, (s.depth || 0) * 0.22)
            ctx.fillStyle = pal.accent
            ctx.beginPath()
            ctx.arc(s.cx, s.cy, s.r1, s.a0, s.a1, false)
            ctx.arc(s.cx, s.cy, s.r0, s.a1, s.a0, true)
            ctx.closePath()
            ctx.fill()
            ctx.globalAlpha = 1
            break
        case "disc":
            ctx.fillStyle = pal.accent
            ctx.beginPath()
            ctx.arc(s.cx, s.cy, s.r, 0, Math.PI * 2, false)
            ctx.fill()
            break
        case "dot":
            ctx.globalAlpha = 1 - Math.min(0.6, (s.depth || 0) * 0.28)
            ctx.fillStyle = pal.accent
            ctx.beginPath()
            ctx.arc(s.cx, s.cy, s.r, 0, Math.PI * 2, false)
            ctx.fill()
            ctx.globalAlpha = 1
            break
        case "line":
            ctx.strokeStyle = pal.muted
            ctx.lineWidth = 1
            ctx.beginPath()
            ctx.moveTo(s.x1, s.y1)
            ctx.lineTo(s.x2, s.y2)
            ctx.stroke()
            break
        case "label":
            ctx.fillStyle = s.node.depth === 0 ? pal.fg : pal.onAccent
            ctx.font = pal.small
            clipText(ctx, s.node.name, s.x, s.y, s.w)
            break
        case "clabel":
            ctx.fillStyle = pal.fg
            ctx.font = pal.small
            ctx.textAlign = "center"
            clipText(ctx, s.node.name, s.cx - s.w / 2, s.cy + 3, s.w)
            ctx.textAlign = "left"
            break
        case "slabel":
            ctx.fillStyle = pal.fg
            ctx.font = pal.small
            clipText(ctx, s.node.name + "  ·  " + Format.size(s.node.bytes), s.x, s.y, s.w)
            break
        case "text":
            ctx.fillStyle = pal.muted
            ctx.font = pal.small
            ctx.fillText(s.text, s.x, s.y)
            break
        }
        // The picked row's own shapes take the foreground outline, so a click reads back.
        if (s.node && s.node === picked) {
            ctx.strokeStyle = pal.fg
            ctx.lineWidth = 1.5
            if (s.k === "rect") {
                ctx.strokeRect(s.x + 0.5, s.y + 0.5, Math.max(1, s.w - 1), Math.max(1, s.h - 1))
            } else if (s.k === "dot" || s.k === "disc") {
                ctx.beginPath()
                ctx.arc(s.cx, s.cy, s.r + 1, 0, Math.PI * 2, false)
                ctx.stroke()
            }
        }
    }
    ctx.globalAlpha = 1
}

// A name never draws past its shape: cut by characters, the cheap way that needs no measure pass.
function clipText(ctx, text, x, y, w) {
    var max = Math.floor(w / 6.5)
    var t = String(text)
    if (t.length > max) {
        t = max > 1 ? t.substring(0, max - 1) + "…" : ""
    }
    ctx.fillText(t, x, y)
}
