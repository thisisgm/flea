#!/usr/bin/env bash
# tests/ui-captures-drops.sh cap_drops_wait_target and undo_copy against ipc doubles: a leg lands on its own whole drag sentence, on no other line, and never on the floor signal.
set -u
cd "$(dirname "$0")/.." || exit 1

failed=0
ok() { printf 'ok   %s\n' "$*"; }
bad() { printf 'FAIL %s\n' "$*"; failed=1; }

MARKER=.flea-test-sandbox
box=$(mktemp -d) || exit 1
: > "$box/$MARKER"
# Removes only an absolute, non-empty root that still carries its marker.
cleanup() {
    case $box in
        /?*) [ -f "$box/$MARKER" ] && rm -rf -- "$box" ;;
    esac
}
trap cleanup EXIT

# Every wait helper and the state they read, cut out of the case file as shipped; the settled and rename waits are other probes.
eval "$(sed -n -e '/^cap_drops_wait_[a-z_]*=/p' -e '/^cap_drops_wait_[a-z_]*()/,/^}/p' -e '/^cap_drops_undo_copy()/,/^}/p' tests/ui-captures-drops.sh)"

move_line='Move 1 item to sub · ctrl copies, shift moves'
copy_line='Copy 1 item to sub'

# Sample input, ipc statusActivityState: {"activities":[{"id":0,"text":"Copy 1 item to sub","running":false,...}],...}, a drag line always id 0.
# d_text is the drag line, d_running its transfer flag, d_after the poll it first shows on, d_other a second activity listed before it, d_floor what listingDropActive answers.
run() {
    local call=$1
    shift
    (
        polls="$box/polls.$BASHPID"
        sleep() { command sleep 0.05; }
        fail() { printf 'FAIL: %s\n' "$*"; exit 1; }
        ipc() {
            if [ "$1" = listingDropActive ]; then printf '%s\n' "$d_floor"; return 0; fi
            [ "$1" = statusActivityState ] || return 1
            local n=0 shown=true
            [ ! -f "$polls" ] || n=$(<"$polls")
            n=$((n + 1))
            printf '%s\n' "$n" > "$polls"
            [ "$n" -ge "$d_after" ] || shown=false
            jq -nc --arg other "$d_other" --arg text "$d_text" --argjson running "$d_running" --argjson shown "$shown" '
                {activities: [(if $other != "" then {id: 7, text: $other, running: false, cancelling: false, ownerPath: "/d", ownerFocused: true} else empty end),
                              (if $shown then {id: 0, text: $text, running: $running, cancelling: false, ownerPath: "/d", ownerFocused: true} else empty end)],
                 errors: 0, notice: ""}'
        }
        d_text="" d_running=false d_after=1 d_other="" d_floor=false
        "$@"
        # SECONDS counts whole seconds, so 3 leaves at least 2 s of polls for the late line.
        cap_drops_wait_s=3
        eval "$call" || exit 1
        printf 'landed\n'
    )
}
target() { printf 'cap_drops_wait_target "%s"' "$1"; }

out=$(run "$(target "$move_line")" eval "d_text='$move_line'")
[ "$out" = landed ] && ok "leg a1 lands on the move sentence a plain lift shows" || bad "the move sentence must land, got: $out"

out=$(run "$(target "$copy_line")" eval "d_text='$copy_line'")
[ "$out" = landed ] && ok "leg a2 lands on the copy sentence a ctrl lift shows" || bad "the copy sentence must land, got: $out"

out=$(run "$(target "$copy_line")" eval "d_text='$copy_line' d_after=4")
[ "$out" = landed ] && ok "a line that shows on the fourth poll lands" || bad "a late line must land, got: $out"

out=$(run "$(target "$move_line")" eval "d_other='Copied 1 item' d_text='$move_line'")
[ "$out" = landed ] && ok "a second activity ahead of the drag line does not hide it" || bad "the drag line beside another activity must land, got: $out"

out=$(run "$(target "$copy_line")" eval "d_text='$move_line'")
[ "$out" = "FAIL: cap_drops: no drag line reading \"$copy_line\" (status lines: $move_line)" ] \
    && ok "the move sentence never answers the copy leg and the failure names what stood" || bad "a move line must not answer the copy leg, got: $out"

out=$(run "$(target "$move_line")" eval "d_text='$copy_line'")
[ "$out" = "FAIL: cap_drops: no drag line reading \"$move_line\" (status lines: $copy_line)" ] \
    && ok "the copy sentence never answers the move leg" || bad "a copy line must not answer the move leg, got: $out"

out=$(run "$(target "$copy_line")" eval "d_text='$copy_line · too wide to drag out'")
[ "$out" = "FAIL: cap_drops: no drag line reading \"$copy_line\" (status lines: $copy_line · too wide to drag out)" ] \
    && ok "a line with the reach note is not the leg's sentence" || bad "a reach note must not answer, got: $out"

out=$(run "$(target "$copy_line")" eval "d_text='$copy_line' d_running=true")
[ "$out" = "FAIL: cap_drops: no drag line reading \"$copy_line\" (status lines: none)" ] \
    && ok "a running transfer carrying the sentence is not a drag line" || bad "a running activity must not answer, got: $out"

out=$(run "$(target "$copy_line")" eval "d_other='$copy_line' d_after=99")
[ "$out" = "FAIL: cap_drops: no drag line reading \"$copy_line\" (status lines: $copy_line)" ] \
    && ok "the sentence on another activity's id is not the drag line" || bad "a line on a non-zero id must not answer, got: $out"

out=$(run "$(target "$copy_line")" eval "d_text='$copy_line' d_after=99")
[ "$out" = "FAIL: cap_drops: no drag line reading \"$copy_line\" (status lines: none)" ] \
    && ok "no activity at all never lands" || bad "an empty bar must not land, got: $out"

# A case that holds the pointer names its release, which runs before the timeout fails and not on a landed line.
out=$(run "$(target "$copy_line")" eval "d_text='$move_line'; abandon() { echo released; }; cap_drops_wait_abort=abandon")
[ "$out" = "released
FAIL: cap_drops: no drag line reading \"$copy_line\" (status lines: $move_line)" ] \
    && ok "a timed out wait runs the case's release before it fails" || bad "a timeout must release the pointer first, got: $out"
out=$(run "$(target "$copy_line")" eval "d_text='$copy_line'; abandon() { echo released; }; cap_drops_wait_abort=abandon")
[ "$out" = landed ] && ok "a landed line never runs the release" || bad "a landed line must not release the pointer, got: $out"
[ "$(grep -c 'cap_drops_wait_abort=cap_dropmods_abandon' tests/ui-captures-dropmods.sh)" = 1 ] \
    && ok "cap_dropmods names its release for the shared wait" || bad "cap_dropmods does not name its release for the shared wait"

# Sample input, case_cap_drops: the five hover waits, in order, are the lines that wait on a drag target.
calls=()
while IFS= read -r line; do calls+=("${line# }"); done < <(sed -n '/^case_cap_drops()/,/^}/p' tests/ui-captures-drops.sh | grep -E '^    cap_drops_wait_' | grep -Ev '^    cap_drops_wait_(settled|rename|column) ')
photos_line='Move 1 item to Photos · ctrl copies, shift moves'
crumb_line='Move 1 item to outer/ · ctrl copies, shift moves'
columns_line='Move 1 item to deep · ctrl copies, shift moves'
held_line='Move 1 item to a folder · ctrl copies, shift moves'
sentences=("$photos_line" "$photos_line" "$crumb_line" "$columns_line" "$held_line")
# Another target's sentence for each leg, never its own.
others=("$crumb_line" "$crumb_line" "$columns_line" "$photos_line" "$columns_line")
legs=("the Photos row" "the Photos row held for the hover open" "the parent crumb" "the Columns parent row" "the Columns held row p2.txt")
[ "${#calls[@]}" = 5 ] && ok "the case waits on five drag targets" || bad "case_cap_drops has ${#calls[@]} hover waits, not 5"

# Each target's own wait, run as the case writes it, over a floor that is dark by design because a row, a crumb or a column took the drag above it.
for i in 0 1 2 3 4; do
    out=$(run "${calls[$i]:-true}" eval "d_floor=false d_text='${sentences[$i]}'")
    [ "$out" = landed ] && ok "${legs[$i]} lands on its own sentence over a dark floor" || bad "${legs[$i]} must land on '${sentences[$i]}' over a dark floor, got: $out"
    out=$(run "${calls[$i]:-true}" eval "d_floor=true d_text='${others[$i]}'")
    [ "$out" = "FAIL: cap_drops: no drag line reading \"${sentences[$i]}\" (status lines: ${others[$i]})" ] \
        && ok "${legs[$i]} stays red on another target's sentence, a lit floor or not" || bad "${legs[$i]} must stay red without its sentence, got: $out"
done
[ -z "$(grep -n 'listingDropActive' tests/ui-captures-drops.sh)" ] \
    && ok "the case never reads the floor signal" || bad "the case still reads listingDropActive"

# One definition of the sentence wait, shared by both cases.
[ "$(grep -h '^cap_[a-z_]*wait_target()' tests/ui-captures-drops.sh tests/ui-captures-dropmods.sh | wc -l)" = 1 ] \
    && ok "the sentence wait is defined once" || bad "the sentence wait is defined more than once"
[ "$(grep -c 'cap_drops_wait_target' tests/ui-captures-dropmods.sh)" = 6 ] \
    && ok "cap_dropmods waits through the shared helper" || bad "cap_dropmods does not call the shared wait six times, the four lift legs and the two probe legs"

# Leg 1 runs with the setting on: a steer before the shot and another before the release each restart the 700 ms rest that opens Photos.
seq=$(sed -n '/^case_cap_drops()/,/^}/p' tests/ui-captures-drops.sh | sed -n '/cap_drops_wait_target "Move 1 item to Photos/,/cap_drops_release/{p;/cap_drops_release/q}' | grep -v '^ *#' | awk '{print $1}' | paste -sd, -)
[ "$seq" = "cap_drops_wait_target,cap_drops_steer,shot,cap_drops_steer,cap_drops_release" ] \
    && ok "leg 1 steers again before its shot and before its release" || bad "leg 1 does not restart the rest around its shot and release: $seq"

# The undo waits for the drop to land before it presses z, and only that drop's path decides.
gone="$box/Photos/a.txt"
mkdir -p "$box/Photos"
undo() {
    (
        sleep() { command sleep 0.05; }
        fail() { printf 'FAIL: %s\n' "$*"; exit 1; }
        key() { printf 'key %s\n' "$*" >> "$box/keys"; if [ "$d_undoes" = yes ]; then rm -f "$gone"; fi; }
        # The failure line reads the ipc message and searches the fixture root, so both exist here under set -u.
        ipc() { return 0; }
        fixture_root="$box"
        d_undoes=yes
        "$@"
        cap_drops_wait_s=3
        cap_drops_undo_copy "$gone"
        printf 'undone\n'
    )
}
: > "$box/keys"
: > "$gone"
out=$(undo true)
[ "$out" = undone ] && [ "$(cat "$box/keys")" = "key z" ] && ok "a landed drop is undone with z" || bad "a landed drop must be undone with one z, got: $out / $(cat "$box/keys")"

: > "$box/keys"
rm -f "$gone"
out=$(undo true 2>&1)
[ "$out" = "FAIL: cap_drops: the drop never landed $gone, so z has no drop to undo (said ; found at )" ] && [ ! -s "$box/keys" ] \
    && ok "a drop that never landed fails before z, which would undo another leg" || bad "a missing drop must fail before z, got: $out / $(cat "$box/keys")"

: > "$box/keys"
: > "$gone"
out=$(undo eval "d_undoes=no")
[ "$out" = "FAIL: cap_drops: undo left $gone behind" ] && ok "an undo that leaves the file fails by path" || bad "an undo that leaves the file must fail, got: $out"

[ "$failed" = 0 ] || exit 1
printf 'DROPMODSWAIT DONE\n'
