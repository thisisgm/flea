#!/usr/bin/env bash
# Runs every suite that needs nothing but a shell, and names the ones that do not.
#
# This repo had twelve suites and no runner: seven were invoked by no file at all, including
# js.sh, the largest. A suite nobody runs reads as coverage in a directory listing and provides
# none. It happened again in 0.1.4: picker.sh, capability-ownership.sh and network-live.sh were
# named by no file at all, so the release's largest new surface had no automated coverage. The
# audit at the bottom is what makes that a failure here rather than a review finding later.
#
# Each suite's OWN exit code is read, never a pipeline's. `./tests/js.sh | tail -1` hands you
# tail's status and reports success over a red suite, which is how a wrong green survived here
# for a whole session.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# Omarchy exports gtk3, whose platform theme opens a display even for offscreen Qt, so the headless suites take Qt's generic theme.
export QT_QPA_PLATFORMTHEME=generic

# Both profiles unconditionally, the debug binary for the suites that drive it and the release one for thumbs.sh: an `[ ! -x <path> ]` guard is satisfied by a stale binary from an older commit, and measured on 2026-09-05 the debug and release hashes were unchanged across a whole run-all over edited source.
printf 'run-all: building target/debug/flea, the debug-binary suites need it\n'
cargo build -q || { printf 'run-all: cargo build failed, nothing else was run\n' >&2; exit 1; }
printf 'run-all: building target/release/flea, thumbs.sh needs it\n'
cargo build -q --release || { printf 'run-all: release build failed, nothing else was run\n' >&2; exit 1; }

# settings-ground pins ground dismissal and card, ruler and content grab ownership above the disabled pane.
headless="staticgates js keymap-gen charts budget aurpush aur-versions pkgrel-check signalarity empty-state sandbox capability-ownership gio-auth gvfs ops fatcopy modes update protocol portal archive thumbs thumbs-exec appimage-icon network-open-share network-keyless mount-listing lazy-objects startup-objects connections-style pdf-turn pdf-first preview-decode preview-swap preview-colwork preview-settle-live preview-select preview-hunt uistate uiwriter xwsettings xwstate media filemanager1 dragwire tabcatcher runall-rule tabreceive sidebarcost shellload bootload xwtab-scan settings-columns preview-frame preview-geometry jump-ui jump-gap picker-recent picker-stall grid-gap rowcost langmarks3 langtoggle columnscost columnrow-geom columndividers columnsfolder columnsback columnspeekgate headercost headerhandles menu-settle menu-snapshot-retire flash-ending-focus menus-evidence menu-shebang pane-states eject-verdict markdown-render markdown-figures markdown-figures-render figure-warm figure-store markdown-security markdown-spec markdown-html markdown-linearity markdown-lazy markdown-memory markdown-blockcost markdown-nest fencehosts preview-layout-loop lockedmenu arm-prompt acceptance-matrix counts listcost poolpark listprebuild gridprebuild slowsource-load cursormark listnarrow gridnarrow columnnarrow columnsfilter-edge filter-held gridmark search-exit-deep columnshidden search-warm rowscroll listnamebudget clickedge-origin ui-fixture-home ui-captures-sheet ui-captures-permissions ui-captures-fences cap-drops-trash network-kernel-mounts mtp-scan columnsfilter columns-peekdrag dropcase-guards search-live filter-firstkey search-exit-reads search-exit-columns wait-listing heldlisting dropmods-wait dropmods-windows search-keep foldersort-keep sortnew search-answered columnspeekorder search-sizes folderviews-flow sheet-query menu-scroll-width menu-refresh-scroll button-system menu-card-sink scroll-fill columnclip-empty listhidden gridhidden gridcaption gridbare statusbar-hint xw-addr xw-harness touchpad fs-matrix-smoke scroll-lanes touchpad-tool touchpad-edge scroll-bounds picker-grid picker-header picker-hunt picker-selection permissions-adv permissions-skips permissions-focus presskeep scrolloff-view hyprdispatch pointer-glide-proof sidebar-flows menu-clipboard-hunt menu-backend-hunt figure-package rename-scroll rename-list-bottom quicklook-firstframe quicklook-hold qslog-crash watch-views markdown-tables remote-start shelf-add-queue pdf-idle run-card-sink opener-ask tagrows sweep-picker-env click-chrome picker-save-wrap trash-locations gridcapbreak gridviewport hoverready dragready gridgrowth griddroplive heldkey trash-view gridresize shelf-mark html-preview markdown-preview-state"
# dualdivider drives the real WindowBody in Dual offscreen and holds the divider's split, drag, double click, clamp and rail states.
# capsweep-check runs capsweep-controls.py and capsweep-ipc.py; theme-contrast applies the 22 stock themes and two fixtures to the real Theme and holds muted and metadata text to 4.5:1, frames to 3:1 and the lit menu row to its floors; ring-bounds and rename-frame drive the real WindowBody offscreen and pin every focus ring and rename frame inside its host; chromering pins ChromeButton's own ring; framegap keeps every dialog, card and Settings frame the control inset clear of its rule and edge. cohesion-reader reads ipc cohesionControls from the real WindowBody and the picker window offscreen, holds every control box inside its window and pins the focus to the control the owner walk or flag names.
# menu-fit sizes the real context menu to its widest row and reads its edge fades; settings-tail walks Settings > Menus to its last row. field-write-guard drives the field bench's write guard with fixture meminfo and vmstat files. field-prefetch-park parks the operator's Flea prefetch list for a field run and restores it. tui-wake opens the TUI with a 5 s input wait and needs its rows at once. tui-quit-playing ends the TUI under a stand-in mpv and needs none of mpv's bytes after the terminal is restored. field-entrant-sweep ends what an entrant started outside the kill list. field-tui-frame stamps a TUI's first drawn screen. rename-cost counts each cursor key that reaches the pane's key handler after a rename in every view. quicklook-tablewarm counts the unseen warm table a rested Markdown table builds once. spinner-still holds the real Spinner still for its first second, then crawls. emptybeats checks the empty state rests after three beats per appearance. preview-holdframes counts the frames one column move draws under the preview hold. navcost steps the cursor through a real Pane offscreen and counts the held rows serialized per step. navframe pauses the real backend under Enter and Back in three views and counts the frames drawn before the landing. archive-threat drives the live backend with 7z, bsdtar and bwrap, then walks the password cards in a real Quickshell. field-instant runs the instant census harness's selftest and a dry run of every part against a synthetic root, reading the dual pointer target, the Enter row and the --only filter. columns-held holds j through a real Pane on 10,000 files in the list and in columns and counts backend requests, swap holds, loads, rebinds and frames per sweep. portal-startup starts the real xdg-desktop-portal frontend on a private bus with Flea as the preferred FileChooser and asserts it owns org.freedesktop.portal.Desktop. portal-startup-abort runs that suite against a frontend that never owns it, sends it a TERM, and fails its gdbus. holdoff-cap turns the real PDF surface past its render cap and reads the cap Timer, the loading state's minimum and a page landing before the cap. gridjump drives the real GridArea beside the List with a stub pane and counts what a far jump and a held sweep rebind and ask. quicklook-firstkind rests the cursor on the first PDF, clip, picture and text file of a fresh window, then presses Space, and requires the kind's unit and Quick Look's own to be Component.Ready before the key, nothing compiled inside it, and no PDF or clip unit held by a window that never rested on one. slowsource-nas reports the NAS storage class through a backend proxy that holds a folder listing or a file read, and requires the loading mark on the first frame after the Timer that owns it and no read of the browsed folder on the GUI thread. quicklook-zoom drives the real Quick Look through four + presses, a drag, 0, a move and a reopen on large, small and vector pictures and counts the full decodes. preview-raw opens a camera RAW in the real Quick Look and column over a stub backend and reads the caption band under the embedded picture. mediastrip-volume draws the real Quick Look media strip with its volume track, drags the level and counts the saves. menu-contents opens the real Properties card on a folder, feeds it counting and done replies, and reads every pair per frame. rename-cost counts each cursor key that reaches the pane's key handler after a rename in every view, and the frames, replies, delegates and row reads those keys cost against the same keys before it. reveal fades the real dialog, share browser and context menu in and out on the shared curve and checks the reduced-motion snap. capsweep-check runs capsweep-controls.py and capsweep-ipc.py; ring-bounds and rename-frame drive the real WindowBody offscreen and pin every focus ring and rename frame inside its host; chromering pins ChromeButton's own ring; framegap keeps every dialog, card and Settings frame the control inset clear of its rule and edge. mpris feeds tools/flea-mpris state lines on a private bus and reads the player back. mediasession plays a clip and a tone through the real PreviewMedia and Quick Look against a stub helper and reads the idle inhibitor and the state lines. preview-tall checks the tall kinds' frame, compact facts line and the i toggle in a Columns window offscreen. columns-pwid drives the real Columns view's preview column edge: no line at rest, hover, a live drag with one save on release, both clamps, a double click, and a relaunch. code-engine drives the live backend's code requests with bat and jq in the jail and counts one bwrap per coloured answer. code-column drives the Columns preview over the real backend and bat, checks the theme inks, the binary, too large and long-line frames and the plain fallback, and samples every live frame of a j and k walk for an uncoloured one. field-launch-order prints the rotated entrant order for 2 and 8 entrants and requires every ordered pair to lead equally often over 2N runs. field-strata-src derives STRATA_SRC from STRATA_BIN and refuses a run whose source does not match the binary. preview-pdf-flow checks that a PDF in the Columns preview fills the frame width with its pages stacked under a scroll knob, builds only the pages near the frame and leaves Quick Look on one page, offscreen. quicklook-code drives Quick Look on code and text over the real backend and bat, checks the bar, the gutter, w and its memory across two windows, and the binary and too-large frames. quicklook-rect opens code, a PDF and a portrait picture at 1200 x 760 and requires the tall rect on the first revealed frame with no later resize. pv-compare opens the Compare overlay in the real Quick Look with stub answers for the board's hunk and each refusal, checks the recipe's geometry and inks, y and the button copying once, then asks the live backend about real files. preview-scroll drives wheel and F/B in Columns, preserving listing focus and each file's position on its first live frame. markdown-code runs Markdown Source and fenced blocks over the real backend and bat, in Quick Look and the column, and holds markup to text with a listener that no fetch may reach. csv-preview draws CSV and TSV as a table in the preview column and in Quick Look over the real backend, checking the header, the numbers, the widths held across a 49,999-row scroll, the quoted newline and the binary, large and empty frames.
headless="$headless capsweep-check ring-bounds rename-frame rename-cost menu-fit settings-tail dialog-board chromering framegap colwatch-gate cursor-swap cursorback field-write-guard preview-earlyask field-prefetch-park navcost navframe tui-wake glass glass-boot preview-holdframes field-entrant-sweep field-tui-frame quicklook-tablewarm spinner-still emptybeats hidden-release unlock-stdin unlock-lsblk-shim unlock-poller unlock-host archive-threat columnsprebuild ui-gvfsbridge-env devicelock dropmods-box-proof columns-peekdrag-noise slowsource-load-cleanup field-instant cap-filter-width readme-grab search-rawnames columns-held hiddenlast-views portal-startup portal-startup-abort holdoff-cap fillroles gridjump quicklook-firstkind slowsource-nas quicklook-zoom mediastrip-volume dualdivider menu-contents tagflyout railmotion theme-contrast glasspick-case reveal navsupersede keyreach heldcolumns columnsdrawn permissions-point-proof transferlive-owners transferhold tui-quit-playing mountfail-image glass-mode flingframe mpris mediasession fontpane preview-tall pv-gif pv-gif-reply columns-pwid code-engine code-column preview-raw field-launch-order field-strata-src preview-pdf-flow quicklook-code quicklook-rect pv-compare pv-compare-menu preview-scroll markdown-code csv-preview cohesion-reader capture-port raw-fixture columnsmoves fenceparity settings-ground raildrop material-startup"
# Sample input: "FAIL x", "suite: FAIL x" and "a FAIL: x" match, "ok FAILED" and "xFAIL x" do not; the flea-ci contract.
fail_line='(^|[: ])FAIL[: ]'
failed=0
ran=0

for name in $headless; do
    suite="tests/$name.sh"
    [ -x "$suite" ] || { printf '  %-14s FAIL   no executable at %s\n' "$name" "$suite"; failed=$((failed + 1)); continue; }
    out=$("./$suite" 2>&1)
    rc=$?
    ran=$((ran + 1))
    # The suites do not share a summary format, so the last non-empty line is quoted as-is
    # rather than parsed into a number this script would then have to keep true.
    last=$(printf '%s\n' "$out" | grep -v '^[[:space:]]*$' | tail -1)
    # Exit status and output must agree: a suite that exits 0 over a FAIL line still failed.
    printed=""
    # A here-string, not a pipe: grep -m1 closing a pipe early kills the writer, and pipefail then drops a line it found.
    [ "$rc" -ne 0 ] || printed=$(grep -a -m1 -E "$fail_line" <<< "$out") || printed=""
    if [ "$rc" -eq 0 ] && [ -n "$printed" ]; then
        printf '  %-14s FAIL   rc=0 but its output holds a FAIL line: %s\n' "$name" "$printed"
        failed=$((failed + 1))
    elif [ "$rc" -eq 0 ]; then
        printf '  %-14s ok     %s\n' "$name" "$last"
    else
        printf '  %-14s FAIL   rc=%s  %s\n' "$name" "$rc" "$last"
        failed=$((failed + 1))
    fi
done

# Named, not run: each needs something this script cannot assume it has. One list, read twice: it
# is printed here and it is what the audit below checks, so a suite cannot be quietly excluded.
not_run="
rename-grid-bottom|needs node, which the flea-ci image does not carry, so it runs by name on a host that has node
preview-040|exits 0 only on its known per-file scroll restoration failure and exits 1 on any other result, unexpected green included
ui|needs the display, and refuses beside a Flea it did not start; cap_raw_grid, cap_raw_columns, cap_raw_quicklook and cap_raw_properties also need native screenshots
drag|needs the display and a real pointer through uinput
drag-look235|needs display and uinput capture for review
drag-look235-case|is a case library sourced by drag.sh look235
cardsizes|needs the display, a real pointer through uinput, and Hyprland to resize the window
bench|is a separate headless benchmark-contract suite
package|needs a real makepkg archive in FLEA_PACKAGE_FILE
picker|needs the display, a session bus, and Flea activatable as the FileChooser backend
picker-issue313-native|needs native Omarchy input, Qt 6.12 and the serialized minipc display lane
picker-040|is 0.3.10's acceptance suite and is red until the picker has path entry and collision review
network-live|needs live share credentials and the approved runtime bundle, maintainer only
hook-gate|standalone pinned-hk hook proof, verified separately in a marked Git fixture
ui-tui|is a standalone native TUI proof that needs the display and owns the display lock
themes|needs the display: it launches the candidate once per stock theme, and tests/js/themes.js is the half that runs here
fs-matrix|needs root, loop devices and the mkfs tools for the loop-mount half; fs-matrix-smoke runs the rest
fs-stick-images|needs root, loop devices, sfdisk and the mkfs tools to write the stick layouts
mdspec-shots|draws every spec example for the contact sheets and judges nothing
markdown-tables-assets|is the table assets library markdown-tables.sh and ui-captures-markdown.sh source, and judges nothing
headless-commons|is the shared OEM Commons fixture library sourced by the offscreen suites, and judges nothing
qslog-gate|is the Quickshell log classification library sourced by offscreen suites, and judges nothing
"

printf '\nNot run here, and why:\n'
named=""
while IFS='|' read -r name reason; do
    [ -n "$name" ] || continue
    named="$named $name"
    printf '  %-16s %s\n' "$name.sh" "$reason"
done <<EOF
$not_run
EOF

# Every suite in tests/ is in one of the two lists. A new one in neither is invoked by no file and
# mentioned by none, which is the state picker.sh shipped in, so it fails this runner rather than
# waiting for somebody to notice the directory listing is longer than the report.
orphans=""
for suite in tests/*.sh; do
    name=${suite#tests/}
    name=${name%.sh}
    [ "$name" = run-all ] && continue
    case " $headless $named " in
        *" $name "*) continue ;;
    esac
    # A ui-*.sh that tests/ui.sh sources is a case library, not a suite: it is run whenever ui.sh is,
    # and it has no entry point of its own. Read out of ui.sh rather than listed here, so a library
    # that stops being sourced becomes an orphan again instead of staying quietly excused.
    if grep -q "tests/$name\.sh" tests/ui.sh 2>/dev/null; then
        continue
    fi
    orphans="$orphans $name"
done
if [ -n "$orphans" ]; then
    printf '\nrun-all: FAIL suite(s) that no list runs and no line names:%s\n' "$orphans"
    failed=$((failed + 1))
fi

# Last, so it counts the audit above as well as the suites: a tally printed before the last check
# ran is the same wrong green this runner was written to stop.
printf '\nrun-all: %d suite(s) run, %d failed\n' "$ran" "$failed"

exit "$failed"
