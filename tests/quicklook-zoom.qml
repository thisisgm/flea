//@ pragma ShellId flea-quicklook-zoom-test

import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/PreviewKeys.js" as PreviewKeys

// Quick Look's image zoom on the real Preview: fit, + steps, a drag and the wheel, 0, a move, a reopen, a small picture, an SVG and a failed SVG redraw.
ShellRoot {
    id: shell

    readonly property string dir: Quickshell.env("QLZOOM_DIR")
    readonly property real tol: 1
    property int checks: 0
    property int failures: 0
    property bool finished: false
    // The full-decode layer being watched, and the decodes it has started since.
    property var watched: null
    property int loads: 0
    property int readies: 0
    property int sets: 0
    // The file the open zoom was made on: no frame may draw another file at a zoom.
    property string zoomPath: ""
    property real fitW: 0
    property real fitH: 0

    function log(line) { console.log("QLZOOM " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function check(cond, what) {
        shell.checks++
        if (!cond) { shell.failures++; shell.log("FAIL " + what) }
        return cond
    }
    function near(a, b) { return Math.abs(a - b) <= shell.tol }
    function item() { return look.surfaceItem() }
    function press(action) { PreviewKeys.act(action, { preview: look }) }
    function drawn() {
        var it = shell.item()
        return it !== null && it.status === "image"
    }
    // Every ViewportScrollBar under the viewport, the plural wrapper walked through and not counted.
    function bars(node, out) {
        var kids = node.children || []
        for (var i = 0; i < kids.length; i++) {
            if (String(kids[i]).indexOf("ViewportScrollBar") === 0 && kids[i].orientation !== undefined) out.push(kids[i])
            shell.bars(kids[i], out)
        }
        return out
    }
    // The wheel handler Quick Look builds for a zoomed picture, not the ones inside each bar: one while zoomed, none at fit.
    function scrollHandlers(node, out) {
        var kids = node.children || []
        for (var i = 0; i < kids.length; i++) {
            if (kids[i].objectName === "fleaScroll" && kids[i].parent.orientation === undefined) out.push(kids[i])
            shell.scrollHandlers(kids[i], out)
        }
        return out
    }
    // The bars drawn: both when the picture overflows both axes of the viewport, none at fit or when it fits.
    function barsUp(it) {
        var found = shell.bars(it.viewport, [])
        return found.filter(function (bar) { return bar.visible }).length
    }
    function fitRect(it, natW, natH, bandH) {
        var scale = Math.min(1, it.width / natW, it.height / natH)
        var w = natW * scale
        var h = natH * scale
        return { x: Math.round((it.width - w) / 2), y: Math.round((it.height - bandH - h) / 2), w: w, h: h }
    }
    function rectIs(it, want, label) {
        var p = it.pictureItem
        shell.check(shell.near(p.x, want.x) && shell.near(p.y, want.y) && shell.near(p.width, want.w) && shell.near(p.height, want.h),
            label + " drew " + p.x + "," + p.y + " " + p.width + "x" + p.height + ", want " + want.x + "," + want.y + " " + want.w + "x" + want.h)
    }
    function centre(f) { return { x: (f.contentX + f.width / 2) / f.contentWidth, y: (f.contentY + f.height / 2) / f.contentHeight } }
    // A + or - press keeps the viewport's centre point fixed on every axis the content overflows.
    function stepKeepsCentre(action, zoomWanted, label) {
        var it = shell.item()
        var before = shell.centre(it.viewport)
        shell.press(action)
        var f = it.viewport
        shell.check(it.zoom === zoomWanted, label + " zoom " + it.zoom + ", want " + zoomWanted)
        var after = shell.centre(f)
        if (f.contentWidth > f.width + 0.5) shell.check(Math.abs(after.x - before.x) < 0.001, label + " moved the horizontal centre " + before.x + " to " + after.x)
        if (f.contentHeight > f.height + 0.5) shell.check(Math.abs(after.y - before.y) < 0.001, label + " moved the vertical centre " + before.y + " to " + after.y)
    }

    // Waits for ready(), then runs then(); a stall is a failure of its own.
    property var waitReady: null
    property var waitThen: null
    property string waitWhat: ""
    property int waited: 0
    function until(ready, then, what) {
        shell.waitReady = ready
        shell.waitThen = then
        shell.waitWhat = what
        shell.waited = 0
        ticker.start()
    }
    Timer {
        id: ticker
        interval: 10
        repeat: true
        onTriggered: {
            if (shell.waitReady()) {
                ticker.stop()
                shell.waitThen()
            } else if (++shell.waited > 1500) {
                ticker.stop()
                shell.check(false, "timed out waiting for " + shell.waitWhat)
                shell.finish()
            }
        }
    }

    FloatingWindow {
        implicitWidth: 1000
        implicitHeight: 800
        color: Flea.Theme.color.background
        TestEvent { id: driver }
        Flea.Preview { id: look; anchors.fill: parent }
    }

    // Every frame: one of the two layers is drawn whenever the picture is, and no frame draws another file at a zoom.
    Connections {
        target: look.Window.window
        function onAfterAnimating() {
            var it = shell.item()
            if (it === null || !shell.drawn() || it.fallbackShown) return
            shell.check(it.pictureItem.visible !== it.fullPicture.visible, "a frame drew " + (it.pictureItem.visible ? "both" : "neither") + " layer")
            shell.check(it.zoom === 1 || it.path === shell.zoomPath, "a frame drew " + it.path + " at zoom " + it.zoom)
        }
    }
    Connections {
        target: shell.watched
        function onStatusChanged() {
            if (shell.watched.status === Image.Loading) shell.loads++
            if (shell.watched.status === Image.Ready) shell.readies++
        }
        function onSourceChanged() {
            if (String(shell.watched.source) !== "") shell.sets++
        }
    }

    function openFile(name, then) {
        look.open(shell.dir + "/" + name, "image-x-generic", 1, "image", "")
        shell.until(function () { return shell.drawn() && look.revealedOpen && shell.item().path === shell.dir + "/" + name },
            function () {
                var it = shell.item()
                shell.watched = it.fullPicture
                shell.loads = 0
                shell.readies = 0
                shell.sets = 0
                shell.zoomPath = it.path
                then(it)
            }, name + " drawn")
    }

    // A large picture: fit, the first step's single full decode, four presses to 200%, a drag, the wheel, 0.
    function large() {
        shell.openFile("large.png", function (it) {
            shell.log("STEP large fit " + it.width + "x" + it.height)
            var want = shell.fitRect(it, 3000, 2000, 0)
            shell.check(it.zoom === 1 && !it.band.visible && it.zoomLabel === "", "fit draws a band or a zoom label")
            shell.check(it.viewport.height === it.height && it.viewport.width === it.width, "fit viewport is not the whole surface")
            shell.check(shell.barsUp(it) === 0, "fit draws a scroll knob")
            shell.rectIs(it, want, "fit")
            shell.fitW = it.pictureItem.width
            shell.fitH = it.pictureItem.height
            shell.check(shell.loads === 0 && it.fullPicture.status === Image.Null && String(it.fullPicture.source) === "", "a full decode started at fit")
            shell.check(it.pictureItem.sourceSize.width === Math.round(it.width * it.Screen.devicePixelRatio) && it.pictureItem.sourceSize.height === Math.round(it.height * it.Screen.devicePixelRatio), "fit decode is not bound to the surface")

            shell.stepKeepsCentre("zoomIn", 1.25, "first +")
            shell.check(shell.loads === 1 && it.fullPicture.status === Image.Loading, "the first step past fit did not start exactly one full decode: loads " + shell.loads)
            shell.check(it.pictureItem.visible && it.pictureItem.status === Image.Ready && it.status === "image", "the fit picture is not drawn while the full decode runs")
            shell.check(it.band.visible && it.zoomLabel === "125%", "first step band reads '" + it.zoomLabel + "'")
            shell.until(function () { return it.fullPicture.status === Image.Ready && !it.pictureItem.visible }, function () { shell.largeSteps(it) }, "the full decode")
        })
    }
    function largeSteps(it) {
        shell.check(it.fullPicture.implicitWidth === 3000 && it.fullPicture.implicitHeight === 2000, "full decode is " + it.fullPicture.implicitWidth + "x" + it.fullPicture.implicitHeight + ", want the whole 3000x2000")
        shell.check(it.fullPicture.visible && !it.pictureItem.visible, "the full decode did not take the fit picture's place")
        shell.stepKeepsCentre("zoomIn", 1.5, "second +")
        shell.stepKeepsCentre("zoomIn", 1.75, "third +")
        shell.stepKeepsCentre("zoomIn", 2, "fourth +")
        shell.check(shell.loads === 1, "steps after the first started another full decode: loads " + shell.loads)
        shell.check(it.zoomLabel === "200%", "four presses read '" + it.zoomLabel + "', want 200%")
        var p = it.pictureItem
        shell.check(p.width === 2 * shell.fitW && p.height === 2 * shell.fitH, "200% drew " + p.width + "x" + p.height + ", want twice " + shell.fitW + "x" + shell.fitH)
        shell.check(it.fullPicture.x === p.x && it.fullPicture.y === p.y && it.fullPicture.width === p.width && it.fullPicture.height === p.height, "the full layer is not on the fit layer's rect")
        // The band: the media strip's height, on the surface colour, the label centred in the foreground at caption size.
        var band = it.band
        shell.check(band.visible && band.height === Flea.Theme.chromeHeight && band.y + band.height === it.height && band.x === 0 && band.width === it.width,
            "band is " + band.x + "," + band.y + " " + band.width + "x" + band.height + " in a " + it.width + "x" + it.height + " surface")
        shell.check(Qt.colorEqual(band.color, Flea.Theme.color.surface), "band is not the surface colour")
        var label = band.children[0]
        shell.check(label.text === "200%" && Qt.colorEqual(label.color, Flea.Theme.color.foreground) && label.font.pixelSize === Flea.Theme.font.caption && label.font.features.tnum === 1,
            "band label is '" + label.text + "' at " + label.font.pixelSize)
        shell.check(Math.abs(label.x + label.width / 2 - band.width / 2) < 1 && Math.abs(label.y + label.height / 2 - band.height / 2) < 1, "band label is not centred")
        var f = it.viewport
        shell.check(f.height === it.height - Flea.Theme.chromeHeight && f.width === it.width, "viewport is " + f.width + "x" + f.height + ", want the surface less the band")
        shell.check(f.contentWidth === p.width && f.contentHeight === p.height, "content is not the picture")
        shell.check(shell.barsUp(it) === 2, "200% draws " + shell.barsUp(it) + " viewport bars, want both")
        shell.check(shell.scrollHandlers(f, []).length === 1, "200% builds " + shell.scrollHandlers(f, []).length + " wheel handlers, want one")
        shell.drag(it)
    }

    function drag(it) {
        var f = it.viewport
        var x0 = f.width / 2
        var y0 = f.height / 2
        var from = { x: f.contentX, y: f.contentY }
        driver.mousePress(f, x0, y0, Qt.LeftButton, Qt.NoModifier, 5)
        for (var i = 1; i <= 12; i++) driver.mouseMove(f, x0 - 12 * i, y0 - 8 * i, 5, Qt.LeftButton, Qt.NoModifier)
        driver.mouseRelease(f, x0 - 144, y0 - 96, Qt.LeftButton, Qt.NoModifier, 5)
        shell.check(f.contentX > from.x + 20 && f.contentY > from.y + 10, "a drag left contentX " + from.x + " to " + f.contentX + " and contentY " + from.y + " to " + f.contentY)
        var found = shell.bars(f, [])
        shell.check(found.length === 2 && found[0].shown && found[1].shown, "the knobs are not shown while the view pans")
        shell.until(function () { return !f.moving && !f.flicking }, function () { shell.wheel(it) }, "the drag to settle")
    }
    function wheel(it) {
        var f = it.viewport
        var from = f.contentY
        // Toward the middle of the range, since the drag's flick may have left the view on a bound.
        var notch = from > (f.contentHeight - f.height) / 2 ? 120 : -120
        driver.mouseWheel(f, f.width / 2, f.height / 2, Qt.NoButton, Qt.NoModifier, 0, notch, 1)
        shell.until(function () { return Math.abs(f.contentY - from) > 1 }, function () { shell.fit(it) }, "the wheel to pan")
    }

    // 0 goes to fit in the same turn: the next frame draws no band, no knob and the fit rect.
    function fit(it) {
        shell.press("zoomFit")
        shell.check(it.zoom === 1 && !it.band.visible && it.zoomLabel === "", "0 left zoom " + it.zoom)
        shell.check(it.viewport.height === it.height && it.viewport.contentX === 0 && it.viewport.contentY === 0, "0 left the view panned or short")
        shell.check(shell.barsUp(it) === 0, "0 left a viewport bar")
        shell.check(shell.scrollHandlers(it.viewport, []).length === 0, "0 left a wheel handler")
        shell.rectIs(it, shell.fitRect(it, 3000, 2000, 0), "0")
        shell.check(it.pictureItem.visible && it.fullPicture.status === Image.Null, "0 kept the full decode")
        shell.stepKeepsCentre("zoomIn", 1.25, "+ after 0")
        shell.next()
    }

    // j: a move to the next file opens at fit, starts no full decode, and k back to the zoomed file opens at fit too.
    function next() {
        var asked = shell.loads
        var askedSets = shell.sets
        look.follow(shell.dir + "/large2.png", "image-x-generic", 1, "image", "")
        shell.until(function () { return shell.drawn() && shell.item().path === shell.dir + "/large2.png" },
            function () {
                var it = shell.item()
                shell.zoomPath = it.path
                shell.check(it.zoom === 1 && !it.band.visible && shell.barsUp(it) === 0, "the next file opened at zoom " + it.zoom)
                shell.check(shell.loads === asked && it.fullPicture.status === Image.Null, "a move to the next file started a full decode")
                shell.check(shell.sets === askedSets, "a move to the next file handed the full layer a file for a moment")
                shell.rectIs(it, shell.fitRect(it, 2400, 1600, 0), "next file")
                shell.stepKeepsCentre("zoomOut", 1, "- at fit")
                shell.back(it)
            }, "the next file drawn")
    }
    function back(it) {
        look.follow(shell.dir + "/large.png", "image-x-generic", 1, "image", "")
        shell.until(function () { return shell.drawn() && shell.item().path === shell.dir + "/large.png" },
            function () {
                shell.zoomPath = it.path
                shell.check(it.zoom === 1 && !it.band.visible, "k back to the zoomed file opened at zoom " + it.zoom)
                shell.reopen(it)
            }, "the first file drawn again")
    }

    // Closing and opening again returns to fit.
    function reopen(it) {
        shell.press("zoomIn")
        shell.check(it.zoom === 1.25, "+ on the first file")
        look.close()
        shell.until(function () { return look.surfaceItem() === null }, function () {
            shell.openFile("large.png", function (again) {
                shell.check(again.zoom === 1 && !again.band.visible, "a reopened Quick Look opened at zoom " + again.zoom)
                look.close()
                shell.until(function () { return look.surfaceItem() === null }, shell.small, "the close")
            })
        }, "the close")
    }

    // A picture smaller than the surface zooms from its own size, and fits the viewport so no knob draws.
    function small() {
        shell.openFile("small.png", function (it) {
            shell.rectIs(it, shell.fitRect(it, 120, 68, 0), "small fit")
            for (var i = 0; i < 4; i++) shell.press("zoomIn")
            var p = it.pictureItem
            var vh = it.height - Flea.Theme.chromeHeight
            shell.check(it.zoom === 2 && it.zoomLabel === "200%" && p.width === 240 && p.height === 136, "a 120x68 picture at 200% drew " + p.width + "x" + p.height)
            shell.check(shell.near(p.x, Math.round((it.width - 240) / 2)) && shell.near(p.y, Math.round((vh - 136) / 2)), "small picture at 200% is not centred in the viewport: " + p.x + "," + p.y)
            shell.check(it.band.visible && shell.barsUp(it) === 0, "a picture that fits the viewport drew a knob")
            shell.press("zoomFit")
            look.close()
            shell.until(function () { return look.surfaceItem() === null }, shell.vector, "the close")
        })
    }

    // An SVG is drawn again at each zoomed size, and the previous frame stays until the new one is ready.
    function vector() {
        shell.openFile("vector.svg", function (it) {
            shell.check(shell.near(it.pictureItem.width, it.width) && shell.near(it.pictureItem.height, it.height), "an SVG at fit does not fill the surface")
            var full = it.fullPicture
            shell.check(full.retainWhileLoading, "the SVG layer does not hold its last frame while it redraws")
            shell.press("zoomIn")
            shell.check(full.sourceSize.width === Math.round(it.width * 1.25 * it.Screen.devicePixelRatio) && full.sourceSize.height === Math.round(it.height * 1.25 * it.Screen.devicePixelRatio),
                "the first SVG step asked for " + full.sourceSize.width + "x" + full.sourceSize.height)
            shell.until(function () { return shell.readies === 1 && !it.pictureItem.visible }, function () {
                shell.press("zoomIn")
                shell.check(full.sourceSize.width === Math.round(it.width * 1.5 * it.Screen.devicePixelRatio) && full.sourceSize.height === Math.round(it.height * 1.5 * it.Screen.devicePixelRatio),
                    "the second SVG step asked for " + full.sourceSize.width + "x" + full.sourceSize.height)
                shell.check(full.visible && !it.pictureItem.visible, "the previous SVG frame was dropped while the new one decoded")
                shell.until(function () { return shell.readies === 2 }, function () {
                    shell.check(full.visible && !it.pictureItem.visible, "the redrawn SVG is not the layer drawn")
                    shell.press("zoomFit")
                    shell.check(it.zoom === 1 && full.status === Image.Null && it.pictureItem.visible && !full.visible, "an SVG back at fit kept its zoomed decode")
                    shell.failedRedraw(it)
                }, "the second SVG decode")
            }, "the first SVG decode")
        })
    }

    // A redraw that fails after a frame was drawn gives the fit picture back: the surface is never left with neither layer.
    function failedRedraw(it) {
        var full = it.fullPicture
        var before = shell.readies
        shell.press("zoomIn")
        shell.until(function () { return shell.readies === before + 1 && full.drawn && !it.pictureItem.visible }, function () {
            // Points the layer at a file that is not there, as a redraw the decoder cannot make does to it.
            full.source = "file://" + shell.dir + "/missing.svg"
            shell.until(function () { return full.status === Image.Error }, function () {
                shell.check(!full.drawn && !full.visible, "a failed SVG redraw kept the sharp layer drawn")
                shell.check(it.pictureItem.visible && it.status === "image", "a failed SVG redraw left the surface with no picture")
                shell.finish()
            }, "the failed SVG redraw")
        }, "the third SVG decode")
    }

    function finish() {
        if (shell.finished) return
        shell.finished = true
        ticker.stop()
        shell.log((shell.failures === 0 ? "PASS" : "DONE-FAIL") + " checks=" + shell.checks + " failures=" + shell.failures)
        shell.log("DONE")
        shell.quit()
    }

    Component.onCompleted: shell.large()
}
