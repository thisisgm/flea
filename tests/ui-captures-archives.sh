# Sourced by ui.sh beside the other case files, runs only by name: Archives040 submenu, password dialog, extract card, ISO Quick Look and the free-space refusal, fixtures in its own root.
# Holds the pointer on a card's eye through ydotool, shoots the revealed field, then lets go; reader is the archiveCardsState key holding the card's capture.
. "$repo/tests/lib/pointer-glide.sh"
cap_archives_hold_eye() {
    local reader="$1" shot_name="$2" eye ex ey wx wy ww wh
    eye=$(ipc archiveCardsState | jq -er --arg r "$reader" '.[$r].eye') || fail "cap_archives: no eye centre in $reader"
    read -r ex ey <<< "$eye"
    read -r wx wy ww wh < <(window_box) || fail "cap_archives: native window coordinates unavailable"
    # Through ydotool, since Hyprland's cursor move sends Qt no pointer frame.
    pointer_glide "$((wx + ex))" "$((wy + ey))" || fail "cap_archives: pointer move onto the eye failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "cap_archives: pointer press on the eye failed"
    menus_expect archiveCardsState ".$reader.revealed" "the held eye reveals the typed password"
    settle
    shot "$shot_name"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "cap_archives: pointer release failed"
    menus_expect archiveCardsState ".$reader.revealed | not" "letting go of the eye masks the field again"
}
case_cap_archives() {
    local dir="$fixture_root/cap-archives" menus_checks=0 submenu="" glyphs="" names="" end status=""
    command -v 7z >/dev/null || { printf 'CAP SKIP cap-archives: 7z is not installed\n'; return 0; }
    sandbox_scratch "$dir"
    printf 'one\n' > "$dir/a.txt"
    printf 'two\n' > "$dir/b.txt"
    printf 'three\n' > "$dir/c.txt"
    printf 'secret\n' > "$dir/secret.txt"
    # One stdin line carries the password; 7z never sees it in argv.
    ( cd "$dir" && printf 'pw\n' | 7z a -p -mhe=on backup.7z secret.txt >/dev/null ) \
        || fail "cap_archives: 7z could not make the password fixture"
    local iso_ok=false iso_src="$fixture_root/cap-archives-isosrc"
    if command -v bsdtar >/dev/null; then
        sandbox_scratch "$iso_src"
        printf 'one\n' > "$iso_src/a.txt"
        printf 'two\n' > "$iso_src/b.txt"
        if ( cd "$iso_src" && bsdtar --format=iso9660 -cf "$dir/image.iso" . >/dev/null 2>&1 ); then iso_ok=true; fi
    fi
    # A GNU sparse tar of an 8 TB hole: 10 kB on disk, and bsdtar lists the full size, so the extract is refused before any child runs.
    local huge_ok=false huge_src="$fixture_root/cap-archives-sparse" huge_bytes=8796093022208 avail huge_why="GNU tar -S, or 8 TB free in the test root"
    sandbox_scratch "$huge_src"
    avail=$(df -B1 --output=avail "$dir" | tail -n 1 | tr -d ' ')
    if ! command -v bsdtar >/dev/null; then
        huge_why="bsdtar is not installed"
    elif [[ "$avail" =~ ^[0-9]+$ ]] && (( avail < huge_bytes )) && truncate -s "$huge_bytes" "$huge_src/big.bin" 2>/dev/null \
        && tar -S -C "$huge_src" -cf "$dir/huge.tar" big.bin 2>/dev/null \
        && [[ "$(bsdtar -tvf "$dir/huge.tar" | awk '{print $5}')" == "$huge_bytes" ]]; then huge_ok=true; fi
    seed_ui_state "$fixture_root/cap-archives-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    local listed=5 extracted=6
    if [[ "$iso_ok" == true ]]; then listed=$((listed + 1)); extracted=$((extracted + 1)); fi
    if [[ "$huge_ok" == true ]]; then listed=$((listed + 1)); extracted=$((extracted + 1)); fi
    wait_listing "$listed"
    cap_resize 1100 700
    [[ "$(ipc archiveFormats)" == *"7z"* ]] \
        || fail "cap_archives: 7z installed but no 7z in archiveFormats [$(ipc archiveFormats)]"
    # Three rows selected, so the dialog titles itself for 3 items.
    click_row "$(row_index_of a.txt)" left
    settle
    click_row "$(row_index_of b.txt)" left --mods ctrl
    settle
    click_row "$(row_index_of c.txt)" left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "3" ]] || fail "cap_archives: three ctrl clicks selected $(ipc selectionCount), not 3"
    click_row "$(row_index_of a.txt)" right
    settle
    menus_expect menuState '.opened and .hasRow' "the file menu opens over the selection"
    menu_seek Compress
    key -k Right >/dev/null
    settle
    # The submenu is the probed table with With password last under its separator.
    menus_expect menuState '.submenu and ([.submenuEntries[] | select(.label == "With password…")] | length) == 1' "the Compress flyout ends in With password"
    submenu=$(ipc menuState | jq -r '[.submenuEntries[] | .label // "-"] | join("|")')
    [[ "$submenu" == *"|-|With password…" ]] \
        || fail "cap_archives: With password is not last under its separator: $submenu"
    glyphs=$(ipc contextMenuSubmenuGlyphs)
    [[ "$glyphs" == *"|key" ]] || fail "cap_archives: With password wears no key mark: $glyphs"
    shot archives-compress-submenu
    menu_seek_submenu "With password…"
    key -k Return >/dev/null
    # The dialog opens over the three resolved paths with 7z selected.
    menus_expect archiveCardsState '.compressOpened and .compressCount == 3 and .compressFormat == "7z"' "the password dialog opens for 3 items in 7z"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_archives: the menu stayed open behind the dialog"
    settle
    shot archives-compress-dialog
    # Mismatched confirmations refuse with the error frame and no submit.
    key "pw" >/dev/null
    key -k Tab >/dev/null
    settle
    key "nope" >/dev/null
    key -k Return >/dev/null
    menus_expect archiveCardsState '.compressOpened and (.compressError | contains("do not match")) and .compressCapture.confirmError' "a mismatched confirm refuses in the error frame"
    settle
    shot archives-compress-mismatch
    # Matching pair enables Compress; Tab reaches the Encrypt row with its focus.
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null
    settle
    key "pw" >/dev/null
    key -k Tab >/dev/null
    settle
    menus_expect archiveCardsState '.compressOpened and .canCompress' "a matching pair enables Compress"
    # The Tab above already reached the Encrypt row; no second Tab, which would move on to Cancel.
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 4 and .compressCapture.encrypt' "Tab reaches the Encrypt file names row"
    settle
    shot archives-compress-focus
    # Space unchecks Encrypt with its focus kept; Shift+Tab leaves it at rest on the confirm field.
    key -k space >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 4 and (.compressCapture.encrypt | not)' "Space unchecks Encrypt file names"
    settle
    shot archives-compress-encrypt-off-focus
    key -M shift -k Tab -m shift >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 0 and (.compressCapture.encrypt | not)' "Shift+Tab leaves Encrypt unchecked at rest"
    settle
    shot archives-compress-encrypt-off-rest
    # The eye held over the typed password shows it in plain text for as long as it is held.
    cap_archives_hold_eye compressCapture archives-compress-eye-held
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 5' "Tab reaches Cancel"
    settle
    shot archives-compress-cancel-focus
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 6' "Tab reaches Compress"
    settle
    shot archives-compress-submit-focus
    # Tab wraps to the Format segment, the card's first stop; Right picks zip, which hides the Encrypt row.
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 1 and .compressFormat == "7z"' "Tab reaches the Format segment"
    settle
    shot archives-compress-format-focus
    key -k Right >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFormat == "zip" and (.compressCapture.encryptShown | not)' "zip hides the Encrypt row"
    settle
    shot archives-compress-zip
    key -k Escape >/dev/null
    menus_expect archiveCardsState '.compressOpened | not' "Escape closes the password dialog"
    settle
    # Extracting the encrypted archive opens the password card for its leaf.
    click_row "$(row_index_of secret.txt)" left
    settle
    click_row "$(row_index_of backup.7z)" left
    settle
    click_row "$(row_index_of backup.7z)" right
    settle
    menus_expect menuState '.opened and .hasRow' "the menu opens over backup.7z"
    menu_seek Extract
    key -k Return >/dev/null
    menus_expect archiveCardsState '.extractOpened and .extractLeaf == "backup.7z" and .extractError == ""' "the encrypted archive asks for its password"
    settle
    shot archives-extract-password
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.extractOpened and .extractFocus == 1' "Tab reaches the extract card's Cancel"
    settle
    shot archives-extract-cancel-focus
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.extractOpened and .extractFocus == 2' "Tab reaches Extract"
    settle
    shot archives-extract-submit-focus
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.extractOpened and .extractFocus == 0' "Tab wraps back to the password field"
    # A wrong answer reopens the card with the typed bullets kept in the error frame, its sentence under the field.
    key "nope" >/dev/null
    key -k Return >/dev/null
    menus_expect archiveCardsState '.extractOpened and (.extractError | contains("did not open backup.7z")) and .extractCapture.filled and .extractCapture.fieldError' "a wrong password asks again with the bullets kept"
    settle
    shot archives-extract-wrong
    # The right password extracts beside the archive; nothing lands before it does.
    [[ ! -e "$dir/backup" ]] || fail "cap_archives: a refused extract already wrote $dir/backup"
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null
    menus_expect archiveCardsState '.extractOpened and .extractError == "" and (.extractCapture.fieldError | not)' "typing drops the sentence and the error frame"
    settle
    key "pw" >/dev/null
    key -k Return >/dev/null
    wait_listing "$extracted"
    [[ -f "$dir/backup/secret.txt" ]] || fail "cap_archives: the right password extracted nothing to $dir/backup"
    # needs an ISO writer the case cannot assume: bsdtar without iso9660 support.
    if [[ "$iso_ok" != true ]]; then
        printf 'CAP SKIP archives-iso-quicklook: no ISO writer made image.iso\n'
    else
        seek_row_named "image.iso" || fail "cap_archives: could not find image.iso"
        key -k space >/dev/null
        end=$((SECONDS + 15))
        while (( SECONDS < end )); do
            names=$(ipc previewArchiveNames) || fail "cap_archives: the archive names reader failed"
            [[ -n "$names" ]] && break
            sleep 0.05
        done
        [[ -n "$names" ]] || fail "cap_archives: the ISO listed no entries in Quick Look"
        [[ "$(ipc previewOpen)" == "true" ]] || fail "cap_archives: Space opened no Quick Look over image.iso"
        settle
        shot archives-iso-quicklook
        key -k Escape >/dev/null
        settle
    fi
    # needs GNU tar's sparse mode and a test root with under 8 TB free.
    if [[ "$huge_ok" != true ]]; then
        printf 'CAP SKIP archives-free-space-refusal: no sparse huge.tar (%s)\n' "$huge_why"
    else
        click_row "$(row_index_of huge.tar)" left
        settle
        click_row "$(row_index_of huge.tar)" right
        settle
        menus_expect menuState '.opened and .hasRow' "the menu opens over huge.tar"
        menu_seek Extract
        key -k Return >/dev/null
        end=$((SECONDS + 15))
        while (( SECONDS < end )); do
            status=$(ipc statusPrimary) || fail "cap_archives: the status reader failed"
            [[ "$status" == "Extracting huge.tar needs 8.8 TB, and cap-archives has "*" free." ]] && break
            sleep 0.05
        done
        [[ "$status" == "Extracting huge.tar needs 8.8 TB, and cap-archives has "*" free." ]] \
            || fail "cap_archives: the free-space refusal never reached the status bar: $status"
        [[ "$(ipc statusError)" == "true" ]] || fail "cap_archives: the free-space refusal is not drawn as an error"
        [[ ! -e "$dir/huge" ]] || fail "cap_archives: a refused extract wrote $dir/huge"
        settle
        shot archives-free-space-refusal
    fi
    printf 'CAP_ARCHIVES submenu=with-password-last dialog=rest,mismatch,focus,encrypt-off,eye,cancel,submit,format,zip extract=card,cancel,submit,wrong iso=%s free-space=%s\n' "$iso_ok" "$huge_ok"
    kill_flea
}

# Hue by Theme's own rule, printed as one CAP_HSF line; refuse-hue fails on hue, record only notes it.
cap_archives_format_hue() {
    local toml="$1" mode="$2" accent="" floor="" sat="" has=""
    # Sample input, one line of colors.toml: accent = "#7e9cd8"
    accent=$(grep -E '^accent' "$toml" | grep -oE '#[0-9A-Fa-f]{6}') || fail "cap_archives_format: no accent in $toml"
    # Sample input, one line of ui/Theme.qml: readonly property real hueFloor: 0.2
    floor=$(grep -oE 'hueFloor: [0-9.]+' "$repo/ui/Theme.qml" | grep -oE '[0-9.]+') || fail "cap_archives_format: no hueFloor in ui/Theme.qml"
    read -r sat has < <(python3 - "$accent" "$floor" <<'PY'
# Sample input: argv "#7e9cd8" "0.2".
import sys
h = sys.argv[1].lstrip('#')
f = float(sys.argv[2])
r, g, b = int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255
mx, mn = max(r, g, b), min(r, g, b)
sat = 0.0 if mx == 0 else (mx - mn) / mx
print("%s %s" % ("%.4f" % sat, 'true' if sat > f else 'false'))
PY
    ) || fail "cap_archives_format: hue read failed for $accent"
    printf 'CAP_HSF accent=%s saturation=%s floor=%s hasHue=%s\n' "$accent" "$sat" "$floor" "$has"
    [[ "$mode" != refuse-hue || "$has" != true ]] || fail "cap_archives_format: kanagawa fixture has hue, $accent at $sat over $floor"
}

# Three files into Compress With password, the dialog open for the Format walk, no shots.
cap_archives_format_open() {
    local dir="$1" submenu="" glyphs=""
    sandbox_scratch "$dir"
    printf 'one\n' > "$dir/a.txt"
    printf 'two\n' > "$dir/b.txt"
    printf 'three\n' > "$dir/c.txt"
    seed_ui_state "$fixture_root/cap-archives-format-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 3
    cap_resize 1100 700
    [[ "$(ipc archiveFormats)" == *"7z"* ]] || fail "cap_archives_format: 7z installed but no 7z in archiveFormats [$(ipc archiveFormats)]"
    click_row "$(row_index_of a.txt)" left
    settle
    click_row "$(row_index_of b.txt)" left --mods ctrl
    settle
    click_row "$(row_index_of c.txt)" left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "3" ]] || fail "cap_archives_format: three ctrl clicks selected $(ipc selectionCount), not 3"
    click_row "$(row_index_of a.txt)" right
    settle
    menus_expect menuState '.opened and .hasRow' "the file menu opens over the selection"
    menu_seek Compress
    key -k Right >/dev/null
    settle
    menus_expect menuState '.submenu and ([.submenuEntries[] | select(.label == "With password…")] | length) == 1' "the Compress flyout ends in With password"
    submenu=$(ipc menuState | jq -r '[.submenuEntries[] | .label // "-"] | join("|")')
    [[ "$submenu" == *"|-|With password…" ]] || fail "cap_archives_format: With password is not last under its separator: $submenu"
    glyphs=$(ipc contextMenuSubmenuGlyphs)
    [[ "$glyphs" == *"|key" ]] || fail "cap_archives_format: With password wears no key mark: $glyphs"
    menu_seek_submenu "With password…"
    key -k Return >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressCount == 3 and .compressFormat == "7z"' "the password dialog opens for 3 items in 7z"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_archives_format: the menu stayed open behind the dialog"
    settle
}

# Matching pair then Tab to the Format segment, the same forward walk the old case ends with.
cap_archives_format_walk() {
    key "pw" >/dev/null
    key -k Tab >/dev/null
    settle
    key "pw" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    menus_expect archiveCardsState '.compressOpened and .compressFocus == 1 and .compressFormat == "7z"' "Tab reaches the Format segment"
}

# Format focus on the live theme, one shot.
case_cap_archives_format_live() {
    local dir="$fixture_root/cap-archives-format-live" menus_checks=0
    command -v 7z >/dev/null || { printf 'CAP SKIP cap-archives-format-live: 7z is not installed\n'; return 0; }
    cap_archives_format_hue "$real_state_dir/theme/colors.toml" record
    cap_archives_format_open "$dir"
    cap_archives_format_walk
    settle
    assert_theme
    shot archives-format-focus-live
    kill_flea
}

# Format focus on kanagawa through a child HOME, one shot.
case_cap_archives_format_kanagawa() {
    local dir="$fixture_root/cap-archives-format-kanagawa" menus_checks=0 src="${kanagawa_colors:-/usr/share/omarchy/themes/kanagawa/colors.toml}"
    command -v 7z >/dev/null || { printf 'CAP SKIP cap-archives-format-kanagawa: 7z is not installed\n'; return 0; }
    [[ -f "$src" ]] || fail "cap_archives_format_kanagawa: kanagawa colors.toml absent at $src"
    local sweep_root="$fixture_root/cap-archives-format-kanagawa-home" sweep_home="$fixture_root/cap-archives-format-kanagawa-home/home" sweep_real_bin="$flea_bin"
    local real_foreground="" flea_bin="$flea_bin"
    sandbox_make "$sweep_root"
    fixture_home_make "$sweep_home"
    cp "$src" "$sweep_home/.local/state/omarchy/current/theme/colors.toml"
    printf 'kanagawa\n' > "$sweep_home/.local/state/omarchy/current/theme.name"
    # Sample input, one line of colors.toml: foreground = "#dcd7ba"
    real_foreground=$(python3 -c 'import pathlib,sys,tomllib; print(tomllib.loads(pathlib.Path(sys.argv[1]).read_text())["foreground"].lower())' "$sweep_home/.local/state/omarchy/current/theme/colors.toml")
    flea_bin="$sweep_root/launcher"
    { printf '#!/usr/bin/env bash\n'; printf 'exec env HOME=%q %q "$@"\n' "$sweep_home" "$sweep_real_bin"; } > "$flea_bin"
    chmod +x "$flea_bin"
    cap_archives_format_hue "$sweep_home/.local/state/omarchy/current/theme/colors.toml" refuse-hue
    cap_archives_format_open "$dir"
    cap_archives_format_walk
    settle
    assert_theme
    shot archives-format-focus-kanagawa
    kill_flea
}

# Issue 397: two marked archives offer Extract, and choosing it extracts each into its own folder.
case_cap_archives_several() {
    local dir="$fixture_root/cap-archives-several" menus_checks=0 src="$fixture_root/cap-archives-several-src" i
    command -v bsdtar >/dev/null || { printf 'CAP SKIP cap-archives-several: bsdtar is not installed\n'; return 0; }
    sandbox_scratch "$dir"
    sandbox_scratch "$src"
    printf 'alpha\n' > "$src/alpha.txt"
    printf 'beta\n' > "$src/beta.txt"
    bsdtar -a -cf "$dir/first.zip" -C "$src" alpha.txt || fail "cap_archives_several: no zip fixture"
    bsdtar -a -cf "$dir/second.tar.gz" -C "$src" beta.txt || fail "cap_archives_several: no tar.gz fixture"
    printf 'notes\n' > "$dir/notes.txt"
    seed_ui_state "$fixture_root/cap-archives-several-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 3
    cap_resize 1100 700
    click_row "$(row_index_of first.zip)" left
    settle
    click_row "$(row_index_of second.tar.gz)" left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "2" ]] || fail "cap_archives_several: two clicks selected $(ipc selectionCount), not 2"
    click_row "$(row_index_of first.zip)" right
    settle
    menus_expect menuState '.opened and .hasRow' "the file menu opens over the two archives"
    printf 'CAP several menu=%s\n' "$(ipc contextMenuEntries)"
    shot archives-several-menu
    menus_expect menuState '([.entries[] | select(.action == "extract" and .disabled != true)] | length) == 1' "two marked archives offer an enabled Extract"
    menu_seek Extract
    key -k Return >/dev/null
    for i in $(seq 1 60); do
        [[ -f "$dir/first/alpha.txt" && -f "$dir/second/beta.txt" ]] && break
        sleep 0.25
    done
    [[ "$(cat "$dir/first/alpha.txt" 2>/dev/null)" == alpha && "$(cat "$dir/second/beta.txt" 2>/dev/null)" == beta ]] \
        || fail "cap_archives_several: each archive did not land in its own folder: $(ls -A "$dir")"
    printf 'CAP several landed=%s\n' "$(ls -A "$dir" | tr '\n' ' ')"
    settle
    shot archives-several-done
    kill_flea
}
