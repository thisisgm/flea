pragma Singleton

import Quickshell
import Quickshell.Io
import QtQuick
import qs.Commons
import "js/Columns.js" as Columns
import "js/Contrast.js" as Contrast
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
    // have no counterpart, and the other two are read here before Color.loadColors has run.
    readonly property var fallbackColor: ({
        background: "#101315",
        surface: "#181825",
        muted: "#707880",
        symlink: "#94e2d5",
        executable: "#a6e3a1"
    })

    readonly property QtObject color: QtObject {
        readonly property color background: Color.background
        readonly property color foreground: Color.foreground
        property color muted: root.fallbackColor.muted
        readonly property color accent: Color.accent
        readonly property color error: Color.urgent
        property color surface: root.fallbackColor.surface
        property color symlink: root.fallbackColor.symlink
        property color executable: root.fallbackColor.executable
    }

    readonly property QtObject font: QtObject {
        readonly property string family: Style.font.family
        // Following takes Omarchy's resolved token, so a theme's own font override still wins. An
        // override runs Style's own fontPx ratios at the pinned stop, which is the same ladder.
        readonly property int bodySmall: root.overridden ? TextSize.bodySmall(root.baseSize) : Style.font.bodySmall
        readonly property int caption: root.overridden ? TextSize.caption(root.baseSize) : Style.font.caption
    }

    readonly property QtObject spacing: QtObject {
        readonly property int hairline: Style.spacing.hairline
        readonly property int rowPaddingX: Math.round(Style.spacing.rowPaddingX * root.sizeRatio)
        readonly property int rowPaddingY: Math.round(Style.spacing.controlPaddingY * root.sizeRatio)
        readonly property int gap: Math.round(Style.spacing.rowGap * root.sizeRatio)
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
        // The send picker's own, anchored the way kind below it is rather than counted in characters.
        readonly property int pickerDate: Math.round(root.pickerDateBaseWidth * root.font.bodySmall / root.pickerDateBaseBodySmall)
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
    readonly property int chromeMarkSize: Math.round(Style.font.icon * root.sizeRatio)
    // Lucide ships stroke 2 on its 24 unit grid; 1.5 is the operator's tune (Tabler ships 2 as well, see the A/B report).
    readonly property real strokeWidth: 1.5
    // WCAG 2.5.8 floor. Marks stay at their type-scale size; the hit box grows to this.
    readonly property int hitMin: 24
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
    // "rwxrwxrwx", Format.permissions is always exactly this wide.
    readonly property int modeChars: 9
    // "1000.0 kB": the SI ladder's tier-boundary rounding is one char wider than "999.9 kB".
    readonly property int sizeChars: 9
    // "Yesterday, 23:16", the widest of Format.date's four forms.
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

    // The grid view's own two numbers. The canvas calls it a "48 px slot"; twice the list's own mark
    // slot is 46 at base-size 14, and the token wins over the mock, see the icon-language spec.
    readonly property QtObject grid: QtObject {
        readonly property int iconSize: root.iconSize * 2
        // GridView.dc.html's own five-column reference viewport: 880 body less 2x40 board padding,
        // 2x1 window hairline, 2x18 grid padding and 4x8 gap, over five tiles, is 146 at base-size 14.
        readonly property int minCellWidth: root.space(125)
    }

    // The Settings board's anatomy, resolved at base-size 14: a border-box panel 560 wide whose two
    // outer hairlines leave 558 inside, split into a 150 rail and a 408 pane. 480 and 350 are those
    // two at the OEM's own 12 anchor, so the pair scales once and the rail is what is left over.
    readonly property QtObject settings: QtObject {
        readonly property int panelWidth: root.space(480)
        readonly property int paneWidth: root.space(350)
        readonly property int railWidth: root.settings.panelWidth - root.settings.paneWidth
                                         - 2 * root.spacing.hairline
        // A row's continuation line, its hint and the Display ruler, indents 52 on five settings boards; those are resolved pixels at base-size 14, whose bodySmall is 13, so space() would scale them twice.
        readonly property int indent: Math.round(52 * root.font.bodySmall / 13)
        // Settings.dc.html insets the rail column by 10 above its first row and below its last, on that same board.
        readonly property int railPaddingY: Math.round(10 * root.font.bodySmall / 13)
    }

    readonly property QtObject preview: QtObject {
        // Quick-Look sized inset over the window, not a second window.
        readonly property real fraction: 0.82
    }

    // The column set a list of this width can draw, less the columns the user has hidden (qs
    // module ViewState). ui/Header.qml and ui/Row.qml each call this with their own width, which
    // anchoring keeps equal, so the header and the rows below it cannot disagree about which
    // columns exist.
    // dateWidth lets the picker afford the date at column.pickerDate, the width it actually draws.
    function columns(width, hidden, dateWidth) {
        return Columns.set(width, {
            rowPaddingX: root.spacing.rowPaddingX,
            gap: root.spacing.gap,
            iconSize: root.iconSize,
            nameMin: root.column.nameMin,
            mode: root.column.mode,
            size: root.column.size,
            date: dateWidth === undefined ? root.column.date : dateWidth,
            kind: root.column.kind
        }, hidden);
    }

    // The same set as one string, which is what the seam in ui/Ipc.qml compares across the two.
    function columnNames(width, hidden, dateWidth) {
        return Columns.names(root.columns(width, hidden, dateWidth));
    }

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
            settingsIndent: root.settings.indent,
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
        // Omarchy palettes are not authored to AA. Flea keeps the hex system and walks L until 4.5:1.
        root.color.muted = Contrast.ensureRatio(
            Contrast.ensureRatio(Palette.pick(found, ["muted"], root.fallbackColor.muted), bg, 4.5), surface, 4.5);
        root.color.symlink = Contrast.ensureRatio(
            Palette.pick(found, ["cyan", "color6"], root.fallbackColor.symlink), bg, 4.5);
        root.color.executable = Contrast.ensureRatio(
            Palette.pick(found, ["green", "color2"], root.fallbackColor.executable), bg, 4.5);
        Color.loadColors(body);
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

    // Flea agrees with the compositor rather than carrying its own switch, the rule the corner
    // radius already follows; FLEA_REDUCED_MOTION is the test override and skips the ask.
    // Two forms below look like mistakes and are not: Quickshell.env returns null and not "" for
    // an unset variable, so the guard is a truthiness test, and StdioCollector text is a property
    // whose call throws. The query is Commons/Style.qml's own decoration:rounding shape.
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
