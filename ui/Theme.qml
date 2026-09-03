pragma Singleton

import Quickshell
import Quickshell.Io
import QtQuick
import qs.Commons
import "js/Columns.js" as Columns
import "js/Contrast.js" as Contrast
import "js/Motion.js" as Motion
import "js/Palette.js" as Palette
import "js/Shadow.js" as ShadowMath

// Flea is its own process, so it plays the role shell.qml plays for the bar: it feeds Color and Style.
Singleton {
    id: root

    readonly property string stateDir: Quickshell.env("HOME") + "/.local/state/omarchy/current"

    // True only once colors.toml parsed to a palette, so a test can tell one from a fallback.
    property bool ready: false

    // Color models five roles; these three have no counterpart and stay Flea's own.
    readonly property var fallbackColor: ({
        surface: "#181825",
        symlink: "#94e2d5",
        executable: "#a6e3a1"
    })

    readonly property QtObject color: QtObject {
        readonly property color background: Color.background
        readonly property color foreground: Color.foreground
        property color muted: "#707880"
        readonly property color accent: Color.accent
        readonly property color error: Color.urgent
        property color surface: root.fallbackColor.surface
        property color symlink: root.fallbackColor.symlink
        property color executable: root.fallbackColor.executable
    }

    // The metrics gate and the canvas are written at omarchy base-size 14. This box is on 12, and
    // on a 4K panel at 1.5 that makes body text 11 logical px. Floor at 14 so Flea matches the
    // board; a larger omarchy display text size still wins.
    readonly property real sizeScale: Math.max(1, 14 / Math.max(1, Style.font.baseSize))

    readonly property QtObject font: QtObject {
        readonly property string family: Style.font.family
        readonly property int bodySmall: Math.round(Style.font.bodySmall * root.sizeScale)
        readonly property int caption: Math.round(Style.font.caption * root.sizeScale)
    }

    readonly property QtObject spacing: QtObject {
        readonly property int hairline: Style.spacing.hairline
        readonly property int rowPaddingX: Math.round(Style.spacing.rowPaddingX * root.sizeScale)
        readonly property int rowPaddingY: Math.round(Style.spacing.controlPaddingY * root.sizeScale)
        readonly property int gap: Math.round(Style.spacing.rowGap * root.sizeScale)
    }

    // One glyph's advance in a monospace face is every glyph's advance, so this sizes every fixed column.
    TextMetrics {
        id: glyphMetrics
        font.family: Style.font.family
        font.pixelSize: root.font.bodySmall
        text: "0"
    }

    // The header and every row read these, so the two cannot drift apart.
    readonly property QtObject column: QtObject {
        // mode is a permanent column per the operator's ruling; Row and Header both read this width.
        readonly property int mode: Math.round(root.modeChars * glyphMetrics.advanceWidth)
        readonly property int size: Math.round(root.sizeChars * glyphMetrics.advanceWidth)
        readonly property int date: Math.round(root.dateChars * glyphMetrics.advanceWidth)
        // Kind text varies too much for a character count, so its base is a pixel width scaled by the same ratio bodySmall already is.
        readonly property int kind: Math.round(root.kindBaseWidth * root.font.bodySmall / 12)
        // Not a column: the floor under the name, which the four above drop one by one to protect.
        readonly property int nameMin: Math.round(root.nameMinChars * glyphMetrics.advanceWidth)
    }

    // Row height follows the font so it scales with omarchy display text size.
    readonly property int rowHeight: Math.round(font.bodySmall * lineBoxRatio) + 2 * spacing.rowPaddingY
    // The icon slot is the row's text line box, so an icon can never change the row height.
    readonly property int iconSize: root.rowHeight - 2 * root.spacing.rowPaddingY
    // A mark is sized from the type scale, never from its slot: 19, the canvas's own M.mark, is the row and menu one.
    readonly property int markSize: Math.round(root.font.bodySmall * 1.45)
    // A mark standing alone takes its own step of the same scale: States.dc.html draws Locked and Error at 40.
    readonly property int stateMarkSize: Math.round(root.font.bodySmall * 3.1)
    // The brand moment, the empty hero and the loading crawl: 48, which is 6/5 of stateMarkSize on the same board.
    readonly property int heroMarkSize: Math.round(root.font.bodySmall * 3.7)
    // A chrome strip's mark is the OEM's own icon token, the one Ui/Button.qml and the Tailscale and
    // Dropbox bar icons size from: 16 at base-size 14, which is the canvas's chrome mark on every board.
    readonly property int chromeMarkSize: Math.round(Style.font.icon * root.sizeScale)
    // Lucide ships stroke 2 on its 24 unit grid; 1.5 is the operator's tune (Tabler ships 2 as well, see the A/B report).
    readonly property real strokeWidth: 1.5
    // WCAG 2.5.8 floor. Marks stay at their type-scale size; the hit box grows to this.
    readonly property int hitMin: 24
    // Wide enough for "Send with Taildrop" at bodySmall, 257 at base-size 14; ui/ContextMenu.qml draws it.
    readonly property int menuWidth: Math.round(Style.space(220) * root.sizeScale)

    // The canvas draws box-shadow: 0 16px 40px rgba(0,0,0,0.5) under every floating in-app surface
    // and nothing in ui/ ever drew one. The offset and the blur are the canvas's own design pixels
    // put through Style.space, which lands them on exactly 16 and 40 at this box's base-size 14
    // (14 x 14/12 rounds to 16, 34 x 14/12 rounds to 40) and scales both with the operator's text size.
    readonly property QtObject shadow: QtObject {
        id: shadowTokens

        readonly property int offset: Style.space(14)
        // CSS puts half a blur inside the shape's edge and half outside, so a ring grows by half of it.
        readonly property int spread: Math.round(Style.space(34) / 2)
        // Stacked rings stand in for the Gaussian. Three is where the falloff stops reading as bands
        // on a dark ground, and it is the one number to turn if it ever does.
        readonly property int steps: 3
        // The canvas's own alpha, which is the shadow's peak rather than what any pixel outside the
        // card shows: a Gaussian sits at half its peak on the shape's own edge, and every value
        // darker than that falls behind the opaque surface, so half is what the visible band reaches.
        readonly property real strength: 0.5
        readonly property real edgeAlpha: shadowTokens.strength / 2
        // A shadow is light the surface blocked rather than a role in the palette, so this is the one
        // colour in this file that multiplies the ground instead of naming it, which is what keeps it
        // right on a light theme as well as on this dark one.
        readonly property color color: Qt.rgba(0, 0, 0, ShadowMath.stepAlpha(shadowTokens.edgeAlpha, shadowTokens.steps))
    }

    // Leading a row gives its text, above and below, before the padding is added.
    readonly property real lineBoxRatio: 1.8

    // Operator's density pass: a sidebar rail reads denser than the list it sits beside in every
    // reference (qui's rail rows sit around 0.85 of its list row, Finder's around 0.75), so the
    // rail gets its own row height and icon slot instead of borrowing the list's directly.
    readonly property real railRowRatio: 0.78
    readonly property int railRowHeight: Math.round(root.rowHeight * root.railRowRatio)
    readonly property int railIconSize: root.railRowHeight - 2 * root.spacing.rowPaddingY

    // The header and status bar are thin chrome strips, not data rows (Finder's own header sits
    // well under its row height); this keeps both denser than a list row without a pixel constant.
    readonly property real chromeRowRatio: 0.72
    readonly property int chromeHeight: Math.round(root.rowHeight * root.chromeRowRatio)
    // "rwxrwxrwx", Format.permissions is always exactly this wide.
    readonly property int modeChars: 9
    // "1000.0 kB": the SI ladder's tier-boundary rounding is one char wider than "999.9 kB".
    readonly property int sizeChars: 9
    // "Yesterday, 23:16", the widest of Format.date's four forms.
    readonly property int dateChars: 16
    // 120px at the OEM's base font size of 12, the Kind column's own anchor, see column.kind above.
    readonly property int kindBaseWidth: 120
    // The name's floor, in the character unit the fixed columns are already written in. Twenty
    // characters holds 84.5% of a 25,473-name sample of /usr/bin, /usr/include,
    // /usr/share/applications and this repo whole, and every further two buys under five points.
    readonly property int nameMinChars: 20

    // The grid view's own two numbers. The canvas calls it a "48 px slot"; twice the list's own mark
    // slot is 46 at base-size 14, and the token wins over the mock, see the icon-language spec.
    readonly property QtObject grid: QtObject {
        readonly property int iconSize: root.iconSize * 2
        // Wide enough for a name of ordinary length under the mark; the view fits as many as this allows.
        readonly property int minCellWidth: root.space(150)
    }

    readonly property QtObject preview: QtObject {
        // Quick-Look sized inset over the window, not a second window.
        readonly property real fraction: 0.82
    }

    // The column set a list of this width can draw. ui/Header.qml and ui/Row.qml each call this
    // with their own width, which anchoring keeps equal, so the header and the rows below it
    // cannot disagree about which columns exist.
    function columns(width) {
        return Columns.set(width, {
            rowPaddingX: root.spacing.rowPaddingX,
            gap: root.spacing.gap,
            iconSize: root.iconSize,
            nameMin: root.column.nameMin,
            mode: root.column.mode,
            size: root.column.size,
            date: root.column.date,
            kind: root.column.kind
        });
    }

    // The same set as one string, which is what the seam in ui/Ipc.qml compares across the two.
    function columnNames(width) {
        return Columns.names(root.columns(width));
    }

    // Five callers: ConvertDialog, KeymapSheet, NetworkDialog, NetworkForm, TransferCard; every other spacing token above is direct.
    function space(px) {
        return Math.round(Style.space(px) * root.sizeScale);
    }

    // The metrics contract as the app resolves it, one key=value per line in the Blueprint board's
    // order; ui/shell.qml serves it as tokens() and tools/flea-metrics-gate diffs it. family is the
    // resolved face, never the "monospace" alias, so the gate cannot pass on a box without the font.
    function tokens() {
        var t = {
            family: Style.font.resolvedFamily,
            baseSize: Style.font.baseSize,
            bodySmall: root.font.bodySmall,
            caption: root.font.caption,
            lineBoxRatio: root.lineBoxRatio,
            rowPaddingX: root.spacing.rowPaddingX,
            rowPaddingY: root.spacing.rowPaddingY,
            gap: root.spacing.gap,
            hairline: root.spacing.hairline,
            rowHeight: root.rowHeight,
            iconSize: root.iconSize,
            markSize: root.markSize,
            stateMarkSize: root.stateMarkSize,
            heroMarkSize: root.heroMarkSize,
            strokeWidth: root.strokeWidth,
            railRowHeight: root.railRowHeight,
            railIconSize: root.railIconSize,
            chromeHeight: root.chromeHeight,
            chromeMarkSize: root.chromeMarkSize,
            columnMode: root.column.mode,
            columnSize: root.column.size,
            columnDate: root.column.date,
            columnKind: root.column.kind,
            menuWidth: root.menuWidth,
            cornerRadius: Style.cornerRadius,
            shadowOffset: root.shadow.offset,
            shadowSpread: root.shadow.spread,
            shadowSteps: root.shadow.steps,
            shadowStrength: root.shadow.strength,
            shadowEdgeAlpha: root.shadow.edgeAlpha,
            previewFraction: root.preview.fraction,
            gridIconSize: root.grid.iconSize,
            gridMinCellWidth: root.grid.minCellWidth
        };
        var lines = [];
        for (var key in t)
            lines.push(key + "=" + t[key]);
        return lines.join("\n");
    }

    // Color owns the other five; only the three it does not model are assigned here.
    function applyColors(body) {
        var found = Palette.parse(body);
        var bg = Palette.pick(found, ["background"], "#101315");
        var surface = Palette.pick(found, ["dark_background", "selection"], root.fallbackColor.surface);
        // corner: the alacritty-derived colors.toml emits neither background ladder key, so selection is third.
        root.color.surface = surface;
        // Omarchy palettes are not authored to AA. Flea keeps the hex system and walks L until 4.5:1.
        root.color.muted = Contrast.ensureRatio(
            Contrast.ensureRatio(Palette.pick(found, ["muted"], "#707880"), bg, 4.5), surface, 4.5);
        root.color.symlink = Contrast.ensureRatio(
            Palette.pick(found, ["cyan", "color6"], root.fallbackColor.symlink), bg, 4.5);
        root.color.executable = Contrast.ensureRatio(
            Palette.pick(found, ["green", "color2"], root.fallbackColor.executable), bg, 4.5);
        Color.loadColors(body);
        // A body that parsed to nothing left every role on its fallback, so the flag says so rather
        // than reporting that the read happened: text() returns "" for a file that is not there.
        root.ready = Palette.isPalette(found);
    }

    function applyReducedMotion() {
        var env = Quickshell.env("FLEA_REDUCED_MOTION");
        if (env === "1" || env === "true") {
            Motion.reduced = true;
            return;
        }
        if (env === "0" || env === "false") {
            Motion.reduced = false;
            return;
        }
        motionQuery.running = true;
    }

    // blockLoading only gates calls to text()/data(); nothing forced that call before this fix,
    // so a window could paint one frame against qs.Commons Color's own un-loaded fallback (blue)
    // before onLoaded ever fired. Component.onCompleted calls text() itself, which blocks the
    // Singleton's own construction, which runs before any window: colors.toml is applied before
    // the first frame, and the later onLoaded is a harmless second, idempotent apply.
    FileView {
        id: colorsFile
        path: root.stateDir + "/theme/colors.toml"
        blockLoading: true
        printErrors: false
        onLoaded: root.applyColors(text())
        onLoadFailed: root.ready = false
        Component.onCompleted: root.applyColors(colorsFile.text())
    }

    // Color.loadShell refreshes Style's whole token scale, so the type ladder flips with the theme.
    FileView {
        id: shellFile
        path: root.stateDir + "/theme/shell.toml"
        blockLoading: true
        printErrors: false
        onLoaded: Color.loadShell(text())
        onLoadFailed: Color.loadShell("")
    }

    // omarchy-theme-set rm -rf's and mv's the theme directory, so an inotify watch on a file inside
    // it dies with the old inode and never fires again. theme.name is rewritten in place after the
    // swap, which makes it the one event that survives, measured across three consecutive switches.
    FileView {
        id: themeNameFile
        path: root.stateDir + "/theme.name"
        blockLoading: true
        watchChanges: true
        printErrors: false
        onFileChanged: {
            reload();
            colorsFile.reload();
            shellFile.reload();
            // The OEM shell's applyTheme runs this beside the two reloads above, and without it a theme that moves decoration:rounding leaves every corner here on the old value.
            Style.scheduleRefresh();
        }
    }

    Process {
        id: motionQuery
        command: ["gsettings", "get", "org.gnome.desktop.a11y.interface", "reduced-motion"]
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: {
                var body = String(text)
                Motion.reduced = body.indexOf("reduce") >= 0 && body.indexOf("no-preference") < 0
            }
        }
    }

    Component.onCompleted: root.applyReducedMotion()
}
