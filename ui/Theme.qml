pragma Singleton

import Quickshell
import Quickshell.Io
import QtQuick
import qs.Commons
import qs.Commons as Omarchy
import "js/Buttons.js" as Buttons
import "js/Columns.js" as Columns
import "js/Contrast.js" as Contrast
import "js/Density.js" as Density
import "js/Palette.js" as Palette
import "js/TextSize.js" as TextSize

// Flea is its own process, so it plays the role shell.qml plays for the bar: it feeds Color and Style.
Singleton {
    id: root

    readonly property string stateDir: Quickshell.env("HOME") + "/.local/state/omarchy/current"

    // True only once colors.toml parsed to a palette, so a test can tell one from a fallback.
    property bool ready: false

    // The size Flea draws at: Omarchy's own unless the Display section pinned an override stop.
    readonly property int baseSize: TextSize.effective(ViewState.textSize, Style.font.baseSize)
    readonly property bool overridden: !TextSize.following(ViewState.textSize)
    // One while following, so every OEM token below is Omarchy's own until an override moves it,
    // and then moves by the ratio between the pinned stop and Omarchy's size.
    readonly property real sizeRatio: Style.font.baseSize > 0 ? root.baseSize / Style.font.baseSize : 1

    // The compositor's own monitor scale, which the Display section shows and never steps; 0 until
    // hyprctl has answered, and the panel says so rather than claiming a number it does not have.
    property real monitorScale: 0

    // A property and not a Motion.js var: a plain library var notifies nothing, so every Behavior
    // reading it would keep whatever it was built with when the compositor's answer arrives.
    property bool reducedMotion: Quickshell.env("FLEA_REDUCED_MOTION") === "1"

    // The only literal colours in the UI. Color models five roles; surface, symlink and executable
    // have no counterpart, and the other two are read here before Omarchy.Color.loadColors has run.
    readonly property var fallbackColor: ({
        background: "#101315",
        surface: "#181825",
        symlink: "#94e2d5",
        executable: "#a6e3a1"
    })

    // A palette role at or under this HSV saturation carries no colour (urgent falls back to the foreground, a grey accent cannot mark focus).
    readonly property real hueFloor: 0.2

    readonly property QtObject color: QtObject {
        readonly property color background: Omarchy.Color.background
        readonly property color foreground: Omarchy.Color.foreground
        // The heading ink: a brighter step of the foreground, applyColors derives it from the palette.
        property color foregroundBright: Omarchy.Color.foreground
        property color muted: Qt.darker(Omarchy.Color.foreground, 1.4)
        readonly property color accent: Omarchy.Color.accent
        property color error: Omarchy.Color.urgent
        // Error ink on the status bar's own surface, lifted there the way error is on the background.
        property color errorOnSurface: Omarchy.Color.urgent
        property color surface: root.fallbackColor.surface
        property color symlink: root.fallbackColor.symlink
        property color executable: root.fallbackColor.executable
        // The accent as a frame rather than as ink: on a card's own surface a frame is a graphical
        // object, so it is lifted to 3:1 there the way symlink and executable are lifted on the list.
        property color accentFrame: Omarchy.Color.accent
        // True where the accent carries colour, so it can mark focus apart from the foreground and muted frames; kanagawa, solitude, vantablack and white do not.
        property bool accentHasHue: Omarchy.Color.accent.hsvSaturation > root.hueFloor
    }

    readonly property QtObject font: QtObject {
        readonly property string family: Style.font.family
        // Following takes Omarchy's resolved token, so a theme's own font override still wins. An
        // override runs Style's own fontPx ratios at the pinned stop, which is the same ladder.
        // Running text draws at Omarchy's regular body (GM, 2026-09-07); bodySmall stays the geometry token every row and mark is sized from.
        readonly property int body: root.overridden ? TextSize.body(root.baseSize) : Style.font.body
        readonly property int bodySmall: root.overridden ? TextSize.bodySmall(root.baseSize) : Style.font.bodySmall
        readonly property int caption: root.overridden ? TextSize.caption(root.baseSize) : Style.font.caption
    }

    // Tight drops the padding through Density040, Compact stays the default.
    readonly property real densityRatio: Density.ratioFor(ViewState.density)

    readonly property QtObject spacing: QtObject {
        readonly property int hairline: Style.spacing.hairline
        readonly property int rowPaddingX: Math.round(Style.spacing.rowPaddingX * root.sizeRatio)
        readonly property int rowPaddingY: Math.round(Style.spacing.controlPaddingY * root.sizeRatio)
        readonly property int gap: Math.round(Style.spacing.rowGap * root.sizeRatio)
    }

    // One glyph's advance in a monospace face is every glyph's advance, so this sizes every fixed column; row names budget off bodyAdvance, grid captions off bodySmallAdvance.
    readonly property real glyphAdvance: glyphMetrics.advanceWidth
    // The body-face advance the row names draw at; Row and ColumnRow budget off this, never the caption one.
    readonly property real bodyAdvance: bodyGlyphMetrics.advanceWidth
    TextMetrics {
        id: glyphMetrics
        font.family: Style.font.family
        font.pixelSize: root.font.bodySmall
        text: "0"
    }

    // Trash draws its Deleted cell at body, and body/bodySmall is not one ratio across the size stops.
    TextMetrics { id: bodyGlyphMetrics; font.family: Style.font.family; font.pixelSize: root.font.body; text: "0" }
    readonly property real bodySmallAdvance: root.glyphAdvance // Same inputs as glyphMetrics, so one object serves both; grid tile budgets off this, never the body one.

    // The header and every row read the stored widths so the two cannot drift apart; a drag previews in the header alone, ListColumns040 callout 1.
    // Sample input: {"mode": "wide"} answers the measured width, {"size": -40} the same.
    function storedWidth(key, fallback) {
        var raw = ViewState.state.columnWidths ? ViewState.state.columnWidths[key] : undefined
        var n = Columns.storedNumber(raw)
        if (!(n >= 0))
            return fallback
        return Columns.clampListWidth(n)
    }
    readonly property QtObject column: QtObject {
        // mode is a permanent column per the operator's ruling; Row and Header both read this width.
        readonly property int mode: root.storedWidth("mode", Math.round(root.modeChars * glyphMetrics.advanceWidth))
        readonly property int size: root.storedWidth("size", Math.round(root.sizeChars * glyphMetrics.advanceWidth))
        readonly property int date: root.storedWidth("date", Math.round(root.dateChars * glyphMetrics.advanceWidth))
        // Trash's Deleted column: the same sixteen characters in the size that draws them, ceil because a rounded width elides at base 14.
        readonly property int trashDate: Math.ceil(root.dateChars * bodyGlyphMetrics.advanceWidth)
        // The send picker's own, anchored the way kind below it is rather than counted in characters.
        readonly property int pickerDate: Math.round(root.pickerDateBaseWidth * root.font.bodySmall / root.pickerDateBaseBodySmall)
        // Kind text varies too much for a character count, so its base is a pixel width scaled by the same ratio bodySmall already is.
        readonly property int kind: root.storedWidth("kind", Math.round(root.kindBaseWidth * root.font.bodySmall / 12))
        readonly property int location: 150 // Sidebar040's fixed Recent column, shared by Header and Row.
        // Not a column: the floor under the name, which the four above drop one by one to protect.
        readonly property int nameMin: Math.round(root.nameMinChars * glyphMetrics.advanceWidth)
    }

    // Row height follows the font so it scales with omarchy display text size.
    readonly property int rowHeight: Math.round(font.bodySmall * lineBoxRatio) + 2 * spacing.rowPaddingY
    readonly property int fileRowHeight: Density.rowHeight(Math.round(font.bodySmall * lineBoxRatio), spacing.rowPaddingY, ViewState.density)
    // The icon slot is the row's text line box, so an icon can never change the row height.
    readonly property int iconSize: root.rowHeight - 2 * root.spacing.rowPaddingY
    // A mark is sized from the type scale, never from its slot: 19, the canvas's own M.mark, is the row and menu one.
    readonly property real markSize: root.font.bodySmall * 1.45
    // A mark standing alone takes its own step of the same scale: States.dc.html draws Locked and Error at 40.
    readonly property int stateMarkSize: Math.round(root.font.bodySmall * 3.1)
    // The brand moment, the empty hero and the loading crawl: 48, which is 6/5 of stateMarkSize on the same board.
    readonly property int heroMarkSize: Math.round(root.font.bodySmall * 3.7)
    // A chrome strip's mark is the OEM's own icon token, the one Ui/Button.qml and the Tailscale and
    // Dropbox bar icons size from: 16 at base-size 14, which is the canvas's chrome mark on every board.
    readonly property int chromeMarkSize: Math.round(Style.font.icon * root.sizeRatio)
    // The boards use one two-unit stroke on the shared 24-unit glyph grid.
    readonly property real strokeWidth: 2
    // WCAG 2.5.8 floor. Marks stay at their type-scale size; the hit box grows to this.
    readonly property int hitMin: 24
    // The wheel, see ui/FastScrollHandler.qml: a notch is the platform's lines times notchPx times the
    // multiplier (PR 16's pair, 4x on Omarchy Spotify); touchpad pixels move times TOUCH_GAIN (GM 2026-10-01).
    readonly property QtObject scroll: QtObject {
        readonly property int notchPx: 24
        readonly property real multiplier: 4
    }
    // Wide enough for "Send with Taildrop" at bodySmall, 257 at base-size 14; ui/ContextMenu.qml draws it.
    readonly property int menuWidth: Math.round(Style.space(220) * root.sizeRatio)

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
    // A chrome control's frame is inset from its strip on every side, and the strip carries its own
    // rule along the bottom edge, so the inset has to clear that rule too: two lines that touch read
    // as one line whatever colour they are. A quarter of the caption keeps the margin visible at
    // every text size instead of pinning it to a pixel, and never below twice the rule's own width.
    readonly property int chromeControlInset: Math.max(2 * root.spacing.hairline, Math.round(root.font.caption / 4))
    // What is left of the strip once its rule and both insets are taken off. The hit box stays the
    // whole strip, so a press still clears hitMin however small the frame gets.
    readonly property int chromeControlHeight: root.chromeHeight - root.spacing.hairline - 2 * root.chromeControlInset
    // The room a clip leaves a button's 2 px ring, drawn outside its frame, so the ring ends one hairline inside the clip.
    readonly property int ringClearance: Buttons.RING + root.spacing.hairline
    // The two steps of that fill, in whichever role the control carries: the pointer's and the
    // keyboard's. The second is the weight the rail's own active row and the segmented chooser's
    // active segment already take, so a control under the keyboard reads at the same strength.
    readonly property real washHover: 0.08
    readonly property real washActive: 0.14
    readonly property real disabledOpacity: 0.55
    // An accent edge is 2px along a tall side and 3px flush along a short one, measured in Quickshell.
    readonly property int accentEdge: 3
    // "rwxrwxrwx", Format.permissions is always exactly this wide.
    readonly property int modeChars: 9
    // "1000.0 kB": the SI ladder's tier-boundary rounding is one char wider than "999.9 kB".
    readonly property int sizeChars: 9
    // "2026-09-12 15:29", the one form Format.date prints.
    readonly property int dateChars: 16
    // SendPicker.html draws the chooser's date in an 80px slot, on a board whose base size is 14 and whose bodySmall is therefore 13.
    readonly property int pickerDateBaseWidth: 80
    readonly property int pickerDateBaseBodySmall: 13
    // 120px at the OEM's base font size of 12, the Kind column's own anchor, see column.kind above.
    readonly property int kindBaseWidth: 120
    // The name's floor, in the character unit the fixed columns are already written in. Twenty
    // characters holds 84.5% of a 25,473-name sample of /usr/bin, /usr/include,
    // /usr/share/applications and this repo whole, and every further two buys under five points.
    readonly property int nameMinChars: 20

    // DualPane specifies these slots at bodySmall 13; mark, gap and padding remain shared tokens.
    readonly property QtObject dualColumn: QtObject {
        readonly property real size: 70 * root.font.bodySmall / 13
        readonly property real date: 125 * root.font.bodySmall / 13
        readonly property real nameMin: 180 * root.font.bodySmall / 13
    }

    function dualColumns(width, hidden) {
        return Columns.dualSet(width, {rowPaddingX: root.spacing.rowPaddingX, gap: root.spacing.gap,
            iconSize: root.markSize, nameMin: root.dualColumn.nameMin,
            size: root.dualColumn.size, date: root.dualColumn.date}, hidden)
    }

    // The grid view's own two numbers. The canvas calls it a "48 px slot"; twice the list's own mark
    // slot is 46 at base-size 14, and the token wins over the mock, see the icon-language spec.
    readonly property QtObject grid: QtObject {
        readonly property int iconSize: root.iconSize * 2
        // GridView.html reserves two caption lines, each 15px at the 11px caption token.
        readonly property real captionLineHeight: root.font.caption * 15 / 11
        readonly property real captionHeight: 2 * captionLineHeight
        // GridView.dc.html's own five-column reference viewport: 880 body less 2x40 board padding,
        // 2x1 window hairline, 2x18 grid padding and 4x8 gap, over five tiles, is 146 at base-size 14.
        readonly property int minCellWidth: root.space(125)
    }

    // GM's September 8 sizing override gives dialog cards more room without enlarging context menus.
    readonly property real dialogWidthRatio: 9 / 8
    // A card's whole size inside its room and a whole centred origin, so no hairline of its frame lands on a half pixel.
    function cardSpan(want, room) { return Math.max(0, Math.min(Math.ceil(want), Math.floor(room))) }
    function cardOrigin(room, span) { return Math.round((room - span) / 2) }

    // The Settings board's anatomy, resolved at base-size 14: a border-box panel 560 wide whose two
    // outer hairlines leave 558 inside, split into a 150 rail and a 408 pane. 480 and 350 are those
    // two at the OEM's own 12 anchor, so the pair scales once and the rail is what is left over.
    readonly property QtObject settings: QtObject {
        readonly property int panelWidth: Math.round(root.space(480) * root.dialogWidthRatio)
        readonly property int paneWidth: Math.round(root.space(350) * root.dialogWidthRatio)
        readonly property int railWidth: root.settings.panelWidth - root.settings.paneWidth
                                         - 2 * root.spacing.hairline
        // Settings.dc.html insets the rail column by 10 above its first row and below its last, in resolved pixels at base-size 14, whose bodySmall is 13, so space() would scale it twice.
        readonly property int railPaddingY: Math.round(10 * root.font.bodySmall / 13)
    }

    readonly property QtObject preview: QtObject {
        // Quick-Look sized inset over the window, not a second window.
        readonly property real fraction: 0.82
    }

    // The columns a list of this width can draw, less the ones ViewState hides; Header and Row call
    // it with their own anchored-equal width, so they cannot disagree, and dateWidth is the picker's.
    readonly property var columnTokens: ({
        rowPaddingX: root.spacing.rowPaddingX, gap: root.spacing.gap, iconSize: root.iconSize,
        nameMin: root.column.nameMin, mode: root.column.mode,
        size: root.column.size, date: root.column.date, kind: root.column.kind, location: root.column.location
    })
    function columnSet(dateWidth, dual) {
        if (dual)
            return Object.assign({}, root.columnTokens, {iconSize: root.markSize,
                nameMin: root.dualColumn.nameMin, size: root.dualColumn.size,
                date: dateWidth === undefined ? root.dualColumn.date : dateWidth});
        return dateWidth === undefined ? root.columnTokens : Object.assign({}, root.columnTokens, {date: dateWidth});
    }
    function columns(width, hidden, dateWidth, recent, dual) { return recent ? Columns.recentSet(width, root.columnSet(dateWidth, dual), hidden, dual) : Columns.set(width, root.columnSet(dateWidth), hidden); }

    // Five callers plus the grid and settings tokens above: ConvertDialog, KeymapSheet, NetworkDialog, NetworkForm, TransferCard; every other spacing token is direct.
    function space(px) {
        return Math.round(Style.space(px) * root.sizeRatio);
    }

    // The metrics contract as the app resolves it, one key=value per line in the Blueprint board's
    // order; ui/shell.qml serves it as tokens() and tools/flea-metrics-gate diffs it. family is the
    // resolved face, never the "monospace" alias, so the gate cannot pass on a box without the font.
    function tokens() {
        var t = {
            family: Style.font.resolvedFamily,
            baseSize: root.baseSize,
            body: root.font.body,
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
            columnPickerDate: root.column.pickerDate,
            columnKind: root.column.kind,
            menuWidth: root.menuWidth,
            cornerRadius: Style.cornerRadius,
            previewFraction: root.preview.fraction,
            gridIconSize: root.grid.iconSize,
            gridMinCellWidth: root.grid.minCellWidth,
            settingsPanelWidth: root.settings.panelWidth,
            settingsRailWidth: root.settings.railWidth,
            settingsPaneWidth: root.settings.paneWidth,
            settingsRailPaddingY: root.settings.railPaddingY
        };
        var lines = [];
        for (var key in t)
            lines.push(key + "=" + t[key]);
        return lines.join("\n");
    }

    // Color owns the other five; only the three it does not model are assigned here.
    function applyColors(body) {
        var found = Palette.parse(body);
        var bg = Palette.pick(found, ["background"], root.fallbackColor.background);
        var surface = Palette.pick(found, Palette.SURFACE_KEYS, root.fallbackColor.surface);
        root.color.surface = surface;
        Omarchy.Color.loadColors(body);
        // Measured over the 22 stock palettes in tests/js/themes.js: 20 set a muted under the 3:1 a
        // caption needs, rose-pine's at 1.48, so it is lifted the way the two ladder colours below are.
        root.color.muted = Contrast.ensureRatio(
            Palette.pick(found, ["muted"], Qt.darker(Omarchy.Color.foreground, 1.4)), bg, 3);
        // Headings take the palette's bright foreground (the ANSI ring's color15 without one) when it has more contrast than the foreground.
        var bright = Palette.pick(found, ["bright_foreground", "color15"], String(Omarchy.Color.foreground));
        root.color.foregroundBright = Contrast.ratio(bright, bg) > Contrast.ratio(String(Omarchy.Color.foreground), bg) ? bright : Omarchy.Color.foreground;
        root.color.accentFrame = Contrast.ensureRatio(Omarchy.Color.accent, surface, 3);
        root.color.accentHasHue = Omarchy.Color.accent.hsvSaturation > root.hueFloor;
        root.color.symlink = Contrast.ensureRatio(
            Palette.pick(found, ["cyan", "color6"], root.fallbackColor.symlink), bg, 4.5);
        root.color.executable = Contrast.ensureRatio(
            Palette.pick(found, ["green", "color2"], root.fallbackColor.executable), bg, 4.5);
        // Urgent is the palette's own red: seven of the 23 installed themes leave it under 4.5:1 on their own ground, so it is lifted the way symlink and executable are, and the three whose red carries no chroma at all (solitude, white, vantablack) fall back to the foreground, because a destructive row drawn in the same grey as an unavailable one reads as switched off rather than as dangerous.
        root.color.error = Omarchy.Color.urgent.hsvSaturation > root.hueFloor ? Contrast.ensureRatio(Omarchy.Color.urgent, bg, 4.5) : String(Omarchy.Color.foreground);
        // The status bar draws that same ink on the surface, where four themes land under 4.5.
        root.color.errorOnSurface = Omarchy.Color.urgent.hsvSaturation > root.hueFloor ? Contrast.ensureRatio(Omarchy.Color.urgent, surface, 4.5) : String(Omarchy.Color.foreground);
        // A body that parsed to nothing left every role on its fallback, so the flag says so rather
        // than reporting that the read happened: text() returns "" for a file that is not there.
        root.ready = Palette.isPalette(found);
    }

    // Sample input: [{"id":0,"name":"DP-1","width":2560,"height":1440,"scale":1.00,"focused":true}]
    function applyMonitorScale(body) {
        try {
            var monitors = JSON.parse(body);
            for (var i = 0; i < monitors.length; i++) {
                if (monitors[i].focused === true)
                    root.monitorScale = monitors[i].scale;
            }
        } catch (e) {
            // hyprctl unreachable, or a shape this build does not know: the row keeps saying so.
        }
    }

    // Sample input: {"option": "animations:enabled", "bool": false, "set": true }
    function applyReducedMotion(body) {
        root.reducedMotion = String(body).indexOf('"bool": false') >= 0;
    }

    // blockLoading only gates calls to text()/data(), so a window could paint one frame against the
    // un-loaded fallback; Component.onCompleted calls text() itself, which blocks this Singleton's
    // construction before any window, and the later onLoaded is a harmless idempotent second apply.
    FileView {
        id: colorsFile
        path: root.stateDir + "/theme/colors.toml"
        blockLoading: true
        printErrors: false
        onLoaded: root.applyColors(text())
        onLoadFailed: root.ready = false
        Component.onCompleted: root.applyColors(colorsFile.text())
    }

    // Omarchy.Color.loadShell refreshes Style's whole token scale, so the type ladder flips with the theme.
    FileView {
        id: shellFile
        path: root.stateDir + "/theme/shell.toml"
        blockLoading: true
        printErrors: false
        onLoaded: Omarchy.Color.loadShell(text())
        onLoadFailed: Omarchy.Color.loadShell("")
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

    // Read once, not watched: the Display section reports the compositor's scale and Flea owns no
    // control that could change it, so there is nothing here for a poll to keep in step with.
    Process {
        running: true
        command: ["hyprctl", "monitors", "-j"]
        stdout: StdioCollector {
            waitForEnd: true
            // text is a property on this type and not a function, which every other collector in
            // this tree already reads that way; calling it throws and the row stays unanswered.
            onStreamFinished: root.applyMonitorScale(text)
        }
    }

    // Flea agrees with the compositor rather than carrying a switch, and FLEA_REDUCED_MOTION is the
    // test override; Quickshell.env answers null for an unset variable, so the guard is a truthiness
    // test, StdioCollector text is a property whose call throws, and the query is Style.qml's own.
    Process {
        id: motionQuery
        running: !Quickshell.env("FLEA_REDUCED_MOTION")
        command: ["hyprctl", "-j", "getoption", "animations:enabled"]
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: root.applyReducedMotion(text)
        }
    }
}
