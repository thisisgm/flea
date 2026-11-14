# F04, boards 0401-nametip-list-b, 0401-nametip-list-lit and 0401-nametip-columns-b.
CASES+=("name-tip-list-rest 1100x700 view:list"
        "name-tip-list-lit 1100x700 view:list"
        "name-tip-columns 1100x700 view:columns"
        "name-tip-elision 1100x700 view:list"
        "name-tip-fitting 1100x700 view:list"
        "name-tip-rounded 1100x700 view:list")
# The tip opens after REST_MS (500 ms, crate::tabs::icons); the snap leaves 200 ms more for its frame.
name_tip_rest_seconds=0.7
# A fitting name must still be bare after the fidelity critic's 800 ms rest.
name_tip_fitting_seconds=0.8

name_tip_shot() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local view=$1 lit=$2 variant=${3:-rest} root=$C/name-tip/files index target= at rows
    local long=riverside-community-garden-committee-meeting-notes-2025-06-02-spring-review-final-approved.md
    if [ "$variant" = elision ]; then
        long=riverside-$(printf 'community-garden-%.0s' {1..7})committee-final-approved.md
    elif [ "$variant" = fitting ]; then
        long=notes.txt
    elif [ "$variant" = rounded ]; then
        # Only the visual lane's private headless container may change compositor configuration.
        [ -d /seed/.config/hypr ] && [ -f /out/frontend ] && [ -x /vinput/ydotoold-wl ] ||
            { state_failure "rounded name tip needs the private headless visual session"; return 1; }
        local previous_rounding
        previous_rounding=$(hyprctl getoption decoration:rounding -j | jq -er .int) &&
            [[ $previous_rounding =~ ^[0-9]+$ ]] ||
            { state_failure "hyprctl getoption decoration:rounding answered '$previous_rounding'"; return 1; }
        trap 'status=$?
            hyprctl eval "hl.config({ decoration = { rounding = $previous_rounding } })" > /dev/null &&
                [ "$(hyprctl getoption decoration:rounding -j | jq -er .int)" = "$previous_rounding" ] ||
                { state_failure "decoration:rounding was not restored to $previous_rounding"; status=1; }
            exit "$status"' EXIT
        # The session rounding already draws the other cases, so this case raises it by 8.
        local rounding=$((previous_rounding + 8))
        hyprctl eval "hl.config({ decoration = { rounding = $rounding } })" > /dev/null &&
            [ "$(hyprctl getoption decoration:rounding -j | jq -er .int)" = "$rounding" ] ||
            { state_failure "decoration:rounding did not change from $previous_rounding to $rounding"; return 1; }
    fi
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    printf 'Short fixture\n' > "$root/alpha.txt" || return 1
    printf 'Long name fixture\n' > "$root/$long" || return 1
    touch -d "$STAMP" "$root/alpha.txt" "$root/$long" || return 1
    LAUNCH_STATE=',"highlightHover":false,"display":{"textSize":{"mode":14}}' launch "$view" "$root" || return 1
    size_window 1100 700 || return 1
    wait_ipc total 2 || return 1
    if [ "$lit" = true ]; then cursor_to "$long" || return 1; fi
    rows=$(ipc visibleRows) && [[ $rows =~ ^[1-9][0-9]*$ ]] ||
        { state_failure "ipc visibleRows answered '$rows'"; return 1; }
    for index in $(seq 0 $((rows - 1))); do
        [ "$(ipc visibleRowName "$index")" != "$long" ] || { target=$index; break; }
    done
    [ -n "$target" ] || { state_failure "name tip fixture row is not painted"; return 1; }
    at=$(ipc rowNameCentre "$target") && [ -n "$at" ] ||
        { state_failure "ipc rowNameCentre $target answered '$at'"; return 1; }
    kdcap_point "$at" || return 1
    if [ "$variant" = fitting ]; then sleep "$name_tip_fitting_seconds"; else sleep "$name_tip_rest_seconds"; fi
    kdcap_shot
)

case_name_tip_list_rest() { name_tip_shot list false; }
case_name_tip_list_lit() { name_tip_shot list true; }
case_name_tip_columns() { name_tip_shot columns false; }
case_name_tip_elision() { name_tip_shot list false elision; }
case_name_tip_fitting() { name_tip_shot list false fitting; }
case_name_tip_rounded() { name_tip_shot list false rounded; }
