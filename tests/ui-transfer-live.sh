#!/usr/bin/env bash
# Sourced by ui.sh after ui-menus.sh, whose menus_expect, menus_shot and menus_same_file this case uses.
# shellcheck disable=SC2034,SC2154 # ui.sh supplies state; sourced helpers consume dynamically scoped locals.

# One MiB, the unit every payload size below is counted in.
transferlive_mib=$((1024 * 1024))
# Microseconds in a second, for the rates the throughput sample divides out.
transferlive_us_per_s=1000000
# Milliseconds in a second, for the rates, spans and times left the card is checked against.
transferlive_ms_per_s=1000
# ui/js/Format.js BYTES_PER_UNIT: each SI step is a thousand of the one below.
transferlive_si_step=1000
# The throughput sample: this much fresh random data, then one read/write copy of it.
transferlive_probe_mib=512
# src/backend/copyfile.rs CHUNK, so the sample copies in the same 256 KiB reads and writes Flea's own loop does.
transferlive_chunk=256K
# The payload aims at this long a copy at the sampled rate; the sample runs in page cache, so a real leg only runs longer.
transferlive_target_s=8
# Below this at the sampled rate the run says so, because the cap or the free space bounded the payload.
transferlive_goal_s=5
# The ceiling on one leg's payload, 12 GiB.
transferlive_cap_mib=$((12 * 1024))
# The smallest payload worth running: under 1 GiB a copy can settle before the card has two rate samples.
transferlive_floor_mib=1024
# Kept free on the fixture filesystem beyond this case's own peak, so the rest of the battery still has room.
transferlive_reserve_mib=$((4 * 1024))
# The peak on disk in half payloads: the full file, the half file, and one leg's full-size copy.
transferlive_peak_halves=5
# dd writers per payload file, each at its own offset, because one urandom reader alone is the slow part of the fill.
transferlive_writers=4
# One dd that runs longer than this is wedged rather than slow.
transferlive_dd_timeout_s=600
# src/backend/opsreq.rs PROGRESS_EVERY is 150 ms, so the harness asks at the backend's own beat.
transferlive_poll_s=0.15
# A leg still running after three minutes at any plausible rate is wedged.
transferlive_leg_ms=180000
# ui/TransferCard.qml rateWindowMs: the harness measures its own rate over at least the window the card measures over.
transferlive_rate_window_ms=2000
# How much earlier the card's window can start than the harness's: its span past 2 s, a publish beat, a wire beat, the call.
transferlive_rate_reach_ms=1000
# The card's rate is judged only where its own figure held within this ratio, in percent, over the last stretch below.
transferlive_rate_steady_percent=150
# Each of the card's figures averages its own last two seconds, so the figures of the last second cover every window judged.
transferlive_rate_steady_ms=1000
# The card's rate and time left may sit this far off the harness's own, in percent: the box measured 87 to 150, a doubled rate is 200.
transferlive_rate_band_percent=175
# Slack on a time left beyond what the card's own rounded figures allow, for a floor landing on a second's edge.
transferlive_left_margin_s=1
# Two full lines are judged as a countdown only when at least two seconds apart, so a fall is more than the one-second floor.
transferlive_countdown_least_ms=2000
# The widest two full lines judged as a countdown may be, so a slowdown hidden between them stays short.
transferlive_countdown_most_ms=4000
# The true time left has to fall by at least one second for every two of wall time between them.
transferlive_countdown_divisor=2
# A judged pair owes a true fall of half its span, and keeps a quarter of it for the rate's own wander, so rounding may hide a quarter.
transferlive_countdown_reserve=4
# With no pair judged, the settle bound stands in only for a last full line this close to the settle.
transferlive_settle_near_ms=3000
# The last full line's time left may exceed what was really left by this much: one poll gap and one floor.
transferlive_settle_slack_s=2
# The settings every leg starts from: the list view, the stock keys, and no preview reading a payload on its own.
transferlive_ui_state='{"view":"list","keys":"default","display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off","loadOn":"manual"},"menu":{"hidden":[]}}'

# ui/js/Format.js size(): whole bytes below 1000 (a rate keeps its fraction there), else one decimal and an SI unit.
transferlive_size_re='([0-9]+(\.[0-9]+)? B|[0-9]+\.[0-9] (kB|MB|GB|TB))'
# ui/js/Format.js duration(): m:ss below an hour, h:mm:ss from there.
transferlive_duration_re='([1-9][0-9]*:[0-5][0-9]:[0-5][0-9]|[1-5]?[0-9]:[0-5][0-9])'
# ui/js/Transfer.js byteParts() holding a total and a rate: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left".
transferlive_full_re="^${transferlive_size_re} of ${transferlive_size_re} · ${transferlive_size_re}/s · ${transferlive_duration_re} left\$"
# ui/js/Transfer.js byteParts() pushes " · <rate>/s" after the total or the moved word on every line it draws.
transferlive_rate_segment_re=" · (${transferlive_size_re})/s( · |\$)"
# Every shape byteParts() can draw: with a total or without one, and a time left only beside both.
transferlive_any_re="^${transferlive_size_re}( of ${transferlive_size_re}| copied| moved) · ${transferlive_size_re}/s( · ${transferlive_duration_re} left)?\$"

# ui/js/Format.js size() in integer arithmetic: the SI step, then tenths rounded half up, as toFixed rounds an exact tie.
transferlive_size() {
    local bytes=$1 scale=1 unit tenths
    if (( bytes < transferlive_si_step )); then
        printf '%s B\n' "$bytes"
        return 0
    fi
    for unit in kB MB GB TB; do
        scale=$((scale * transferlive_si_step))
        if (( bytes < scale * transferlive_si_step )); then break; fi
    done
    tenths=$(( (bytes * 20 + scale) / (2 * scale) ))
    printf '%s.%s %s\n' "$((tenths / 10))" "$((tenths % 10))" "$unit"
}

# Sample input: "4.2 GB", "734 B", or a slow rate's "523.456 B"; sets its bytes, and its slack: half the last digit it shows.
transferlive_figure() {
    local number="" unit="" scale whole fraction
    read -r number unit <<< "$1"
    case "$unit" in
        B) scale=1 ;;
        kB) scale=$transferlive_si_step ;;
        MB) scale=$((transferlive_si_step ** 2)) ;;
        GB) scale=$((transferlive_si_step ** 3)) ;;
        TB) scale=$((transferlive_si_step ** 4)) ;;
        *) return 1 ;;
    esac
    [[ "$number" =~ ^([0-9]+)(\.([0-9]+))?$ ]] || return 1
    whole=${BASH_REMATCH[1]} fraction=${BASH_REMATCH[3]}
    if (( scale == 1 )); then
        # A byte count prints whole and a rate below 1000 B/s prints its raw fraction, which the whole bytes stand in for.
        transferlive_figure_bytes=$((10#$whole))
        transferlive_figure_slack=$(( ${#fraction} > 0 ? 1 : 0 ))
        return 0
    fi
    [[ ${#fraction} -eq 1 ]] || return 1
    transferlive_figure_bytes=$(( (10#$whole * 10 + 10#$fraction) * scale / 10 ))
    transferlive_figure_slack=$(( scale / 20 ))
}

# Sample input: "1:03:05" or "0:04", ui/js/Format.js duration(); prints whole seconds.
transferlive_seconds() {
    local -a field=()
    IFS=: read -r -a field <<< "$1"
    case ${#field[@]} in
        2) printf '%s\n' "$((10#${field[0]} * 60 + 10#${field[1]}))" ;;
        3) printf '%s\n' "$((10#${field[0]} * 3600 + 10#${field[1]} * 60 + 10#${field[2]}))" ;;
        *) return 1 ;;
    esac
}

# Each y starts a detached flea --clip-own (src/clip/own.rs spawn_owner) that holds the selection until a newer copy cancels it, so kill_flea never drains it.
transferlive_stop_owners() {
    local pid pids result=0 deadline=$((SECONDS + drain_wait_s))
    local -a owned=() alive=()
    pids=$(pgrep -x flea) || result=$?
    (( result <= 1 )) || return 1
    for pid in $pids; do
        # Sample cmdline: "/usr/bin/flea\0--clip-own\0", which tr reads as "/usr/bin/flea --clip-own ".
        [[ "$(tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline")" == *" --clip-own " ]] || continue
        flea_process_owned "$pid" || continue
        owned+=("$pid")
        kill "$pid" 2>/dev/null || true
    done
    while (( ${#owned[@]} > 0 )); do
        alive=()
        for pid in "${owned[@]}"; do
            if flea_process_owned "$pid"; then alive+=("$pid"); fi
        done
        owned=("${alive[@]}")
        (( ${#owned[@]} > 0 )) || break
        (( SECONDS < deadline )) || fail "transferlive: clipboard owner ${owned[*]} survived for $drain_wait_s s after SIGTERM"
        sleep 0.05
    done
}

# Every exit path: stop any payload writer, drain the window and its clipboard owners, drop the private bus, and delete every payload byte.
transferlive_teardown() {
    local pid drained=1
    for pid in "${transferlive_writer_pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${transferlive_writer_pids[@]}"; do wait "$pid" 2>/dev/null || true; done
    transferlive_writer_pids=()
    ( kill_flea ) || drained=0
    ( transferlive_stop_owners ) || drained=0
    if [[ -n "$transferlive_bus_pid" ]]; then kill "$transferlive_bus_pid" 2>/dev/null || true; fi
    transferlive_bus_pid=""
    if [[ -n "$transferlive_root" ]]; then sandbox_remove "$transferlive_root"; fi
    (( drained )) || fail "transferlive: the window or a clipboard owner did not drain at teardown; the payload was deleted anyway"
}

# A diagnostic for a transfer that stalls: FLEA_TRANSFERLIVE_SYNC=1 waits for the fixture's own writeback before each launch.
transferlive_sync() {
    if [[ "${FLEA_TRANSFERLIVE_SYNC:-}" == 1 ]]; then
        sync -f "$transferlive_root" || fail "transferlive: sync -f $transferlive_root failed"
    fi
}

# Writes random bytes into each FILE MIB pair, several dd writers per file at their own offsets, and waits for all of them.
transferlive_fill() {
    local file mib part seek count pid status=0
    local -a pids=()
    while (( $# >= 2 )); do
        file=$1 mib=$2
        shift 2
        : > "$file" || fail "transferlive: cannot create $file"
        part=$(( (mib + transferlive_writers - 1) / transferlive_writers ))
        for (( seek = 0; seek < mib; seek += part )); do
            count=$(( mib - seek < part ? mib - seek : part ))
            timeout "$transferlive_dd_timeout_s" dd if=/dev/urandom of="$file" bs=1M seek="$seek" count="$count" \
                conv=notrunc iflag=fullblock status=none &
            pids+=("$!")
            transferlive_writer_pids+=("$!")
        done
    done
    for pid in "${pids[@]}"; do wait "$pid" || status=1; done
    transferlive_writer_pids=()
    (( status == 0 )) || fail "transferlive: a random payload writer failed or ran past $transferlive_dd_timeout_s s"
}

# Sizes the payload from a bounded sample of this filesystem, then writes it: full.bin and half.bin, random throughout.
transferlive_prepare() {
    local probe="$transferlive_root/probe" payload="$transferlive_root/payload" probe_bytes began=0 written=0 copied=0
    local blocks="" block_size="" avail_mib space_mib mib expected_ms filled_ms
    probe_bytes=$((transferlive_probe_mib * transferlive_mib))
    sandbox_scratch "$probe"
    began=$(date +%s%6N)
    timeout "$transferlive_dd_timeout_s" dd if=/dev/urandom of="$probe/random.bin" bs=1M count="$transferlive_probe_mib" \
        iflag=fullblock status=none || fail "transferlive: the random throughput sample could not be written"
    written=$(date +%s%6N)
    timeout "$transferlive_dd_timeout_s" dd if="$probe/random.bin" of="$probe/copy.bin" bs="$transferlive_chunk" status=none \
        || fail "transferlive: the copy throughput sample failed"
    copied=$(date +%s%6N)
    (( written > began && copied > written )) || fail "transferlive: the throughput sample took no measurable time"
    transferlive_random_rate=$(( probe_bytes * transferlive_us_per_s / (written - began) ))
    transferlive_copy_rate=$(( probe_bytes * transferlive_us_per_s / (copied - written) ))
    sandbox_remove "$probe"
    mib=$(( (transferlive_copy_rate * transferlive_target_s + transferlive_mib - 1) / transferlive_mib ))
    (( mib <= transferlive_cap_mib )) || mib=$transferlive_cap_mib
    (( mib >= transferlive_floor_mib )) || mib=$transferlive_floor_mib
    # Sample input: "118234567 4096", stat -f's blocks free to this user and the size they are counted in.
    read -r blocks block_size < <(stat -f -c '%a %S' "$transferlive_root")
    [[ "$blocks" =~ ^[0-9]+$ && "$block_size" =~ ^[0-9]+$ ]] \
        || fail "transferlive: stat -f gave no free-space figure for $transferlive_root: '$blocks' '$block_size'"
    avail_mib=$(( blocks * block_size / transferlive_mib ))
    space_mib=$(( (avail_mib - transferlive_reserve_mib) * 2 / transferlive_peak_halves ))
    (( mib <= space_mib )) || mib=$space_mib
    # Even, so the half payload is a whole number of MiB and the batch's two copied halves are exactly one full payload.
    mib=$(( mib / 2 * 2 ))
    (( mib >= transferlive_floor_mib )) \
        || fail "transferlive: $avail_mib MiB free on the fixture filesystem holds a $mib MiB payload at most, under the $transferlive_floor_mib MiB floor"
    transferlive_full_bytes=$((mib * transferlive_mib))
    transferlive_half_bytes=$((mib * transferlive_mib / 2))
    expected_ms=$(( transferlive_full_bytes * transferlive_ms_per_s / transferlive_copy_rate ))
    printf 'TRANSFERLIVE_SIZING random_write=%s/s copy=%s/s target=%ss cap=%sMiB free=%sMiB room=%sMiB payload=%sMiB expected_leg=%sms\n' \
        "$(transferlive_size "$transferlive_random_rate")" "$(transferlive_size "$transferlive_copy_rate")" \
        "$transferlive_target_s" "$transferlive_cap_mib" "$avail_mib" "$space_mib" "$mib" "$expected_ms"
    if (( expected_ms < transferlive_goal_s * transferlive_ms_per_s )); then
        printf 'TRANSFERLIVE_NOTE one leg lasts about %s ms at the sampled rate, under the %s s goal, because the cap or the free space bounds the payload\n' \
            "$expected_ms" "$transferlive_goal_s"
    fi
    mkdir "$payload" || fail "transferlive: no payload folder"
    began=$(date +%s%3N)
    transferlive_fill "$payload/full.bin" "$mib" "$payload/half.bin" "$((mib / 2))"
    filled_ms=$(( $(date +%s%3N) - began ))
    transferlive_sync
    [[ "$(stat -c %s "$payload/full.bin")" == "$transferlive_full_bytes" && "$(stat -c %s "$payload/half.bin")" == "$transferlive_half_bytes" ]] \
        || fail "transferlive: the payload files are not the sizes written: $(stat -c '%n %s' "$payload/full.bin" "$payload/half.bin")"
    printf 'TRANSFERLIVE_PAYLOAD full=%s (%s) half=%s (%s) random_fill_ms=%s writers_per_file=%s\n' \
        "$transferlive_full_bytes" "$(transferlive_size "$transferlive_full_bytes")" \
        "$transferlive_half_bytes" "$(transferlive_size "$transferlive_half_bytes")" "$filled_ms" "$transferlive_writers"
}

# A private session bus, the way case_collide runs: gio's trash answers inside this case's XDG_DATA_HOME, never the operator's.
transferlive_start_bus() {
    local -a bus=()
    # Sample output: "unix:path=/tmp/dbus-Ab12Cd34,guid=5f0e9a" then "48213", the address and the daemon's pid, one per line.
    mapfile -t bus < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
    [[ ${#bus[@]} -eq 2 && "${bus[1]}" =~ ^[0-9]+$ ]] || fail "transferlive: no private session bus, dbus-daemon printed: ${bus[*]}"
    transferlive_bus_pid=${bus[1]}
    export DBUS_SESSION_BUS_ADDRESS="${bus[0]}"
}

# The harness's own rate from the newest reading back to the latest one at least the given ms older: its low and high, rounding allowed for.
transferlive_harness_rate() {
    local at="$1" reach="$2" earlier span growth slack
    for (( earlier = at - 1; earlier >= 0; earlier-- )); do
        if (( transferlive_seen_at[at] - transferlive_seen_at[earlier] >= reach )); then break; fi
    done
    (( earlier >= 0 )) || return 1
    span=$(( transferlive_seen_at[at] - transferlive_seen_at[earlier] ))
    growth=$(( transferlive_seen_moved[at] - transferlive_seen_moved[earlier] ))
    slack=$(( transferlive_seen_moved_slack[at] + transferlive_seen_moved_slack[earlier] ))
    # Growth inside its own rounding gives no floor, a stall's reading, but its ceiling still holds a frozen card to its figures.
    transferlive_harness_low=0
    (( growth <= slack )) || transferlive_harness_low=$(( (growth - slack) * transferlive_ms_per_s / span ))
    transferlive_harness_high=$(( (growth + slack) * transferlive_ms_per_s / span ))
    transferlive_harness_span=$span
}

# The card's rate against the harness's own over the card's window and over one reaching as far back as the card's can.
transferlive_check_rate() {
    local leg="$1" line="$2" speed="$3" speed_slack="$4" at earlier least most evidence=0 low high spans percent
    transferlive_harness_judged=0
    at=$(( ${#transferlive_seen_at[@]} - 1 ))
    # Across a rate change the harness's windows and the card's start apart by up to a publish and a call, so only steady stretches are judged.
    least=$speed most=$speed
    for (( earlier = at - 1; earlier >= 0; earlier-- )); do
        (( transferlive_seen_at[at] - transferlive_seen_at[earlier] <= transferlive_rate_steady_ms )) || break
        evidence=$((evidence + 1))
        (( transferlive_seen_speed[earlier] >= least )) || least=${transferlive_seen_speed[earlier]}
        (( transferlive_seen_speed[earlier] <= most )) || most=${transferlive_seen_speed[earlier]}
    done
    # A line with no earlier figure inside the stretch proves nothing about steadiness, so it is skipped like an unsteady one.
    if (( evidence == 0 )); then
        transferlive_seen_rate_unproven=$((transferlive_seen_rate_unproven + 1))
        return 0
    fi
    if (( most * 100 > least * transferlive_rate_steady_percent )); then
        transferlive_seen_rate_unsteady=$((transferlive_seen_rate_unsteady + 1))
        return 0
    fi
    transferlive_harness_rate "$at" "$transferlive_rate_window_ms" || return 0
    low=$transferlive_harness_low high=$transferlive_harness_high spans=$transferlive_harness_span
    # The card's window can start at the transfer's zero-byte first sample, which no reading matches, so judge only past it.
    if ! transferlive_harness_rate "$at" "$((transferlive_rate_window_ms + transferlive_rate_reach_ms))"; then
        transferlive_seen_rate_unproven=$((transferlive_seen_rate_unproven + 1))
        return 0
    fi
    (( transferlive_harness_low >= low )) || low=$transferlive_harness_low
    (( transferlive_harness_high <= high )) || high=$transferlive_harness_high
    spans="$spans and $transferlive_harness_span"
    if (( (speed + speed_slack) * transferlive_rate_band_percent < low * 100 || (speed - speed_slack) * 100 > high * transferlive_rate_band_percent )); then
        fail "transferlive: $leg: the card says $(transferlive_size "$speed")/s, the moved figure grew $(transferlive_size "$low")/s to $(transferlive_size "$high")/s over $spans ms: [$line]"
    fi
    transferlive_harness_judged=1 transferlive_harness_line_low=$low transferlive_harness_line_high=$high
    transferlive_seen_rate_checks=$((transferlive_seen_rate_checks + 1))
    # The spread is reported against the harness's measurable windows; a ceiling alone has no middle to compare with.
    (( low > 0 )) || return 0
    transferlive_seen_rate_floors=$((transferlive_seen_rate_floors + 1))
    percent=$(( speed * 200 / (low + high) ))
    if (( transferlive_seen_rate_least < 0 || percent < transferlive_seen_rate_least )); then transferlive_seen_rate_least=$percent; fi
    if (( percent > transferlive_seen_rate_most )); then transferlive_seen_rate_most=$percent; fi
}

# The card's time left against its own figures: (total - moved) / rate in whole seconds, each figure's rounding allowed for.
transferlive_check_left() {
    local leg="$1" line="$2" total="$3" total_slack="$4" moved="$5" moved_slack="$6" speed="$7" speed_slack="$8" left="$9"
    local rest_low rest_high slowest low high
    rest_low=$(( total - moved - total_slack - moved_slack ))
    (( rest_low > 0 )) || rest_low=0
    rest_high=$(( total - moved + total_slack + moved_slack ))
    (( rest_high > 0 )) || rest_high=0
    slowest=$(( speed - speed_slack ))
    (( slowest > 0 )) || slowest=1
    low=$(( rest_low / (speed + speed_slack) - transferlive_left_margin_s ))
    high=$(( rest_high / slowest + transferlive_left_margin_s ))
    (( left >= low && left <= high )) \
        || fail "transferlive: $leg: the card says $left s left, where its own total, moved figure and rate put it at $low to $high s: [$line]"
    transferlive_seen_left_checks=$((transferlive_seen_left_checks + 1))
}

# The card's time left against the harness's own estimate, (total - moved) / the harness's rate, within the same band.
transferlive_check_left_harness() {
    local leg="$1" line="$2" total="$3" total_slack="$4" moved="$5" moved_slack="$6" left="$7" rest_low rest_high low high
    # A harness rate that rounds to nothing gives no bound to divide by.
    (( transferlive_harness_judged && transferlive_harness_line_low > 0 )) || return 0
    rest_low=$(( total - moved - total_slack - moved_slack ))
    (( rest_low > 0 )) || rest_low=0
    rest_high=$(( total - moved + total_slack + moved_slack ))
    (( rest_high > 0 )) || rest_high=0
    low=$(( rest_low * 100 / (transferlive_harness_line_high * transferlive_rate_band_percent) ))
    high=$(( rest_high * transferlive_rate_band_percent / (transferlive_harness_line_low * 100) ))
    (( left >= low && left <= high )) \
        || fail "transferlive: $leg: the card says $left s left, where the harness's own rate puts it at $low to $high s: [$line]"
    transferlive_seen_left_harness_checks=$((transferlive_seen_left_harness_checks + 1))
}

# Polls the card until the leg settles, checking every byte line's shape, total, rate and time left as it is drawn.
transferlive_watch() {
    local leg="$1" want_total="$2" state before after now deadline line text value unit rate left
    local head_re="^Copying [0-9]+ of $3( · ($4))?\$" moved moved_slack total total_slack speed speed_slack
    local -a field=()
    transferlive_seen_visible=0 transferlive_seen_polls=0 transferlive_seen_lines=0 transferlive_seen_totaled=0
    transferlive_seen_full=0 transferlive_seen_first="" transferlive_seen_last="" transferlive_seen_last_any=""
    transferlive_seen_early="" transferlive_seen_state="" transferlive_seen_settled_at=0
    transferlive_seen_rate_checks=0 transferlive_seen_rate_floors=0 transferlive_seen_rate_least=-1 transferlive_seen_rate_most=-1 transferlive_seen_left_checks=0
    transferlive_seen_rate_unsteady=0 transferlive_seen_rate_unproven=0 transferlive_seen_left_harness_checks=0
    transferlive_seen_at=() transferlive_seen_moved=() transferlive_seen_moved_slack=() transferlive_seen_speed=()
    transferlive_seen_full_at=() transferlive_seen_full_left=() transferlive_seen_full_line=()
    transferlive_seen_full_speed=() transferlive_seen_full_speed_slack=()
    deadline=$(( $(date +%s%3N) + transferlive_leg_ms ))
    while :; do
        before=$(date +%s%3N)
        state=$(ipc statusActivityState) || fail "transferlive: $leg: the activity observer failed"
        after=$(date +%s%3N)
        # The card answered somewhere inside the call, so the reading is dated at the call's middle.
        now=$(( (before + after) / 2 ))
        transferlive_seen_polls=$((transferlive_seen_polls + 1))
        # Sample state: {"activities":[{"text":"Copying 1 of 1 · big.bin","running":true}],"errors":0,"notice":"","transferCard":{"visible":true,"byteLine":"4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"}}
        mapfile -t field < <(jq -r '.transferCard.visible, (.activities | length), .errors, .transferCard.byteLine, .notice, (.activities[0].text // "")' <<< "$state")
        [[ ${#field[@]} -eq 6 ]] || fail "transferlive: $leg: unreadable activity state: $state"
        if [[ "${field[0]}" == true ]]; then transferlive_seen_visible=1; fi
        text=${field[5]}
        if [[ "$text" == Copying* && ! "$text" =~ $head_re ]]; then
            fail "transferlive: $leg: the headline reads [$text], which names no item this leg copies"
        fi
        line=${field[3]}
        if [[ -n "$line" ]]; then
            transferlive_seen_lines=$((transferlive_seen_lines + 1))
            transferlive_seen_last_any=$line
            [[ "$line" =~ $transferlive_any_re ]] || fail "transferlive: $leg: [$line] is no shape ui/js/Transfer.js byteParts draws"
            # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"; its first two words are the moved figure.
            read -r value unit _ <<< "$line"
            transferlive_figure "$value $unit" || fail "transferlive: $leg: unreadable moved figure in [$line]"
            moved=$transferlive_figure_bytes moved_slack=$transferlive_figure_slack
            if (( ${#transferlive_seen_moved[@]} > 0 && moved < transferlive_seen_moved[-1] )); then
                fail "transferlive: $leg: the moved figure went back, ${transferlive_seen_moved[-1]} bytes then [$line]"
            fi
            # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"; the rate is the size right before "/s", read only where that segment is.
            [[ "$line" =~ $transferlive_rate_segment_re ]] || fail "transferlive: $leg: a byte line without its rate, which byteParts always draws: [$line]"
            transferlive_figure "${BASH_REMATCH[1]}" || fail "transferlive: $leg: unreadable rate in [$line]"
            speed=$transferlive_figure_bytes speed_slack=$transferlive_figure_slack
            transferlive_seen_at+=("$now")
            transferlive_seen_moved+=("$moved")
            transferlive_seen_moved_slack+=("$moved_slack")
            transferlive_seen_speed+=("$speed")
            if [[ "$line" == *" of "* ]]; then
                # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s"; the total sits between " of " and the first " · ".
                total=${line#* of }
                total=${total%% · *}
                [[ "$total" == "$want_total" ]] || fail "transferlive: $leg: the card totals $total, not $want_total: [$line]"
                transferlive_figure "$total" || fail "transferlive: $leg: unreadable total in [$line]"
                total=$transferlive_figure_bytes total_slack=$transferlive_figure_slack
                transferlive_seen_totaled=$((transferlive_seen_totaled + 1))
            elif [[ -z "$transferlive_seen_early" ]]; then
                transferlive_seen_early=$line
            fi
            if [[ "$line" =~ $transferlive_full_re ]]; then
                transferlive_seen_full=$((transferlive_seen_full + 1))
                transferlive_seen_last=$line
                if [[ -z "$transferlive_seen_first" ]]; then
                    transferlive_seen_first=$line
                    menus_shot "transferlive-$leg"
                fi
                # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"; the time left is the word before " left".
                left=${line% left}
                left=$(transferlive_seconds "${left##* }") || fail "transferlive: $leg: unreadable time left in [$line]"
                transferlive_check_rate "$leg" "$line" "$speed" "$speed_slack"
                transferlive_check_left "$leg" "$line" "$total" "$total_slack" "$moved" "$moved_slack" "$speed" "$speed_slack" "$left"
                transferlive_check_left_harness "$leg" "$line" "$total" "$total_slack" "$moved" "$moved_slack" "$left"
                transferlive_seen_full_at+=("$now")
                transferlive_seen_full_left+=("$left")
                transferlive_seen_full_line+=("$line")
                transferlive_seen_full_speed+=("$speed")
                transferlive_seen_full_speed_slack+=("$speed_slack")
            fi
        fi
        # Settled: nothing running, the card down, and either a completion carrying its undo hint or an error.
        if [[ "${field[1]}" == 0 && "${field[0]}" == false ]] && [[ "${field[4]}" == *" · z undoes" || "${field[2]}" != 0 ]]; then
            transferlive_seen_state=$state
            transferlive_seen_settled_at=$now
            return 0
        fi
        (( now < deadline )) || fail "transferlive: $leg: no settled transfer after $((transferlive_leg_ms / transferlive_ms_per_s)) s: $state"
        sleep "$transferlive_poll_s"
    done
}

# A real countdown: full lines 2 to 4 s apart whose rate held must lose half the wall time between, and the last must not overpromise.
transferlive_check_countdown() {
    local leg="$1" last earlier later span fall pairs=0 remaining hidden near causes short=0 long=0 fell=0 hid=0
    last=$(( ${#transferlive_seen_full_at[@]} - 1 ))
    for (( earlier = 0; earlier < last; earlier++ )); do
        for (( later = earlier + 1; later <= last; later++ )); do
            span=$(( transferlive_seen_full_at[later] - transferlive_seen_full_at[earlier] ))
            if (( span < transferlive_countdown_least_ms )); then short=$((short + 1)); continue; fi
            if (( span > transferlive_countdown_most_ms )); then long=$((long + 1)); continue; fi
            # A falling rate may rightly hold or raise the time left, so only pairs whose shown rate held or rose are judged.
            if (( transferlive_seen_full_speed[later] < transferlive_seen_full_speed[earlier] )); then fell=$((fell + 1)); continue; fi
            # Equal figures can still hide a true fall of one rounding step, which costs up to that share of the earlier time left.
            hidden=$(( (transferlive_seen_full_speed[earlier] + transferlive_seen_full_speed_slack[earlier] - transferlive_seen_full_speed[later] + transferlive_seen_full_speed_slack[later])
                * (transferlive_seen_full_left[earlier] + 1) * transferlive_ms_per_s
                / (transferlive_seen_full_speed[earlier] > transferlive_seen_full_speed_slack[earlier] ? transferlive_seen_full_speed[earlier] - transferlive_seen_full_speed_slack[earlier] : 1) ))
            if (( hidden * transferlive_countdown_reserve > span )); then hid=$((hid + 1)); continue; fi
            fall=$(( transferlive_seen_full_left[earlier] - transferlive_seen_full_left[later] ))
            # The card floors to whole seconds, so a true fall of half the span reads as more than half the span less one second.
            (( (fall + 1) * transferlive_ms_per_s * transferlive_countdown_divisor > span )) \
                || fail "transferlive: $leg: the time left fell $fall s in $span ms at a steady rate, under half the wall time: [${transferlive_seen_full_line[earlier]}] then [${transferlive_seen_full_line[later]}]"
            pairs=$((pairs + 1))
        done
    done
    # Rounded up, so a copy that settled 300 ms after its last line had one whole second still to give.
    remaining=$(( (transferlive_seen_settled_at - transferlive_seen_full_at[last] + transferlive_ms_per_s - 1) / transferlive_ms_per_s ))
    (( transferlive_seen_full_left[last] <= remaining + transferlive_settle_slack_s )) \
        || fail "transferlive: $leg: the last full line promised ${transferlive_seen_full_left[last]} s but the copy settled within $remaining s: [${transferlive_seen_full_line[last]}]"
    causes="span short $short, span long $long, rate fell $fell, rounding hid $hid"
    near=$(( transferlive_seen_settled_at - transferlive_seen_full_at[last] ))
    # With no pair judged, the settle bound above is the only countdown proof, and it proves one only for a line close to the settle.
    if (( pairs == 0 )); then
        (( near <= transferlive_settle_near_ms )) \
            || fail "transferlive: $leg: no countdown pair was judged ($causes), and the last full line came $near ms before the settle, too early for the settle bound to stand in"
        transferlive_seen_countdown="settle-bound-only ($causes);"
    else
        transferlive_seen_countdown="pairs=$pairs ($causes);"
    fi
    transferlive_seen_countdown="$transferlive_seen_countdown last=[${transferlive_seen_full_line[last]}] settled=$((transferlive_seen_settled_at - transferlive_seen_full_at[last]))ms-after"
}

# A leg's end: the card seen and gone, its speed and time left proven, no error, and the leg's own completion.
transferlive_settled() {
    local leg="$1" said="$2"
    (( transferlive_seen_visible == 1 )) \
        || fail "transferlive: $leg: the copy settled before the card was ever seen; payload $(transferlive_size "$transferlive_full_bytes") at a sampled $(transferlive_size "$transferlive_copy_rate")/s outran the harness: $transferlive_seen_state"
    jq -e --arg said "$said" '(.activities | length) == 0 and (.transferCard.visible | not) and .errors == 0 and .notice == $said' <<< "$transferlive_seen_state" >/dev/null \
        || fail "transferlive: $leg: the transfer did not settle as [$said] with no card and no error: $transferlive_seen_state"
    (( transferlive_seen_full > 0 )) \
        || fail "transferlive: $leg: $transferlive_seen_lines byte lines and none carried a total, a rate and a time left together; the last was [$transferlive_seen_last_any]"
    (( transferlive_seen_moved[-1] > transferlive_seen_moved[0] )) \
        || fail "transferlive: $leg: the moved figure never grew, ${transferlive_seen_moved[0]} bytes first and ${transferlive_seen_moved[-1]} last"
    (( transferlive_seen_rate_floors > 0 )) \
        || fail "transferlive: $leg: no full line's rate was held to a floor: $transferlive_seen_rate_checks judged against a ceiling only, $transferlive_seen_rate_unsteady unsteady, $transferlive_seen_rate_unproven with no earlier figure in the steadiness window"
    transferlive_check_countdown "$leg"
    menus_expect collideState '.opened | not' "$leg: no collision card is left open"
    printf 'TRANSFERLIVE_LEG leg=%s polls=%s lines=%s totaled=%s full=%s rate_checks=%s rate_floors=%s rate_unsteady=%s rate_unproven=%s rate_vs_harness=%s%%..%s%% left_checks=%s left_vs_harness=%s first=[%s] last=[%s] countdown=%s notice=[%s]\n' \
        "$leg" "$transferlive_seen_polls" "$transferlive_seen_lines" "$transferlive_seen_totaled" "$transferlive_seen_full" \
        "$transferlive_seen_rate_checks" "$transferlive_seen_rate_floors" "$transferlive_seen_rate_unsteady" "$transferlive_seen_rate_unproven" "$transferlive_seen_rate_least" "$transferlive_seen_rate_most" \
        "$transferlive_seen_left_checks" "$transferlive_seen_left_harness_checks" \
        "$transferlive_seen_first" "$transferlive_seen_last" "$transferlive_seen_countdown" "$said"
}

# The HIG collision title is a full sentence and ends with a full stop (APL-DIALOG-16); the QML card drew none.
transferlive_title_stop() {
    [[ "$test_frontend" == gpui ]] && printf '.'
    return 0
}

# From the leg's source listing into its to folder, which is where every paste in this case lands.
transferlive_enter_to() {
    local dir="$1" count="$2"
    if [[ "$test_frontend" == gpui ]]; then
        key -k Escape >/dev/null
        [[ -z "$(ipc selectedIndices)" ]] || fail "transferlive: marks survived Escape before entering to"
    fi
    seek_row_named to
    key -k Return >/dev/null
    wait_path "$dir/to"
    wait_listing "$count"
}

# The destination holds exactly these names, in C order, and nothing a Keep both or a stray partial would add.
transferlive_holds() {
    local dir="$1" want="$2" listing seen
    # find on its own line, so the guard reads find's own status and not the last stage of a pipeline.
    listing=$(find "$dir" -mindepth 1 -maxdepth 1 -printf '%f\n') || fail "transferlive: cannot list $dir"
    seen=$(LC_ALL=C sort <<< "$listing" | tr '\n' ' ')
    [[ "$seen" == "$want " ]] || fail "transferlive: $dir holds [$seen], not [$want]"
}

# Leg one: one big file pasted into an empty folder, the card watched from its first sample to its last.
transferlive_one() {
    local dir="$transferlive_root/one" want
    sandbox_scratch "$dir"
    mkdir "$dir/to" || fail "transferlive: one: no destination folder"
    # A hard link to the full payload: the leg reads the same random bytes without writing them twice.
    ln "$transferlive_root/payload/full.bin" "$dir/big.bin" || fail "transferlive: one: the source could not be linked in"
    want=$(transferlive_size "$transferlive_full_bytes")
    launch "$dir"
    wait_listing 2
    seek_row_named big.bin
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.paths[0] | endswith("/one/big.bin")) and (.clipboard.cut | not)' \
        "one: big.bin is on the clipboard"
    transferlive_enter_to "$dir" 0
    key p >/dev/null
    transferlive_watch one "$want" 1 'big\.bin'
    transferlive_settled one "Copied 1 item · z undoes"
    transferlive_sample_one="[$transferlive_seen_first] [$transferlive_seen_last]"
    kill_flea
    transferlive_holds "$dir/to" "big.bin"
    menus_same_file "one: the copy is byte-identical to its source" "$dir/big.bin" "$dir/to/big.bin"
    sandbox_remove "$dir"
    transferlive_sync
}

# Leg two: three big files, one already in the destination, pasted with Skip; the batch total leaves the skipped one out.
transferlive_batch() {
    local dir="$transferlive_root/batch" want skipped every name
    sandbox_scratch "$dir"
    mkdir "$dir/to" || fail "transferlive: batch: no destination folder"
    # All three are the half payload: the two that copy total one full payload, the skipped one half, all three one and a half.
    for name in a.bin b.bin c.bin; do
        ln "$transferlive_root/payload/half.bin" "$dir/$name" || fail "transferlive: batch: $name could not be linked in"
    done
    printf 'there\n' > "$dir/to/b.bin" || fail "transferlive: batch: the colliding b.bin could not be written"
    want=$(transferlive_size "$((2 * transferlive_half_bytes))")
    skipped=$(transferlive_size "$transferlive_half_bytes")
    every=$(transferlive_size "$((3 * transferlive_half_bytes))")
    [[ "$want" != "$skipped" && "$want" != "$every" ]] \
        || fail "transferlive: batch: the copied total $want reads the same as the skipped one's $skipped or all three's $every"
    launch "$dir"
    wait_listing 4
    seek_row_named a.bin
    key v >/dev/null
    key J >/dev/null
    key J >/dev/null
    settle
    [[ "$(ipc selectedIndices)" == "$(row_index_of a.bin),$(row_index_of b.bin),$(row_index_of c.bin)" ]] \
        || fail "transferlive: batch: the three files are not selected, $(ipc selectedIndices) is"
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 3 and (.clipboard.cut | not)' "batch: the three files are on the clipboard"
    transferlive_enter_to "$dir" 1
    key p >/dev/null
    menus_expect collideState '.opened and .title == "b.bin already exists in to'"$(transferlive_title_stop)"'" and .names == ["b.bin"] and .more == ""
        and .focus == "keep" and ([.buttons[] | select(.name != "merge") | .visible] | all) and ([.buttons[] | select(.name == "merge") | .visible] == [false])' "batch: the paste asks once about b.bin, on Keep both, with every file button up and Merge hidden"
    key h >/dev/null
    menus_expect collideState '.opened and .focus == "replace"' "batch: h moves the focus to Replace"
    key h >/dev/null
    menus_expect collideState '.opened and .focus == "skip"' "batch: h again moves the focus to Skip"
    key -k Return >/dev/null
    transferlive_watch batch "$want" 3 'a\.bin|c\.bin'
    transferlive_settled batch "Copied 2 of 3 · 1 skipped · z undoes"
    (( transferlive_seen_totaled > 0 )) \
        || fail "transferlive: batch: no byte line ever carried the sweep's total; the last was [$transferlive_seen_last_any]"
    printf 'TRANSFERLIVE_BATCH_TOTAL of=[%s] skipped_only_would_read=[%s] all_three_would_read=[%s] totaled_lines=%s before_the_sweep=[%s]\n' \
        "$want" "$skipped" "$every" "$transferlive_seen_totaled" "$transferlive_seen_early"
    transferlive_sample_batch="[$transferlive_seen_first] [$transferlive_seen_last]"
    kill_flea
    transferlive_holds "$dir/to" "a.bin b.bin c.bin"
    [[ "$(cat "$dir/to/b.bin")" == there ]] || fail "transferlive: batch: Skip wrote over the b.bin already there"
    for name in a.bin c.bin; do
        menus_same_file "batch: $name is byte-identical to its source" "$dir/$name" "$dir/to/$name"
    done
    sandbox_remove "$dir"
    transferlive_sync
}

# Leg three: one big file onto a name that exists, with Replace; the old item lands in this case's own Trash.
transferlive_replace() {
    local dir="$transferlive_root/replace" want replace_trash
    sandbox_scratch "$dir"
    # GLib trashes to the home store or the fixture mount's top trash by device.
    replace_trash=$(sandbox_trash_files_dir "$dir") || fail "transferlive: replace: no GLib trash directory for $dir"
    mkdir "$dir/to" || fail "transferlive: replace: no destination folder"
    ln "$transferlive_root/payload/full.bin" "$dir/big.bin" || fail "transferlive: replace: the source could not be linked in"
    printf 'there\n' > "$dir/to/big.bin" || fail "transferlive: replace: the item to replace could not be written"
    want=$(transferlive_size "$transferlive_full_bytes")
    launch "$dir"
    wait_listing 2
    seek_row_named big.bin
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.paths[0] | endswith("/replace/big.bin")) and (.clipboard.cut | not)' \
        "replace: big.bin is on the clipboard"
    transferlive_enter_to "$dir" 1
    key p >/dev/null
    menus_expect collideState '.opened and .title == "big.bin already exists in to'"$(transferlive_title_stop)"'" and .names == ["big.bin"] and .more == ""
        and .focus == "keep" and ([.buttons[] | select(.name != "merge") | .visible] | all) and ([.buttons[] | select(.name == "merge") | .visible] == [false])' "replace: the paste asks about big.bin, on Keep both, with every file button up and Merge hidden"
    key h >/dev/null
    menus_expect collideState '.opened and .focus == "replace"' "replace: h moves the focus to Replace"
    trash_trap_own "$replace_trash" "$dir" "transferlive: replace" big.bin
    key -k Return >/dev/null
    transferlive_watch replace "$want" 1 'big\.bin'
    transferlive_settled replace "Copied 1 item · z undoes"
    transferlive_sample_replace="[$transferlive_seen_first] [$transferlive_seen_last]"
    kill_flea
    [[ "$(cat "$replace_trash/big.bin" 2>/dev/null)" == there ]] \
        || fail "transferlive: replace: the replaced big.bin is not in $replace_trash: $(ls -A "$replace_trash" 2>&1)"
    transferlive_holds "$dir/to" "big.bin"
    menus_same_file "replace: the incoming big.bin is byte-identical to its source" "$dir/big.bin" "$dir/to/big.bin"
    # A mount-top trash is shared, so only this run's own entry goes, each pair checked in the command.
    if [[ "$replace_trash" != "$XDG_DATA_HOME/Trash/files" ]]; then
        sandbox_trash_remove_own "$replace_trash" "$dir" big.bin \
            || fail "transferlive: replace: owned entries could not be removed from $replace_trash"
    fi
    sandbox_remove "$dir"
}
