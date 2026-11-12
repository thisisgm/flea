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
export PYTHONDONTWRITEBYTECODE=1

# Both profiles unconditionally, the debug binary for the suites that drive it and the release one for thumbs.sh: an `[ ! -x <path> ]` guard is satisfied by a stale binary from an older commit, and measured on 2026-09-05 the debug and release hashes were unchanged across a whole run-all over edited source.
printf 'run-all: building target/debug/flea, the debug-binary suites need it\n'
cargo build -q || { printf 'run-all: cargo build failed, nothing else was run\n' >&2; exit 1; }
printf 'run-all: building target/release/flea, thumbs.sh needs it\n'
cargo build -q --release || { printf 'run-all: release build failed, nothing else was run\n' >&2; exit 1; }
printf 'run-all: building the headless IPC surface\n'
cargo build --locked -q -p flea-gui --example ipc-fixture || { printf 'run-all: IPC fixture build failed\n' >&2; exit 1; }

cargo build --locked -q -p flea-gui || exit 1

# Suites that need no display; each suite's own first comment line says what it checks.
headless="aur-versions preview-quality suite-inventory jobs rclone rclone-ci staticgates gpui-lint gpui-gates js keymap-gen charts budget pkgrel-check signalarity empty-state sandbox capability-ownership gio-auth gvfs ops empty-trash fatcopy modes update locale protocol viewerorder-backend portal archive unzip-progress thumbs thumbs-exec trashthumb-backend appimage-icon network-open-share network-keyless mount-listing lazy-objects startup-objects connections-style pdf-turn pdf-first preview-decode preview-swap preview-colwork preview-settle-live preview-select preview-hunt uistate uiwriter xwsettings xwstate media filemanager1 dragwire tabcatcher runall-rule tabreceive sidebarcost shellload bootload xwtab-scan settings-columns preview-frame preview-geometry jump-ui jump-gap picker-recent picker-stall grid-gap rowcost langmarks3 langtoggle columnscost columnrow-geom columndividers columnsfolder columnsback  headercost headerhandles menu-settle menu-snapshot-retire flash-ending-focus menus-evidence menu-shebang pane-states eject-verdict markdown-render markdown-figures markdown-figures-render figure-warm figure-store markdown-security markdown-spec markdown-html markdown-linearity markdown-lazy markdown-memory markdown-blockcost markdown-nest fencehosts preview-layout-loop lockedmenu arm-prompt acceptance-matrix counts listcost poolpark listprebuild gridprebuild slowsource-load cursormark listnarrow gridnarrow columnnarrow columnsfilter-edge filter-held gridmark search-exit-deep columnshidden search-warm rowscroll listnamebudget clickedge-origin ui-fixture-home ui-captures-sheet ui-captures-permissions ui-captures-fences cap-drops-trash network-kernel-mounts mtp-scan columnsfilter columns-peekdrag dropcase-guards search-live filter-firstkey search-exit-reads search-exit-columns wait-listing heldlisting dropmods-wait dropmods-windows search-keep foldersort-keep sortnew search-answered  search-sizes folderviews-flow sheet-query menu-scroll-width menu-refresh-scroll button-system menu-card-sink scroll-fill columnclip-empty listhidden gridhidden gridcaption gridbare statusbar-hint xw-addr xw-harness touchpad fs-matrix-smoke scroll-lanes touchpad-tool touchpad-edge scroll-bounds picker-grid picker-header picker-hunt picker-selection permissions-adv permissions-skips permissions-focus presskeep scrolloff-view hyprdispatch pointer-glide-proof sidebar-flows menu-clipboard-hunt menu-backend-hunt figure-package rename-scroll rename-list-bottom quicklook-firstframe quicklook-hold quicklook-followrest qslog-crash watch-views markdown-tables remote-start shelf-add-queue pdf-idle run-card-sink opener-ask tagrows sweep-picker-env click-chrome picker-save-wrap trash-locations gridcapbreak gridviewport hoverready dragready gridgrowth griddroplive heldkey trash-view gridresize shelf-mark html-preview markdown-preview-state"
# More suites that need no display; each suite's own first comment line says what it checks.
headless="$headless g1-01 g1-02 g1-03 g1-04 g1-06 g1-07 g1-08 g1-10 g1-11 g1-12 g1-13 g2-mn1 g2-pl3 g2-sr1 g2-pv2-md g2-pv2-web g2-pv2-office raw-corpus gpui-spinner gui-ipc gpui-theme-gate tui-cwd tui-battery tui-detail tui-owner-stall parity capsweep-check ring-bounds rename-frame rename-cost rename-lag menu-fit settings-tail dialog-board chromering framegap colwatch-gate cursor-swap cursorback field-write-guard preview-earlyask field-prefetch-park navcost navframe tui-wake glass glass-boot preview-holdframes field-entrant-sweep field-tui-frame quicklook-tablewarm spinner-still emptybeats hidden-release unlock-stdin unlock-lsblk-shim unlock-poller unlock-host archive-threat columnsprebuild ui-gvfsbridge-env devicelock dropmods-box-proof columns-peekdrag-noise slowsource-load-cleanup field-instant cap-filter-width readme-grab search-rawnames columns-held hiddenlast-views portal-startup portal-startup-abort holdoff-cap fillroles gridjump quicklook-firstkind slowsource-nas quicklook-zoom mediastrip-volume dualdivider menu-contents tagflyout railmotion theme-contrast glasspick-case reveal navsupersede keyreach heldcolumns  permissions-point-proof transferlive-owners transferhold tui-quit-playing tui-editor-stop mountfail-image glass-mode flingframe mpris mediasession fontpane preview-tall pv-gif pv-gif-reply columns-pwid code-engine code-column preview-raw field-launch-order field-strata-src preview-pdf-flow quicklook-code quicklook-rect pv-compare pv-compare-menu preview-scroll markdown-code csv-preview cohesion-reader capture-port raw-fixture columnsmoves fenceparity settings-ground raildrop material-startup volume-udisks properties-multi properties-races print-office-real matchtext-zerowidth native-transport native-transport-r1 search-ci-lint searchprior-ci"
headless="$headless g1-14"
headless="$headless i18n-quality"
# Sample input: "FAIL x", "suite: FAIL x" and "a FAIL: x" match, "ok FAILED" and "xFAIL x" do not; the flea-ci contract.
headless="$headless third-party-licenses gpui-capture-smoke gpui-visual-scenes gui-render-fallback g2-op1 g2-op2 g2-dg1 g2-mn2 g2-pv2 g2-pl1 g2-st2 field-product field-localsearch"
headless="$headless cli-contract cli-contract-mutations legacy-upgrade legacy-helpers state-compat chooser-replies"
# A UISH suite is registered here when its file lands, so the full run never names a missing suite.
headless="$headless ui-uish-inventory ui-uish-fixtures ui-uish-native-admission ui-uish-common-observers ui-gpui-headless"
# Bench, filter and search contracts that need no display.
headless="$headless bench-fleet-contract strata-bench bench-markdown-matrix g2-sr2 search-fb5"
headless="$headless bench-field-work bench-gate-provenance"
headless="$headless g1-05"
headless="$headless fs-instrument startup-order-tool"
headless="$headless bench-b3-capture"
headless="$headless native-runtime"
headless="$headless prove-ci-runner bench-raw-corpus-contract"
headless="$headless tool-preflight opr-recipe slop-ratchet"
headless="$headless bench-thumbs"
headless="$headless nvidia-gpui"
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
native-transport-gpui|needs a native display or Xvfb and the stage native picker entry
rename-timing|is a manual twenty-run protocol timing diagnosis through the host broker
rename-cost-matrix|is a manual three-run comparison of the frozen rename-cost gate
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
license-policy|needs cargo-deny, which the CI image does not carry, so it runs by name on a host that has it
hook-gate|standalone pinned-hk hook proof, verified separately in a marked Git fixture
ui-tui|is a standalone native TUI proof that needs the display and owns the display lock
themes|needs the display: it launches the candidate once per stock theme, and tests/js/themes.js is the half that runs here
fs-matrix|needs root, loop devices and the mkfs tools for the loop-mount half; fs-matrix-smoke runs the rest
fs-stick-images|needs root, loop devices, sfdisk and the mkfs tools to write the stick layouts
mdspec-shots|draws every spec example for the contact sheets and judges nothing
markdown-tables-assets|is the table assets library markdown-tables.sh and ui-captures-markdown.sh source, and judges nothing
headless-commons|is the shared OEM Commons fixture library sourced by the offscreen suites, and judges nothing
qslog-gate|is the Quickshell log classification library sourced by offscreen suites, and judges nothing
strata-fixtures|writes the full 6 GiB disk fixture and needs magick and ffmpeg, so it runs by name before a native bench
m9_integ_b1_missing_trials_never_pass_full_coverage|is run by strata-bench.sh, so the headless list runs it once
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
