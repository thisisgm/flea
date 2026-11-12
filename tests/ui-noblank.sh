# Defines the native listing-swap and searchnoblank cases; tests/ui.sh supplies the guarded fixture and IPC helpers.

# Stub delays: 20 ms swaps inside Swap.js CAP_MS, 120 ms falls back inside the mark's minimum, 2.5 s outlasts the cap and its reads.
noblank_held_delay_s=0.02
noblank_held_min_ms=20
noblank_brief_delay_s=0.12
noblank_slow_delay_s=2.5
# A search answer held past Swap.HOLD_MS: the live path arms no cap, so the rows must stay held anyway.
searchnoblank_query_delay_s=0.8
searchnoblank_query_hold_s=0.3
# The cap is a QTimer, which may fire up to 5 percent early at this interval.
noblank_cap_min_ms=62
# Swap.js MIN_SHOW_MS: a drawn mark stands this long before the rows replace it.
noblank_min_show_ms=100

# One read of ui/PaneSwap.qml describe() through ui/Ipc.qml swapState, the record noblank_expect parses.
noblank_state() {
    ipc swapState
}

# A listing that has landed in whichever view is up: the path, the count, and nothing left in flight.
noblank_landed() {
    local want_path="$1" want_total="$2" path=unavailable total=unavailable flight=unavailable
    for _attempt in $(seq 1 300); do
        path=$(ipc path 2>/dev/null || printf unavailable)
        total=$(ipc total 2>/dev/null || printf unavailable)
        flight=$(ipc listInFlight 2>/dev/null || printf true)
        [[ "$path" == "$want_path" && "$total" == "$want_total" && "$flight" == false ]] && return 0
        sleep 0.05
    done
    fail "noblank: the listing never landed on $want_path with $want_total rows; path=$path total=$total inFlight=$flight"
}

# The two records either side of one navigation, held to a jq condition over $b (before) and $a (after).
# Sample input, each of $b and $a: {"holding":false,"fellBack":false,"holds":2,"fallbacks":0,"waits":0,"blankFrames":0,"loadingFrames":0,"last":{"ms":38,"end":"landed"}}
noblank_expect() {
    local name="$1" before="$2" after="$3" condition="$4"
    jq -e -n --argjson b "$before" --argjson a "$after" "$condition" >/dev/null \
        || fail "noblank: $name broke '$condition'; before $before after $after"
}

# One navigation, shot issued and landed: held for at least minimum ms, swapped before the cap, and no frame drawn with no row.
noblank_step() {
    local name="$1" want_path="$2" want_total="$3" minimum="$4" before after
    shift 4
    before=$(noblank_state)
    "$@" >/dev/null
    shot "noblank-$name-issued"
    noblank_landed "$want_path" "$want_total"
    shot "noblank-$name-landed"
    after=$(noblank_state)
    noblank_expect "$name" "$before" "$after" \
        "\$a.holds == \$b.holds + 1 and \$a.last.end == \"landed\" and \$a.last.ms >= $minimum
         and \$a.fallbacks == \$b.fallbacks and \$a.blankFrames == \$b.blankFrames and (\$a.holding | not)"
    printf 'NOBLANK %s held_ms=%s\n' "$name" "$(jq -r '.last.ms' <<< "$after")"
}

# The listing swap, AGENTS.md "The listing swap". Before it, opening a folder cleared the pane at the
# request and drew 3 to 4 frames of an empty list before the rows came back; the pane now holds the
# old rows until the new listing's rows land and swaps path, rows, cursor and selection in one turn.
# ui/PaneSwap.qml counts, from the window's own afterAnimating, every frame handed to the scene graph
# while a listing was out and the pane was drawn in the loading state, and the case requires that
# count not to move across Enter, a double click and BackSpace in a 300-file folder, in all three
# views. The stub backend below answers two directories late: one inside the cap, which must be held
# for its whole wait, and one past it, which must fall back to the loading state and is the control
# that proves the counter sees frames at all, since the loading frames it counts are drawn empty too.
case_noblank() {
    local dir="$fixture_root/noblank" bin="$fixture_root/noblank-bin" real_bin="$flea_bin" i
    local before after
    sandbox_scratch "$dir"
    mkdir -p "$dir/inner" "$dir/held" "$dir/slow" "$dir/zbrief"
    for i in $(seq -w 1 300); do printf 'x\n' > "$dir/inner/file-$i.txt"; done
    for i in $(seq -w 1 40); do printf 'x\n' > "$dir/outer-$i.txt"; printf 'x\n' > "$dir/held/held-$i.txt"; done
    for i in 1 2 3; do printf 'x\n' > "$dir/slow/slow-$i.txt"; printf 'x\n' > "$dir/zbrief/brief-$i.txt"; done

    # A relay to the real binary that holds back the two late directories' list requests and ends on quit.
    sandbox_scratch "$bin"
    cat > "$bin/flea" <<'SH'
#!/usr/bin/env bash
set -u
[[ "${1:-}" == --backend ]] || exec "$NOBLANK_REAL_BIN" "$@"
while IFS= read -r line; do
    case "$line" in
        *'"c":"list"'*"\"path\":\"$NOBLANK_HELD\""*) sleep "$NOBLANK_HELD_DELAY" ;;
        *'"c":"list"'*"\"path\":\"$NOBLANK_SLOW\""*) sleep "$NOBLANK_SLOW_DELAY" ;;
        *'"c":"list"'*"\"path\":\"$NOBLANK_BRIEF\""*) sleep "$NOBLANK_BRIEF_DELAY" ;;
    esac
    printf '%s\n' "$line"
    [[ "$line" == *'"c":"quit"'* ]] && break
done | "$NOBLANK_REAL_BIN" --backend
SH
    chmod +x "$bin/flea"

    seed_ui_state "$fixture_root/noblank-state" '{"keys":"default","view":"list"}'
    local -x NOBLANK_REAL_BIN="$real_bin" NOBLANK_HELD="$dir/held" NOBLANK_SLOW="$dir/slow" NOBLANK_BRIEF="$dir/zbrief"
    local -x NOBLANK_HELD_DELAY="$noblank_held_delay_s" NOBLANK_SLOW_DELAY="$noblank_slow_delay_s" NOBLANK_BRIEF_DELAY="$noblank_brief_delay_s"
    flea_bin="$bin/flea"
    launch "$dir"
    flea_bin="$real_bin"
    # held, inner, slow, zbrief, then outer-01 to outer-40.
    wait_listing 44
    settle
    printf 'NOBLANK launch %s\n' "$(noblank_state)"

    goto_row 1
    noblank_step enter "$dir/inner" 300 0 key -k Return
    noblank_step backspace "$dir" 44 0 key -k BackSpace
    # The parent listing put the cursor back on inner, row 1, which is the row the double click lands on.
    noblank_step doubleclick "$dir/inner" 300 0 click_row 1 left --double
    noblank_step doubleclick-back "$dir" 44 0 key -k BackSpace

    key k >/dev/null
    settle
    noblank_step held "$dir/held" 40 "$noblank_held_min_ms" key -k Return
    noblank_step held-back "$dir" 44 0 key -k BackSpace

    # Past the cap: held first, then the loading state at once rather than after its own hold-off.
    key j >/dev/null
    key j >/dev/null
    settle
    before=$(noblank_state)
    key -k Return >/dev/null
    shot noblank-slow-issued
    for _attempt in $(seq 1 100); do
        [[ "$(noblank_state | jq -r '.fellBack')" == true ]] && break
        sleep 0.05
    done
    [[ "$(ipc state)" == loading && "$(noblank_state | jq -r '.fellBack')" == true ]] \
        || fail "noblank: the late listing never fell back to the loading state; $(noblank_state), state $(ipc state)"
    shot noblank-slow-loading
    noblank_landed "$dir/slow" 3
    shot noblank-slow-landed
    after=$(noblank_state)
    noblank_expect slow "$before" "$after" \
        "\$a.holds == \$b.holds + 1 and \$a.fallbacks == \$b.fallbacks + 1 and \$a.last.end == \"expired\"
         and \$a.last.ms >= $noblank_cap_min_ms and \$a.blankFrames == \$b.blankFrames
         and \$a.loadingFrames > \$b.loadingFrames and (\$a.fellBack | not) and \$a.shownMs >= $noblank_min_show_ms"
    printf 'NOBLANK slow held_ms=%s loading_frames=%s\n' "$(jq -r '.last.ms' <<< "$after")" \
        "$(jq -n --argjson b "$before" --argjson a "$after" '$a.loadingFrames - $b.loadingFrames')"
    noblank_step slow-back "$dir" 44 0 key -k BackSpace

    # Rows that land after the cap but inside the mark's minimum wait for it: the mark never flashes in and out.
    key j >/dev/null
    settle
    before=$(noblank_state)
    key -k Return >/dev/null
    noblank_landed "$dir/zbrief" 3
    shot noblank-brief-landed
    after=$(noblank_state)
    noblank_expect brief "$before" "$after" \
        "\$a.fallbacks == \$b.fallbacks + 1 and \$a.waits == \$b.waits + 1 and \$a.last.end == \"expired\" and \$a.blankFrames == \$b.blankFrames
         and \$a.shownMs >= $noblank_min_show_ms and \$a.shownMs < 1000"
    printf 'NOBLANK brief shown_ms=%s\n' "$(jq -r '.shownMs' <<< "$after")"
    noblank_step brief-back "$dir" 44 0 key -k BackSpace
    key k >/dev/null

    key k >/dev/null
    settle
    switch_view grid
    noblank_step grid-enter "$dir/inner" 300 0 key -k Return
    noblank_step grid-back "$dir" 44 0 key -k BackSpace
    switch_view columns
    noblank_step columns-enter "$dir/inner" 300 0 key -k Return
    noblank_step columns-back "$dir" 44 0 key -k BackSpace
    switch_view list

    printf 'NOBLANK enter=ok doubleclick=ok backspace=ok held=ok slow=ok brief=ok grid=ok columns=ok %s\n' "$(noblank_state)"
    kill_flea
}

# The session's own stopwatch: the gen whose settled line has landed, so the rows on screen are that query's.
wait_search_gen() {
    local want_gen="$1" stamps
    for _attempt in $(seq 1 300); do
        stamps=$(ipc searchStamps 2>/dev/null || printf '{}')
        jq -e --argjson g "$want_gen" '.gen == $g and .keyAt > 0 and .settledAt >= .keyAt' <<< "$stamps" >/dev/null 2>&1 && return 0
        sleep 0.05
    done
    fail "searchnoblank: gen $want_gen never settled; stamps $stamps"
}

# One search keystroke answered: the session settled on its gen with blankFrames and fallbacks still.
searchnoblank_key() {
    local view="$1" step="$2" gen="$3" before after
    shift 3
    before=$(noblank_state)
    key "$@" >/dev/null
    wait_search_gen "$gen"
    shot "searchnoblank-$view-$step"
    after=$(noblank_state)
    noblank_expect "$view-$step" "$before" "$after" \
        '$a.blankFrames == $b.blankFrames and $a.fallbacks == $b.fallbacks'
    printf 'SEARCHNOBLANK %s-%s gen=%s total=%s\n' "$view" "$step" "$gen" "$(ipc total)"
}

# One "/" keystroke narrowed: the query stands with blankFrames and fallbacks still.
searchnoblank_filter_key() {
    local view="$1" step="$2" query="$3" before after
    shift 3
    before=$(noblank_state)
    key "$@" >/dev/null
    settle
    [[ "$(ipc keyDeliveryState 2>/dev/null | jq -r '.filterQuery')" == "$query" ]] \
        || fail "searchnoblank: $view-$step left the filter query at $(ipc keyDeliveryState 2>/dev/null | jq -r '.filterQuery'), not $query"
    shot "searchnoblank-$view-$step"
    after=$(noblank_state)
    noblank_expect "$view-$step" "$before" "$after" \
        '$a.blankFrames == $b.blankFrames and $a.fallbacks == $b.fallbacks'
    printf 'SEARCHNOBLANK %s-%s query=%s shown=%s\n' "$view" "$step" "$query" "$(ipc listingWindowState 2>/dev/null | jq -r '.shownTotal')"
}

# Search and "/" change the rows in one frame in all three views, with PaneSwap's blank-frame count still across every key, while the relay holds each searchquery 0.8 s past Swap.HOLD_MS.
case_searchnoblank() {
    local dir="$fixture_root/searchnoblank" bin="$fixture_root/searchnoblank-bin" real_bin="$flea_bin"
    local before mid after i base
    sandbox_scratch "$dir"
    for i in $(seq -w 1 120); do printf 'x\n' > "$dir/notes-$i.md"; done
    for i in $(seq -w 1 60); do printf 'x\n' > "$dir/north-$i.txt"; done
    for i in $(seq -w 1 100); do printf 'x\n' > "$dir/misc-$i.dat"; done
    mkdir -p "$dir/sub"
    for i in $(seq -w 1 10); do printf 'x\n' > "$dir/sub/notes-sub-$i.md"; printf 'x\n' > "$dir/sub/north-sub-$i.txt"; done

    # A relay to the real binary that holds back every searchquery answer past the hold cap and ends on quit.
    sandbox_scratch "$bin"
    cat > "$bin/flea" <<'SH'
#!/usr/bin/env bash
set -u
[[ "${1:-}" == --backend ]] || exec "$SEARCHNOBLANK_REAL_BIN" "$@"
while IFS= read -r line; do
    case "$line" in
        *'"c":"searchquery"'*) sleep "$SEARCHNOBLANK_QUERY_DELAY" ;;
    esac
    printf '%s\n' "$line"
    [[ "$line" == *'"c":"quit"'* ]] && break
done | "$SEARCHNOBLANK_REAL_BIN" --backend
SH
    chmod +x "$bin/flea"

    seed_ui_state "$fixture_root/searchnoblank-state" '{"keys":"default","view":"list"}'
    local -x SEARCHNOBLANK_REAL_BIN="$real_bin" SEARCHNOBLANK_QUERY_DELAY="$searchnoblank_query_delay_s"
    flea_bin="$bin/flea"
    launch "$dir"
    flea_bin="$real_bin"
    # 280 files plus the sub folder itself.
    wait_listing 281
    settle
    printf 'SEARCHNOBLANK launch %s\n' "$(noblank_state)"

    local view
    for view in list columns grid; do
        switch_view "$view"
        # The search strip opens with its keys.toml key and types with no Enter.
        key f >/dev/null
        settle
        [[ "$(ipc keyDeliveryState 2>/dev/null | jq -r '.searchMode')" == typing ]] \
            || fail "searchnoblank: $view f never opened the search strip"
        # Gens keep rising across sessions (ui/js/Search.js), so each view counts from the gen its session opened at.
        base=$(ipc searchStamps 2>/dev/null | jq -r '.gen')
        [[ "$base" =~ ^[0-9]+$ ]] || fail "searchnoblank: $view searchStamps gave no gen to count from: '$base'"
        if [[ "$view" == list ]]; then
            # The control: the first answer is held 0.8 s past Swap.HOLD_MS; 300 ms in the folder rows stand.
            before=$(noblank_state)
            key n >/dev/null
            sleep "$searchnoblank_query_hold_s"
            [[ "$(ipc total)" == 281 ]] \
                || fail "searchnoblank: the held answer stopped drawing the folder rows: total $(ipc total)"
            [[ "$(ipc state)" == ready ]] \
                || fail "searchnoblank: the held answer left the ready state: $(ipc state)"
            mid=$(noblank_state)
            noblank_expect "$view-control" "$before" "$mid" \
                '$a.blankFrames == $b.blankFrames and $a.fallbacks == $b.fallbacks'
            wait_search_gen $((base + 1))
            shot "searchnoblank-$view-search-1"
            after=$(noblank_state)
            noblank_expect "$view-search-1" "$before" "$after" \
                '$a.blankFrames == $b.blankFrames and $a.fallbacks == $b.fallbacks'
            [[ "$(ipc total)" != 281 && "$(ipc total)" != 0 ]] \
                || fail "searchnoblank: gen $((base + 1)) answered total $(ipc total), not the narrowed results"
            printf 'SEARCHNOBLANK %s-search-1 gen=%s total=%s\n' "$view" "$((base + 1))" "$(ipc total)"
        else
            searchnoblank_key "$view" search-1 $((base + 1)) n
        fi
        searchnoblank_key "$view" search-2 $((base + 2)) o
        searchnoblank_key "$view" search-3 $((base + 3)) r
        searchnoblank_key "$view" search-4 $((base + 4)) z
        searchnoblank_key "$view" search-5 $((base + 5)) z
        searchnoblank_key "$view" search-6 $((base + 6)) q
        # No match is an honest empty window with its own caption, never a blank frame.
        [[ "$(ipc total)" == 0 ]] \
            || fail "searchnoblank: $view zzq matched total $(ipc total), not the honest zero"
        # The hero's caption string is pinned in tests/empty-state.sh; here only its state is read.
        if [[ "$test_frontend" == gpui ]]; then
            [[ "$(ipc searchQuery)" == norzzq && "$(ipc noMatchHeadline)" == *norzzq* ]] \
                || fail "searchnoblank: $view norzzq drew no search-empty headline: $(ipc noMatchHeadline)"
        else
            [[ "$(ipc emptyHeroState 2>/dev/null | jq -r '.visible and .captionOpacity == 1')" == true ]] \
                || fail "searchnoblank: $view zzq drew no honest empty hero: $(ipc emptyHeroState 2>/dev/null)"
        fi
        searchnoblank_key "$view" search-7 $((base + 7)) -k BackSpace
        searchnoblank_key "$view" search-8 $((base + 8)) -k BackSpace
        searchnoblank_key "$view" search-9 $((base + 9)) -k BackSpace
        [[ "$(ipc total)" != 0 ]] \
            || fail "searchnoblank: $view backspace never brought the results back"
        # Escape closes the session and re-lists the folder with blankFrames still.
        before=$(noblank_state)
        key -k Escape >/dev/null
        wait_listing 281
        settle
        shot "searchnoblank-$view-escape"
        after=$(noblank_state)
        noblank_expect "$view-escape" "$before" "$after" \
            '$a.blankFrames == $b.blankFrames and $a.fallbacks == $b.fallbacks'
        printf 'SEARCHNOBLANK %s search=ok\n' "$view"

        # "/" narrows held rows without a request; the empty query tail excludes close matches too.
        key / >/dev/null
        settle
        [[ "$(ipc keyDeliveryState 2>/dev/null | jq -r '.filterTyping')" == true ]] \
            || fail "searchnoblank: $view / never opened the filter line"
        searchnoblank_filter_key "$view" filter-1 n n
        searchnoblank_filter_key "$view" filter-2 no o
        searchnoblank_filter_key "$view" filter-3 nor r
        searchnoblank_filter_key "$view" filter-4 norzqqqq zqqqq
        [[ "$(ipc listingWindowState 2>/dev/null | jq -r '.shownTotal')" == 0 ]] \
            || fail "searchnoblank: $view norzqqqq still shows $(ipc listingWindowState 2>/dev/null | jq -r '.shownTotal') rows, not the honest none"
        searchnoblank_filter_key "$view" filter-5 nor -k BackSpace -k BackSpace -k BackSpace -k BackSpace -k BackSpace
        before=$(noblank_state)
        key -k Escape >/dev/null
        settle
        after=$(noblank_state)
        noblank_expect "$view-filter-escape" "$before" "$after" \
            '$a.blankFrames == $b.blankFrames and $a.fallbacks == $b.fallbacks'
        printf 'SEARCHNOBLANK %s filter=ok\n' "$view"
    done

    printf 'SEARCHNOBLANK list=ok columns=ok grid=ok %s\n' "$(noblank_state)"
    kill_flea
}

# One launch on the lookopen fixture in a view, from its own state seeded with patch; launch() stops any running Flea first.
lookopen_launch() {
    local dir="$1" view="$2" label="$3" patch="$4"
    seed_ui_state "$fixture_root/lookopen-state-$view-$label" "$patch"
    launch "$dir"
    wait_listing 4
    settle
    [[ "$(ipc viewMode)" == "$view" ]] || fail "lookopen: fixture did not open its $view view"
}

# A cap reveal shows the loading state first, so a slow source lands after settle: wait up to 15 s, past cap reveal number min_caps.
lookopen_wait_landed() {
    local min_caps="$1" state caps
    for _ in $(seq 60); do
        state=$(ipc previewSwapState)
        caps=$(jq -r '.lookOpen.revealCaps' <<< "$state")
        [[ "$(jq -r '.lookOpen.landed' <<< "$state")" == true && "$caps" =~ ^[0-9]+$ ]] && (( caps >= min_caps )) && return 0
        sleep 0.25
    done
    fail "lookopen: the preview never landed past cap reveal $min_caps in 15 s, last state $state"
}

# With row thumbnails off the Columns preview still decodes the cursor row's cached thumbnail, so wait for its file: Space then always finds Quick Look's interim ready.
lookopen_wait_thumb() {
    local index file
    for _ in $(seq 60); do
        index=$(ipc cursor)
        file=$(ipc thumbnailPolicyState | jq -r ".files[\"$index\"] // empty")
        [[ -n "$file" && "$file" != cache-asked ]] && return 0
        sleep 0.25
    done
    fail "lookopen: the preview column never decoded a thumbnail for the cursor row"
}

# One Space open and close of a row, held to previewSwapState's lookOpen; want_caps is the cap reveals it adds, wait_thumb waits for the preview column's decoded thumbnail first.
lookopen_step() {
    local view="$1" name="$2" want_caps="$3" wait_thumb="${4:-0}" before after empty0 empty1 caps0 caps1
    seek_row_named "$name"
    settle
    if (( wait_thumb )); then lookopen_wait_thumb; fi
    before=$(ipc previewSwapState)
    key -k space >/dev/null
    shot "lookopen-$view-$name-issued"
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "lookopen: space did not open Quick Look on $name in $view"
    lookopen_wait_landed 0
    shot "lookopen-$view-$name-landed"
    after=$(ipc previewSwapState)
    key -k Escape >/dev/null
    settle
    # Sample input: previewSwapState {"look":{...},"lookOpen":{"revealed":true,"revealedEarly":false,"landed":true,"lookEmptyFrames":0,"revealCaps":0},...}.
    empty0=$(jq -r '.lookOpen.lookEmptyFrames' <<< "$before"); empty1=$(jq -r '.lookOpen.lookEmptyFrames' <<< "$after")
    caps0=$(jq -r '.lookOpen.revealCaps' <<< "$before"); caps1=$(jq -r '.lookOpen.revealCaps' <<< "$after")
    [[ "$empty1" =~ ^[0-9]+$ && "$empty0" =~ ^[0-9]+$ ]] || fail "lookopen: previewSwapState carries no lookOpen counter: $after"
    [[ "$(jq -r '.lookOpen.revealed' <<< "$after")" == true && "$(jq -r '.lookOpen.landed' <<< "$after")" == true ]] \
        || fail "lookopen: $name in $view never revealed or landed: $after"
    [[ "$empty1" == "$empty0" ]] || fail "lookopen: $name in $view drew $((empty1 - empty0)) empty card or dim frame(s): $after"
    # No press or wheel is sent, so an early reveal is the cap's and nothing else's.
    if (( want_caps )); then
        [[ "$caps1" == "$((caps0 + want_caps))" && "$(jq -r '.lookOpen.revealedEarly' <<< "$after")" == true ]] \
            || fail "lookopen: the slow $name in $view did not reveal by the cap: $after"
    else
        [[ "$caps1" == "$caps0" && "$(jq -r '.lookOpen.revealedEarly' <<< "$after")" == false ]] \
            || fail "lookopen: $name in $view revealed by the cap, not by its content: $after"
    fi
    printf 'LOOKOPEN view=%s file=%s empty=%s cap=%s\n' "$view" "$name" "$((empty1 - empty0))" "$((caps1 - caps0))"
}

# Space, Escape, Space on a slow row: the reopen lands inside the close fade of a card that drew, and draws no frame of it; want_caps is the cap reveals it adds, as in lookopen_step.
lookopen_reopen() {
    local view="$1" name="$2" want_caps="$3" before after empty caps early
    seek_row_named "$name"
    settle
    key -k space >/dev/null
    lookopen_wait_landed 0
    before=$(ipc previewSwapState)
    # One stroke, so the reopen arrives well inside the 140 ms close fade.
    key -k Escape -k space >/dev/null
    shot "lookopen-$view-$name-reopened"
    [[ "$(ipc previewOpen)" == "true" ]] || fail "lookopen: Escape then Space did not reopen Quick Look on $name in $view"
    lookopen_wait_landed "$(( $(jq -r '.lookOpen.revealCaps' <<< "$before") + want_caps ))"
    after=$(ipc previewSwapState)
    key -k Escape >/dev/null
    settle
    empty=$(( $(jq -r '.lookOpen.lookEmptyFrames' <<< "$after") - $(jq -r '.lookOpen.lookEmptyFrames' <<< "$before") ))
    caps=$(( $(jq -r '.lookOpen.revealCaps' <<< "$after") - $(jq -r '.lookOpen.revealCaps' <<< "$before") ))
    early=$(jq -r '.lookOpen.revealedEarly' <<< "$after")
    [[ "$(jq -r '.lookOpen.landed' <<< "$after")" == true ]] || fail "lookopen: the reopen of $name in $view never landed: $after"
    (( empty == 0 )) || fail "lookopen: the reopen of $name in $view drew $empty frame(s) of the closing card or dim: $after"
    # No press or wheel is sent, so an early reveal is the cap's and nothing else's.
    if (( want_caps )); then
        (( caps == 1 )) || fail "lookopen: the reopen of $name in $view did not reveal by the cap once: $after"
        [[ "$early" == true ]] || fail "lookopen: the reopen of $name in $view counted a cap without an early reveal: $after"
    else
        (( caps == 0 )) || fail "lookopen: the reopen of $name in $view revealed by the cap, not by its content: $after"
        [[ "$early" == false ]] || fail "lookopen: the reopen of $name in $view revealed early without the cap: $after"
    fi
    printf 'LOOKOPEN view=%s file=%s step=reopen empty=%s cap=%s\n' "$view" "$name" "$empty" "$caps"
}

# Quick Look on one row must draw its picture; a failed read draws the sentence in its place (#290).
relstart_look() {
    local name="$1" kind="$2"
    seek_row_named "$name"
    settle
    key -k space >/dev/null
    flea_ipc_wait -p "$flea_ui/boot" flea previewState "$kind" --timeout 10 >/dev/null \
        || fail "relstart: Quick Look on $name draws '$(ipc previewState)', not its $kind"
    [[ "$(ipc previewOpen)" == true && "$(ipc previewKind)" == "$kind" ]] \
        || fail "relstart: Quick Look on $name is $(ipc previewKind), open=$(ipc previewOpen), not a $kind"
    shot "relstart-$name"
    key -k Escape >/dev/null
    settle
}
