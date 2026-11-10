# RunFile board captures: the run card for a script, an ELF and an AppImage with and without a handler, each with the ring on its default, on every other button and under Shift, plus a missing-bit script and Open with.
. "$repo/tests/lib/pointer-glide.sh"

# Run (Shift+Enter on Run in Terminal) and Run in Terminal are pressed only on a harmless fixture script that writes a marker inside this case's own test root.

# Poll runCardState until .opened matches the want ("true" or "false").
runfile_wait_card() {
    local want="$1" seen="" poll
    for poll in $(seq 1 100); do
        seen=$(ipc runCardState 2>/dev/null || printf '{"opened":"ipc-broken"}')
        if [[ "$want" == "true" ]]; then
            jq -e '.opened == true' <<< "$seen" >/dev/null 2>&1 && return 0
        else
            jq -e '.opened == false' <<< "$seen" >/dev/null 2>&1 && return 0
        fi
        sleep 0.05
    done
    fail "runfile: the run card never became opened=$want, last state $seen"
}

# Assert one jq expression against the open card's runCardState.
runfile_expect() {
    local expression="$1" label="$2" seen
    seen=$(ipc runCardState) || fail "runfile: runCardState unreadable for $label"
    jq -e "$expression" <<< "$seen" >/dev/null \
        || fail "runfile: $label, card state $seen"
}

# Step the card focus to the named button with h/l, the card's own keys.
runfile_focus() {
    local want="$1" seen="" focus="" buttons="" at="" target="" step
    for step in $(seq 1 8); do
        seen=$(ipc runCardState) || fail "runfile: runCardState unreadable seeking $want"
        focus=$(jq -er '.focus' <<< "$seen")
        [[ "$focus" == "$want" ]] && return 0
        buttons=$(jq -er '.buttons | join(" ")' <<< "$seen")
        at=$(jq -er --arg f "$focus" '.buttons | index($f)' <<< "$seen")
        target=$(jq -er --arg f "$want" '.buttons | index($f)' <<< "$seen")
        [[ "$at" =~ ^[0-9]+$ && "$target" =~ ^[0-9]+$ ]] \
            || fail "runfile: focus $focus has no seat beside $want in $buttons"
        if (( at > target )); then key h >/dev/null; else key l >/dev/null; fi
        settle
    done
    fail "runfile: focus never reached $want, last state $seen"
}

# Hold or release Shift through ydotool, which relabels Run in Terminal to Run (HIG runfile, APL-DIALOG-13); 42 is KEY_LEFTSHIFT.
runfile_shift() {
    local state="$1" want="$2" poll label=""
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key "42:$state" >/dev/null 2>&1 || fail "runfile: Shift $state failed"
    for poll in $(seq 1 100); do
        label=$(ipc runCardState | jq -r '.controls[] | select(.name == "terminal") | .label')
        [[ "$label" == "$want" ]] && return 0
        sleep 0.05
    done
    [[ "$state" == 0 ]] || YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 42:0 >/dev/null 2>&1
    fail "runfile: with Shift $state the terminal button reads $label, not $want"
}

# Shift+Enter on the focused Run in Terminal runs the program itself; Return goes only once the held Shift reads Run, and the card closing proves it landed.
runfile_run() {
    local key_enter=28 key_left_shift=42 pressed=0
    runfile_focus terminal
    : > "$dir/bin/terminal.argv"
    runfile_shift 1 Run
    # Cases run under set -e, so the Return status is caught here and Shift is released before any failure.
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key "$key_enter:1" "$key_enter:0" >/dev/null || pressed=$?
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key "$key_left_shift:0" >/dev/null || fail "runfile: ydotool could not release Shift"
    (( pressed == 0 )) || fail "runfile: ydotool could not press Return with Shift held, exit $pressed"
    runfile_wait_card false
}

# Once a Run's effect has landed, the terminal launcher stub must still be empty.
runfile_not_terminal() {
    [[ ! -s "$dir/bin/terminal.argv" ]] || fail "runfile: Shift+Enter reached the terminal launcher: $(tr '\n' ' ' < "$dir/bin/terminal.argv")"
}

# Put the cursor on the named row, press Enter and wait for the card.
runfile_open() {
    local name="$1"
    seek_row_named "$name" || fail "runfile: no row named $name"
    key -k Return >/dev/null
    runfile_wait_card true
    settle
}

# Close the card with Escape and wait for it to leave.
runfile_close() {
    key -k Escape >/dev/null
    runfile_wait_card false
    settle
}

# Poll the Open with dialog until .opened matches the want.
runfile_wait_openwith() {
    local want="$1" seen="" poll
    for poll in $(seq 1 100); do
        seen=$(ipc openWithState 2>/dev/null || printf '{"opened":"ipc-broken"}')
        [[ "$(jq -r '.opened' <<< "$seen" 2>/dev/null)" == "$want" ]] && return 0
        sleep 0.05
    done
    fail "runfile: Open with never became opened=$want, last state $seen"
}

# One Esc dismisses the head sticky error, which otherwise masks every later sentence.
runfile_dismiss_error() {
    local poll
    [[ "$(ipc statusError 2>/dev/null)" == "true" ]] || return 0
    key -k Escape >/dev/null
    for poll in $(seq 1 100); do
        [[ "$(ipc statusError 2>/dev/null)" == "false" ]] && return 0
        sleep 0.05
    done
    fail "runfile: the refusal never left the bar, still showing $(ipc lastMessage)"
}

# Assert one jq expression against the Open with dialog's state.
runfile_expect_openwith() {
    ipc openWithState | jq -e "$1" >/dev/null || fail "runfile: Open with $2, state $(ipc openWithState)"
}

# Pointer onto one card button through ydotool, since Hyprland's cursor move sends Qt no pointer frame.
runfile_point() {
    local button="$1" wx wy _ww _wh cx cy
    read -r wx wy _ww _wh < <(window_box) || fail "runfile: native window coordinates unavailable"
    read -r cx cy <<< "$(ipc runCardState | jq -r --arg b "$button" '.controls[] | select(.name == $b) | .centre')"
    [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "runfile: $button has no centre to point at"
    pointer_glide "$((wx + cx))" "$((wy + cy))" || fail "runfile: pointer move to $button failed"
    settle
}

# Pointer onto the card's own title, off every button, so a release there activates nothing.
runfile_point_title() {
    local wx wy _ww _wh rx ry rw _rh
    read -r wx wy _ww _wh < <(window_box) || fail "runfile: native window coordinates unavailable"
    read -r rx ry rw _rh <<< "$(ipc runCardState | jq -r '.rect')"
    [[ "$rx" =~ ^[0-9]+$ && "$ry" =~ ^[0-9]+$ && "$rw" =~ ^[0-9]+$ ]] || fail "runfile: the card reported no rect"
    pointer_glide "$((wx + rx + rw / 2))" "$((wy + ry + 12))" || fail "runfile: pointer move to the card title failed"
    settle
}

# Poll one card button's pointer state until it reads as wanted, so a hover or press shot is taken only once it holds.
runfile_wait_control() {
    local button="$1" field="$2" want="$3" seen="" poll
    for poll in $(seq 1 100); do
        seen=$(ipc runCardState 2>/dev/null || true)
        [[ "$(jq -r --arg b "$button" --arg f "$field" '.controls[] | select(.name == $b) | .[$f]' <<< "$seen" 2>/dev/null)" == "$want" ]] && return 0
        sleep 0.05
    done
    fail "runfile: $button never reported $field=$want, last state $seen"
}

# Hover, then press and release off the button, on every button the card draws; the ydotool button codes come from ui-captures-unlock.sh.
runfile_pointer_states() {
    local prefix="$1" button buttons
    buttons=$(ipc runCardState | jq -er '.buttons[]') || fail "runfile: $prefix lists no card buttons to point at"
    for button in $buttons; do
        runfile_point "$button"
        runfile_wait_control "$button" hovered true
        shot "$prefix-hover-$button"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click "$cap_unlock_left_down" >/dev/null 2>&1 || fail "runfile: pointer press on $button failed"
        runfile_wait_control "$button" pressed true
        shot "$prefix-pressed-$button"
        runfile_point_title
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click "$cap_unlock_left_up" >/dev/null 2>&1 || fail "runfile: pointer release off $button failed"
        runfile_wait_control "$button" pressed false
        runfile_expect '.opened == true' "a release off $button keeps the card"
    done
}

case_cap_runfile() {
    local dir="$fixture_root/cap-runfile" saved_path="$PATH" saved_data="${XDG_DATA_HOME-}" poll
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin/share/applications"
    # A gio that opens only plain.sh and launches only the sandbox handler, recording the rest: no editor or handler outside the test root starts.
    cat > "$dir/bin/gio" <<'SH'
#!/bin/sh
case "$1" in
open) printf '%s\n' "$2" >> "$(dirname "$0")/gio-open.argv"; case "$2" in */plain.sh) exit 0 ;; esac; exit 1 ;;
launch) case "$2" in */zzflea-appimage.desktop) exec /usr/bin/gio "$@" ;; esac; printf '%s\n' "$@" > "$(dirname "$0")/gio-launch.argv"; exit 0 ;;
esac
exec /usr/bin/gio "$@"
SH
    # A terminal launcher that records its argv and runs nothing.
    cat > "$dir/bin/xdg-terminal-exec" <<'SH'
#!/bin/sh
printf '%s\n' "$@" > "$(dirname "$0")/terminal.argv"
SH
    # The AppImage handler's program: it records its argv and opens nothing.
    cat > "$dir/bin/handler.sh" <<'SH'
#!/bin/sh
printf '%s\n' "$@" > "$(dirname "$0")/handler.argv"
SH
    chmod +x "$dir/bin/gio" "$dir/bin/xdg-terminal-exec" "$dir/bin/handler.sh"
    # The one harmless script Run may press: it writes one marker beside itself and nothing else.
    printf '#!/bin/sh\nprintf ok > "$(dirname "$0")/tool.ran"\n' > "$dir/tool.sh"
    chmod +x "$dir/tool.sh"
    printf '#!/bin/sh\nprintf ok > "$(dirname "$0")/changed.ran"\n' > "$dir/changed.sh"
    chmod +x "$dir/changed.sh"
    printf '#!/nonexistent/interp\nprintf ok > "$(dirname "$0")/badinterp.ran"\n' > "$dir/badinterp.sh"
    chmod 755 "$dir/badinterp.sh"
    printf '#!/bin/sh\necho ran >> "$(dirname "$0")/slow.ran"\ni=0\nwhile [ ! -e "$(dirname "$0")/slow.go" ] && [ $i -lt 600 ]; do sleep 0.05; i=$((i+1)); done\necho end >> "$(dirname "$0")/slow.end"\n' > "$dir/slow.sh"
    chmod +x "$dir/slow.sh"
    cp /bin/true "$dir/elfprog" || fail "runfile: no /bin/true to copy the ELF from"
    chmod +x "$dir/elfprog"
    # A fake type 2 AppImage: ELF magic with the AI tag and version 2 at offset 8, whatever the name.
    python3 - "$dir/app.AppImage" <<'PY'
import sys
open(sys.argv[1], 'wb').write(b'\x7fELF\x02\x01\x01\x00AI\x02' + b'\x00' * 200)
PY
    chmod +x "$dir/app.AppImage"
    # GLib falls back to any entry listing a type, so the AppImage handler's entry is written only at the handler step; the guard asks for the type this box gives the fixture.
    local app_type
    # Sample input, the one gio info line read: "  standard::content-type: application/vnd.appimage"
    app_type=$(LC_ALL=C XDG_DATA_HOME="$dir/bin/share" /usr/bin/gio info -a standard::content-type "$dir/app.AppImage" 2>/dev/null | sed -n 's/^ *standard::content-type: //p')
    [[ -n "$app_type" ]] || fail "runfile: gio gave the fake AppImage no content type"
    LC_ALL=C XDG_DATA_HOME="$dir/bin/share" /usr/bin/gio mime "$app_type" 2>/dev/null | grep -q '^Default application' \
        && fail "runfile: this box already names a $app_type default, so the no-handler card cannot be shown"
    # A shebang script without its bit; it writes a marker only if something ran it.
    printf '#!/bin/sh\nprintf ok > "$(dirname "$0")/plain.ran"\n' > "$dir/plain.sh"
    chmod 644 "$dir/plain.sh"
    # Binary bytes no type claims by content or name, so Open with draws the board's All applications state.
    printf '\000\001\002\003\376\377\000\020\000\001\002\003\376\377\000\020' > "$dir/data.bin"
    seed_ui_state "$fixture_root/cap-runfile-state" '{"keys":"default","view":"list"}'
    export PATH="$dir/bin:$PATH" XDG_DATA_HOME="$dir/bin/share"
    launch "$dir"
    export PATH="$saved_path"
    if [[ -n "$saved_data" ]]; then export XDG_DATA_HOME="$saved_data"; else unset XDG_DATA_HOME; fi
    wait_listing 9
    cap_resize 1100 700
    # A script with a shebang: Cancel, Open as Text and Run in Terminal, ring on the default; Shift relabels it Run.
    runfile_open tool.sh
    runfile_expect '.opened == true and .kind == "script" and (.buttons | join(",") == "cancel,text,terminal") and .focus == "terminal" and .default == "terminal" and .title == "tool.sh is a program."' "script default"
    shot cap-runfile-script-default
    runfile_shift 1 Run
    shot cap-runfile-script-shift-run
    runfile_shift 0 "Run in Terminal"
    runfile_focus text
    runfile_expect '.focus == "text" and .default == "terminal"' "script focus text"
    shot cap-runfile-script-focus-text
    runfile_focus cancel
    runfile_expect '.focus == "cancel" and .default == "terminal"' "script focus cancel"
    shot cap-runfile-script-focus-cancel
    # Backtab from Cancel comes round to Run in terminal, and Tab from there back to Cancel.
    key -M shift -k Tab -m shift >/dev/null
    settle
    runfile_expect '.focus == "terminal"' "script Backtab wraps to terminal"
    shot cap-runfile-script-backtab-wrap
    key -k Tab >/dev/null
    settle
    runfile_expect '.focus == "cancel"' "script Tab wraps to cancel"
    shot cap-runfile-script-tab-wrap
    runfile_pointer_states cap-runfile-script
    runfile_close
    # Opening the card spawns nothing: the marker may only appear after a deliberate Run below.
    [[ ! -e "$dir/tool.ran" ]] || fail "runfile: tool.sh spawned on open, before any Run"
    # Open as text hands the script to the text editor by gio launch, recorded by the stub instead of started.
    runfile_open tool.sh
    runfile_focus text
    key -k Return >/dev/null
    runfile_wait_card false
    for poll in $(seq 1 200); do [[ -s "$dir/bin/gio-launch.argv" ]] && break; sleep 0.05; done
    [[ "$(sed -n 1p "$dir/bin/gio-launch.argv" 2>/dev/null)" == launch && "$(sed -n 3p "$dir/bin/gio-launch.argv")" == "$(realpath "$dir/tool.sh")" ]] \
        || fail "runfile: Open as text launched [$(tr '\n' ' ' < "$dir/bin/gio-launch.argv" 2>/dev/null)], bar says $(ipc lastMessage)"
    printf 'RUNFILE text editor=%s\n' "$(sed -n 2p "$dir/bin/gio-launch.argv")"
    # Run in terminal hands the script to the OEM terminal launcher, recorded by the stub instead of opened.
    runfile_open tool.sh
    runfile_focus terminal
    key -k Return >/dev/null
    runfile_wait_card false
    for poll in $(seq 1 200); do [[ -s "$dir/bin/terminal.argv" ]] && break; sleep 0.05; done
    [[ "$(tail -n 2 "$dir/bin/terminal.argv" 2>/dev/null | tr '\n' ' ')" == "-e $(realpath "$dir/tool.sh") " ]] \
        || fail "runfile: Run in terminal asked for [$(tr '\n' ' ' < "$dir/bin/terminal.argv" 2>/dev/null)], bar says $(ipc lastMessage)"
    [[ ! -e "$dir/tool.ran" ]] || fail "runfile: the terminal stub ran tool.sh"
    # An ELF: no Open as text, since a binary is not text.
    runfile_open elfprog
    runfile_expect '.opened == true and .kind == "elf" and (.buttons | join(",") == "cancel,terminal") and .focus == "terminal" and .default == "terminal" and .title == "elfprog is a program."' "elf default"
    shot cap-runfile-elf-default
    runfile_shift 1 Run
    shot cap-runfile-elf-shift-run
    runfile_shift 0 "Run in Terminal"
    runfile_focus cancel
    shot cap-runfile-elf-focus-cancel
    runfile_close
    # An AppImage with no handler: Open as text stays off, Run in terminal stays the default.
    runfile_open app.AppImage
    runfile_expect '.opened == true and .kind == "appimage" and (.buttons | join(",") == "cancel,terminal") and .focus == "terminal" and .default == "terminal" and .title == "app.AppImage is a program."' "appimage default"
    shot cap-runfile-appimage-default
    runfile_shift 1 Run
    shot cap-runfile-appimage-shift-run
    runfile_shift 0 "Run in Terminal"
    runfile_focus cancel
    runfile_expect '.focus == "cancel" and .default == "terminal"' "appimage focus cancel"
    shot cap-runfile-appimage-focus-cancel
    runfile_close
    # A script without its bit opens with its handler like any document, spawns nothing of its own and draws no card.
    seek_row_named plain.sh || fail "runfile: no row named plain.sh"
    key -k Return >/dev/null
    for poll in $(seq 1 100); do grep -qxF "$(realpath "$dir/plain.sh")" "$dir/bin/gio-open.argv" 2>/dev/null && break; sleep 0.05; done
    grep -qxF "$(realpath "$dir/plain.sh")" "$dir/bin/gio-open.argv" 2>/dev/null \
        || fail "runfile: plain.sh never reached its handler, bar says $(ipc lastMessage)"
    settle
    runfile_expect '.opened == false' "plain.sh opens no card"
    [[ ! -e "$dir/plain.ran" ]] || fail "runfile: plain.sh ran itself"
    [[ "$(stat -c '%a' "$dir/plain.sh")" == "644" ]] || fail "runfile: opening plain.sh changed its mode to $(stat -c '%a' "$dir/plain.sh")"
    shot cap-runfile-plain-handler
    # The context menu's Make executable is the one route to the bit, and one undo restores it.
    click_row "$(row_index_of plain.sh)" right
    for poll in $(seq 1 40); do
        [[ "$(ipc menuState | jq -r '[.entries[] | select(.action == "makeExecutable" and .disabled != true)] | length')" == 1 ]] && break
        sleep 0.25
    done
    menus_seek makeExecutable
    shot cap-runfile-menu-make
    key -k Return >/dev/null
    wait_message "Made it executable. · z undoes"
    [[ "$(stat -c '%a' "$dir/plain.sh")" == "744" ]] \
        || fail "runfile: plain.sh stayed $(stat -c '%a' "$dir/plain.sh"), not 744"
    shot cap-runfile-made-executable
    key z >/dev/null
    wait_message "Undid the permissions"
    [[ "$(stat -c '%a' "$dir/plain.sh")" == "644" ]] \
        || fail "runfile: undo left $(stat -c '%a' "$dir/plain.sh"), not 644"
    shot cap-runfile-undid-executable
    # A file no app claims opens the approved Open with dialog instead of an error.
    seek_row_named data.bin || fail "runfile: no row named data.bin"
    key -k Return >/dev/null
    runfile_wait_openwith true
    settle
    # The registry is merged from the installed apps, so only one app row and eyebrow-shaped nulls are asserted.
    [[ "$(ipc openWithState | jq -er '[.rows[] | select(.label != null)] | length')" -ge 1 ]] \
        || fail "runfile: Open with offers no application for data.bin"
    [[ "$(ipc openWithState | jq -er '[.rows[] | select(.label == null and .eyebrow == null)] | length')" == "0" ]] \
        || fail "runfile: Open with draws an app row with no name, which is a product defect"
    # The board state: one type for card and dialog, so no registered group, and the search field holds focus on the first app.
    runfile_expect_openwith '.mime == "application/octet-stream" and ([.rows[] | select(.eyebrow) | .eyebrow] == ["All applications"])' "is not in the All applications state"
    runfile_expect_openwith '.cursor == 0 and .search == "" and ([.controls[] | select(.name == "Field")][0].focused)' "did not open on the search field"
    printf 'RUNFILE openwith kind=%s\n' "$(ipc openWithState | jq -r '.kind')"
    shot cap-runfile-openwith
    # j types into the focused search rather than moving the list.
    key j >/dev/null
    settle
    runfile_expect_openwith '.search == "j"' "did not take j as search text"
    key -k BackSpace >/dev/null
    settle
    runfile_expect_openwith '.search == ""' "did not clear its search"
    key -k Escape >/dev/null
    runfile_wait_openwith false
    settle
    # The one allowed Run: the harmless script writes its marker and the card closes silent.
    runfile_open tool.sh
    runfile_run
    runfile_wait_card false
    for poll in $(seq 1 200); do [[ -s "$dir/tool.ran" ]] && break; sleep 0.05; done
    [[ -s "$dir/tool.ran" ]] || fail "runfile: Run never wrote the marker, bar says $(ipc lastMessage)"
    printf 'RUNFILE ran marker=%s\n' "$(cat "$dir/tool.ran")"
    runfile_not_terminal
    # A file changed since the card asked is refused instead of run.
    runfile_open changed.sh
    printf '# touched\n' >> "$dir/changed.sh"
    runfile_run
    wait_message "That file changed after Flea checked it, so it was not run."
    [[ ! -e "$dir/changed.ran" ]] || fail "runfile: the changed file still ran"
    runfile_not_terminal
    shot cap-runfile-refused-changed
    runfile_dismiss_error
    # A program the system cannot spawn fails at Run; it is built before the ask so the card names it as Run meets it.
    runfile_open badinterp.sh
    runfile_run
    wait_message "Nothing on this system could run that file."
    [[ ! -e "$dir/badinterp.ran" ]] || fail "runfile: the bad-interpreter file still ran"
    runfile_not_terminal
    shot cap-runfile-refused-spawn
    runfile_dismiss_error
    # A second Run while the slow one still runs starts a second copy: each spawn appends one line as it starts.
    runfile_open slow.sh
    # The card opened, nothing started yet: the marker may only land after the Run below.
    [[ ! -e "$dir/slow.ran" ]] || fail "runfile: slow.sh spawned on open, before any Run"
    runfile_run
    runfile_wait_card false
    for poll in $(seq 1 200); do [[ -s "$dir/slow.ran" ]] && break; sleep 0.05; done
    [[ "$(wc -l < "$dir/slow.ran" 2>/dev/null)" == 1 ]] || fail "runfile: the first run never landed, bar says $(ipc lastMessage)"
    runfile_not_terminal
    # The lane is per-press by detached-launch design: flea --run spawns and exits, so a second Run starts a second copy instead of refusing one already running.
    runfile_open slow.sh
    runfile_run
    runfile_wait_card false
    for poll in $(seq 1 200); do [[ "$(wc -l < "$dir/slow.ran")" == 2 ]] && break; sleep 0.05; done
    [[ "$(wc -l < "$dir/slow.ran")" == 2 ]] || fail "runfile: the second run never landed, bar says $(ipc lastMessage)"
    runfile_not_terminal
    # Each copy holds until slow.go exists, so no end line yet proves the first still ran when the second started.
    [[ ! -e "$dir/slow.end" ]] || fail "runfile: the first copy ended before the second Run"
    touch "$dir/slow.go"
    # The desktop now names a default for the AppImage type, so the card offers that application by its own Name first.
    printf '[Desktop Entry]\nType=Application\nName=AppImage Manager\nExec=%s %%f\nMimeType=application/vnd.appimage;application/x-iso9660-appimage;\n' "$dir/bin/handler.sh" \
        > "$dir/bin/share/applications/zzflea-appimage.desktop"
    printf '[Default Applications]\napplication/vnd.appimage=zzflea-appimage.desktop\napplication/x-iso9660-appimage=zzflea-appimage.desktop\n' \
        > "$dir/bin/share/applications/mimeapps.list"
    # The default must be the sandbox entry, not one a user config outranks it with.
    LC_ALL=C XDG_DATA_HOME="$dir/bin/share" /usr/bin/gio mime application/vnd.appimage 2>/dev/null | grep -q "^Default application.*zzflea-appimage.desktop" \
        || fail "runfile: the AppImage default is not the sandbox entry: $(LC_ALL=C XDG_DATA_HOME="$dir/bin/share" /usr/bin/gio mime application/vnd.appimage 2>&1 | head -1)"
    runfile_open app.AppImage
    runfile_expect '.opened == true and .kind == "appimage" and (.buttons | join(",") == "cancel,handler,terminal") and .focus == "handler" and .default == "handler" and ([.controls[] | select(.name == "handler")][0] | .label == "AppImage Manager" and .primary)' "appimage handler default"
    shot cap-runfile-appimage-handler-default
    runfile_pointer_states cap-runfile-appimage-handler
    runfile_focus handler
    key -k Return >/dev/null
    runfile_wait_card false
    for poll in $(seq 1 200); do [[ -s "$dir/bin/handler.argv" ]] && break; sleep 0.05; done
    [[ "$(cat "$dir/bin/handler.argv" 2>/dev/null)" == "$(realpath "$dir/app.AppImage")" ]] \
        || fail "runfile: the handler got [$(cat "$dir/bin/handler.argv" 2>/dev/null)], bar says $(ipc lastMessage)"
    shot cap-runfile-appimage-handler-opened
    # the generic line follows a launcher failure that is none of spawn, noexec or a change, which no fixture makes
    printf 'CAP SKIP runfile-refused-other: That program could not be run. needs a launcher failure no fixture makes\n'
    # needs a noexec volume, driven by the controller
    printf 'CAP SKIP runfile-refused-noexec: a noexec volume cannot be made safely here\n'
    printf 'CAP_RUNFILE script=12 elf=3 appimage=3 handler=8 plain=1 menu=3 openwith=1 text=1 terminal=1 run=2 refused=2\n'
    kill_flea
}
