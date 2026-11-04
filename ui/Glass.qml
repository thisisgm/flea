pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import "js/Material.js" as Material

// shell.toml's [flea] background-alpha through Omarchy's reader, and Omarchy's Window Transparency mode: one plane per pixel, content opaque; AGENTS.md "Theme roles and sources".
QtObject {
    id: root

    readonly property string key: "flea.background-alpha"
    // Pick candidate A: match Nautilus's 0.75 ground, lifting text rather than fading it like window opacity does.
    property real glassAlpha: root.mode === "glassy" && root.usesDefault ? Material.floor(Theme.color.opaquePalette, Theme.textContrast) / 255 : 1
    property bool focused: Qt.application.active
    readonly property bool split: true
    readonly property bool usesDefault: root.valueIn(Commons.Color.userShellValues) < 0 && root.valueIn(Commons.Color.themeShellValues) < 0
    // "opaque", "glassy" or "transparent" (Omarchy's default), read from its toggle files by reread().
    property string mode: "transparent"
    property bool hasOpaque: false
    property bool hasGlassy: false
    // Set once by launch(), before the window exists, because a Wayland surface's format is fixed at creation.
    property bool clear: false
    // In whole 8-bit steps, so a value that rounds to 1 is 1; a launch a [flea] value pins opaque holds 1 until the next launch.
    readonly property real alpha: root.clear ? root.stepped(root.wanted()) : 1
    // False keeps every plane on its own opaque role, so a stock theme draws exactly what it drew before.
    readonly property bool on: root.alpha < 1
    readonly property color background: root.on ? Util.alpha(Theme.color.background, root.alpha) : Theme.color.background
    readonly property color surface: root.on ? Util.alpha(Theme.color.surface, root.alpha) : Theme.color.surface
    readonly property real listAlpha: root.on && root.usesDefault && root.mode === "glassy" && root.split ? Math.max(root.alpha, Material.listFloor(Theme.color.opaquePalette, Theme.textContrast) / 255) : root.alpha
    readonly property color listBackground: root.on ? Util.alpha(Theme.color.background, root.listAlpha) : Theme.color.background

    readonly property string toggleDir: (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/omarchy/toggles/hypr"
    // A file test and not omarchy-hyprland-window-transparency, which needs OMARCHY_PATH that ssh and some launchers lack.
    property FileView opaqueFile: FileView {
        path: root.toggleDir + "/window-opaque.lua"
        printErrors: false
        onLoaded: root.hasOpaque = true
        onLoadFailed: root.hasOpaque = false
    }
    property FileView glassyFile: FileView {
        path: root.toggleDir + "/window-glassy.lua"
        printErrors: false
        onLoaded: root.hasGlassy = true
        onLoadFailed: root.hasGlassy = false
    }
    // Hyprland reloads its config for a Window Transparency change, with no theme switch.
    property Connections reloads: Connections {
        target: Theme
        function onConfigReloaded() { root.reread(); root.retag() }
        function onFleaWindowOpened() { root.startTagger() }
    }
    // Only inside Hyprland, as ui/Theme.qml's event hookup: a headless run has no compositor to ask.
    readonly property bool inHyprland: Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE") !== null
    // ui/GlassTagger.qml, made when Hyprland opens the first Flea window, so it costs nothing before the first rows.
    property QtObject tagger: null
    onModeChanged: root.retag()
    onUsesDefaultChanged: root.retag()

    // Sample input: 0.34 answers 87/255, 0.999 answers 1.
    function stepped(value) { return Math.round(value * 255) / 255 }

    // One shell.toml's value: -1 when it sets none, the clamped number, or 1 for what is not a number.
    function valueIn(values) {
        var v = values ? values[root.key] : undefined
        if (typeof v !== "string" || v.length === 0) return -1
        var n = Number(v)
        return isFinite(n) ? Util.clampAlpha(n) : 1
    }
    // User and theme values keep their precedence; bare Transparent is opaque: no shipped theme sets an opacity of its own, and no toggle file means transparency was never chosen.
    function wanted() {
        var user = root.valueIn(Commons.Color.userShellValues)
        if (user >= 0) return user
        if (root.mode === "opaque") return 1
        var theme = root.valueIn(Commons.Color.themeShellValues)
        if (theme >= 0) return theme
        return root.mode === "glassy" ? root.glassAlpha : 1
    }

    function reread() {
        root.opaqueFile.reload()
        root.glassyFile.reload()
        root.opaqueFile.waitForJob()
        root.glassyFile.waitForJob()
        root.mode = root.hasGlassy ? "glassy" : root.hasOpaque ? "opaque" : "transparent"
    }

    // A [flea] value that pins 1 keeps an opaque surface; every other launch is made clear, so each Window Transparency switch shows live.
    function pinsOpaque() {
        var user = root.valueIn(Commons.Color.userShellValues)
        if (user >= 0) return root.stepped(user) >= 1
        var theme = root.valueIn(Commons.Color.themeShellValues)
        return theme >= 0 && root.stepped(theme) >= 1
    }

    // Color's two shell readers load asynchronously; waiting here lands both before the decision.
    function launch() {
        Commons.Color.shellFile.waitForJob()
        Commons.Color.userShellFile.waitForJob()
        root.reread()
        root.clear = !root.pinsOpaque()
    }

    function startTagger() {
        if (!root.inHyprland) return
        if (!root.tagger) root.tagger = Qt.createComponent("GlassTagger.qml").createObject(root, {
            mode: Qt.binding(function () { return root.mode }),
            usesDefault: Qt.binding(function () { return root.usesDefault })
        })
        root.retag()
    }
    function retag() { if (root.tagger) root.tagger.retag() }
}
