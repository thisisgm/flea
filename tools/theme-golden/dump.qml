import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons as Commons
import "Oracle" as Flea

// The runner supplies read-only Commons and frozen Flea modules plus an isolated marked HOME.
ShellRoot {
    id: root
    property var palettes: []
    property int at: 0
    property int stop: 12
    property bool failed: false

    FileView {
        id: manifest
        path: Quickshell.env("FLEA_GOLDEN_MANIFEST")
        blockLoading: true
        printErrors: true
    }

    function row(key, value) {
        var text = String(value)
        if (/[\t\r\n]/.test(text)) throw new Error("TSV value contains a line break or tab: " + key)
        console.log("THEME_GOLDEN\t" + palettes[at].name + "\t" + stop + "\t" + key + "\t" + text)
    }
    function channelByte(n) { return ("0" + Math.round(n * 255).toString(16)).slice(-2) }
    function color(c) {
        if (typeof c === "string") c = Qt.color(c)
        return "#" + channelByte(c.a) + channelByte(c.r) + channelByte(c.g) + channelByte(c.b)
    }
    function number(key, n) { row(key, Number(n).toFixed(6)) }

    function input(section, values) {
        var keys = Object.keys(values)
        for (var i = 0; i < keys.length; i++) {
            var order = ("000000" + i).slice(-6)
            row("input." + section + "." + order + "." + keys[i], values[keys[i]])
        }
    }

    function dumpTags(found, bg, surface) {
        var names = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"]
        var roles = ["red", "orange", "yellow", "green", "blue", "magenta", "dark_foreground"]
        var seen = {}, fallback = false
        for (var i = 0; i < names.length; i++) {
            var hex = found[roles[i]] || ""
            if (!hex) fallback = true
            else {
                var key = hex.toLowerCase()
                if (seen[key] || (i !== 6 && tagIsGrey(hex))) fallback = true
                seen[key] = true
            }
            row("tag.colors." + names[i], hex ? color(Flea.Theme.tag.colors[names[i]]) : "")
            row("tag.menuColors." + names[i], hex ? color(Flea.Theme.tag.menuColors[names[i]]) : "")
        }
        row("tag.fallback", fallback)
        row("tag.swatchSize", 14)
    }

    // Keep Contrast.tagTheme's raw RGB comparison; QColor HSV quantization changes the 0.12 boundary.
    function tagIsGrey(hex) {
        var r = parseInt(hex.substr(1, 2), 16) / 255
        var g = parseInt(hex.substr(3, 2), 16) / 255
        var b = parseInt(hex.substr(5, 2), 16) / 255
        var hi = Math.max(r, g, b), lo = Math.min(r, g, b)
        return hi === 0 || (hi - lo) / hi < 0.12
    }

    function dump() {
        var theme = Flea.Theme, style = Commons.Style, c = theme.color
        var names = ["background", "foreground", "opaqueForegroundBright", "foregroundBright", "opaqueMuted", "muted",
            "opaqueMutedFrame", "mutedFrame", "opaqueMetadata", "metadata", "opaqueMetadataFrame", "metadataFrame", "accent",
            "opaqueError", "error", "opaqueErrorOnSurface", "errorOnSurface", "surface", "opaqueSymlink", "symlink",
            "opaqueExecutable", "executable", "opaqueAccentFrame", "accentFrame"]
        for (var i = 0; i < names.length; i++) row("color." + names[i], color(c[names[i]]))
        row("color.accentHasHue", c.accentHasHue)
        row("ready", theme.ready)
        row("baseSize", theme.baseSize)
        row("font.family", style.font.family)
        names = ["caption", "bodySmall", "body", "subtitle", "title", "heading", "display", "displayLarge", "iconSmall", "icon", "iconLarge"]
        for (i = 0; i < names.length; i++) row("font." + names[i], style.font[names[i]])
        names = ["hairline", "xxs", "xs", "sm", "md", "lg", "xl", "xxl", "xxxl", "huge", "controlGap", "controlPaddingX",
            "controlPaddingY", "inputPaddingY", "controlHeight", "popupRowHeight", "dropdownWidth", "searchableDropdownWidth",
            "numberFieldWidth", "searchablePopupMinHeight", "rowGap", "rowPaddingX", "labelGap", "panelGap", "panelPadding", "popupPadding"]
        for (i = 0; i < names.length; i++) row("spacing." + names[i], style.spacing[names[i]])
        number("spacing.scale", style.spacing.scale)
        names = ["normal", "hover", "selected", "pressed", "focus", "selection"]
        for (i = 0; i < names.length; i++) {
            var name = names[i]
            row("style." + name + "Fill", color(style[name === "focus" ? "focusFillColor" : name + "Fill"]))
            number("style." + name + "FillAlpha", style[name + "FillAlpha"])
            if (name !== "pressed" && name !== "selection") {
                row("style." + name + "BorderColor", color(style[name + "BorderColor"]))
                number("style." + name + "BorderWidth", style[name + "BorderWidth"])
            }
        }
        row("style.selectedAccentFill", color(style.selectedAccentFill))
        row("cursorWash", color(style.selectedAccentFill))
        row("inactiveCursorWash", color(style.selectedFill))
        row("menuCursorWash", color(Qt.alpha(Commons.Color.foreground, 0.08)))
        row("windowHairline", color(Qt.alpha(Commons.Color.foreground, 0.12)))
        row("rule", color(Qt.alpha(c.mutedFrame, 0.4)))
        row("primaryWash", color(Qt.alpha(Commons.Color.accent, 0.14)))
        var plate = style.selectionFill, alpha = Math.round(plate.a * 100)
        while (alpha > 12 && contrast(c.foreground, over(plate, alpha / 100, c.background)) < 4.5) alpha--
        row("markedPlate", color(Qt.alpha(plate, alpha / 100)))
        number("markedPlateAlpha", alpha / 100)
        row("cornerRadius", 0)
        row("gapsOut", 5)
        row("reducedMotion", false)
        dumpTags(theme.palette, c.background, c.surface)
    }

    // Only the approved marked-plate recipe is new; resolved inks come directly from the frozen QML.
    function over(fg, alpha, bg) {
        return Qt.rgba(Math.round((fg.r * alpha + bg.r * (1 - alpha)) * 255) / 255,
            Math.round((fg.g * alpha + bg.g * (1 - alpha)) * 255) / 255,
            Math.round((fg.b * alpha + bg.b * (1 - alpha)) * 255) / 255, 1)
    }
    function luminance(c) {
        function linear(v) { return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    }
    function contrast(a, b) {
        var la = luminance(a), lb = luminance(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }

    function guarded(action) {
        if (failed) return
        try { action() }
        catch (error) {
            failed = true
            console.error("THEME_GOLDEN_FAIL: " + String(error))
            Quickshell.execDetached(["kill", "-TERM", String(Quickshell.processId)])
        }
    }

    function next() {
        guarded(function() {
            if (at === palettes.length) {
                console.log("THEME_GOLDEN_DONE")
                Quickshell.execDetached(["kill", String(Quickshell.processId)])
                return
            }
            var p = palettes[at], user = "[font]\nbase-size = " + stop
            Commons.Color.foreground = "#cacccc"
            Commons.Color.background = "#101315"
            Commons.Color.accent = "#cacccc"
            Commons.Color.urgent = "#a55555"
            Commons.Color.muted = "#707880"
            Flea.ViewState.setTextSize({mode: "system"})
            Commons.Color.loadShell(p.shell)
            Commons.Color.loadUserShell(user)
            Flea.Theme.applyColors(p.colors)
            Commons.Style.cornerRadius = 0
            Commons.Style.gapsOut = 5
            Commons.Style.reduceMotion = false
            Qt.callLater(function() {
                guarded(function() {
                    input("colors", Flea.Theme.palette)
                    input("shell", Commons.Color.parseShell(p.shell))
                    input("user", Commons.Color.parseShell(user))
                    dump()
                    if (stop === 12) stop = 14
                    else { stop = 12; at++ }
                    Qt.callLater(next)
                })
            })
        })
    }

    Timer {
        interval: 750
        running: true
        onTriggered: {
            root.guarded(function() {
                root.palettes = JSON.parse(manifest.text())
                Commons.Color.colorsFile.waitForJob()
                Commons.Color.shellFile.waitForJob()
                Commons.Color.userShellFile.waitForJob()
                // The probe's fixture is opaque regardless of the controller's compositor transparency setting.
                Flea.Glass.clear = false
                root.next()
            })
        }
    }
}
