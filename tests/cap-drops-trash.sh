#!/usr/bin/env bash
# Runs cap_drops up to its launch with stat doubles, and pins the GLib trash rule the case's HOME and preflight rest on.
set -uo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)" || exit 1
. "$repo/tools/flea-sandbox-guard"
. "$repo/tests/ui-captures-drops.sh"
sandbox_root_ok
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-capdrops-trash.XXXXXX") || exit 1
: > "$scratch/$SANDBOX_MARKER"
trap 'sandbox_remove "$scratch"' EXIT

dev_of_folder=11
failed=0
checks=0
# The stub qs gets 100 tries of 50 ms to record what it saw; the step stays under 1000 ms.
qs_wait_tries=100
qs_wait_step_ms=50
qs_wait_deadline_ms=$((qs_wait_tries * qs_wait_step_ms))
qs_wait_step=$(printf '0.%03d' "$qs_wait_step_ms")

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# The shared transport launches only worktree binaries, so the case spawns its double the way ui.sh did before it.
. "$repo/tests/lib/fixture-launch.sh" || fail "cannot load the fixture launch double"
test_frontend=qml

seed_ui_state() {
    :
}

# The real helper copies the box's theme into the home; the stub keeps the wipe and names the home the stat double answers for.
fixture_home_make() {
    sandbox_scratch "$1"
    seen_home="$1"
    printf '%s\n' "$1" > "$fixture_root/home-seen"
}

# The real launch and the launch_window it calls from tests/ui.sh, launch renamed: the case's flea_launch_wrap goes through the code that consumes it.
real_launch_src=$(sed -n '/^launch_window() {$/,/^}$/p;/^launch() {$/,/^}$/{s/^launch()/real_launch()/;p}' "$repo/tests/ui.sh")
[[ "$real_launch_src" == launch_window\(\)* && "$real_launch_src" == *$'\n'real_launch\(\)* ]] || fail "tests/ui.sh has no launch and launch_window functions to run"
eval "$real_launch_src"
eval "$(sed -n '/^case_homes() {$/,/^}$/p' "$repo/tests/ui.sh")"
declare -F launch_window >/dev/null || fail "the launch_window of tests/ui.sh did not load"
kill_flea() { :; }
assert_window() { :; }
flea_pid() { printf stub; }
omarchy-drive() { :; }

# The case stops here: what Flea would be launched with is the whole question.
launch() {
    local i seen=""
    real_launch "$1"
    printf 'LAUNCH_WRAP %s\n' "${flea_launch_wrap[*]-}"
    printf 'LAUNCH_HOME %s\n' "$HOME"
    printf 'LAUNCH_DATA %s\n' "${XDG_DATA_HOME-unset}"
    for i in $(seq 1 "$qs_wait_tries"); do [[ -s "$fixture_root/qs-saw" ]] && break; sleep "$qs_wait_step"; done
    seen=$(cat "$fixture_root/qs-saw" 2>/dev/null || true)
    printf 'LAUNCH_SAW_HOME %s\n' "$(sed -n 's/^HOME=//p' <<< "$seen")"
    printf 'LAUNCH_SAW_DATA %s\n' "$(sed -n 's/^XDG_DATA_HOME=//p' <<< "$seen")"
    exit 0
}

# Sample input: stat -c %d /home/flea-sandbox/flea-capdrops-trash.AbCdEf/same/cap-drops, answered by which of the three paths it names.
stat() {
    if [[ "$1" == -c && "$2" == %d ]]; then
        case "$3" in
            "$fixture_root/cap-drops") printf '%s\n' "$dev_of_folder" ;;
            "$seen_home") printf '%s\n' "$dev_of_home" ;;
            "$fixture_root"/*) printf '%s\n' "$dev_of_data" ;;
            *) return 1 ;;
        esac
    else
        command stat "$@"
    fi
}

check() {
    checks=$((checks + 1))
    if [[ "$1" == ok ]]; then
        printf 'ok %s\n' "$2"
    else
        printf 'FAIL %s\n' "$2"
        failed=$((failed + 1))
    fi
}

# Runs the case with the three devices given and leaves its log and exit status in the case's folder.
run_case() {
    local scenario="$1" dev_of_home="$2" dev_of_data="$3" case_dir="$scratch/$1"
    mkdir -p -- "$case_dir/bin"
    : > "$case_dir/$SANDBOX_MARKER"
    # flea hands its environment to qs, and qs here records the HOME and XDG_DATA_HOME it was started with.
    printf '#!/bin/sh\nexec qs "$@"\n' > "$case_dir/bin/flea"
    printf '#!/bin/sh\nprintf "HOME=%%s\\nXDG_DATA_HOME=%%s\\n" "$HOME" "${XDG_DATA_HOME-unset}" > "%s.tmp" && mv "%s.tmp" "%s"\n' \
        "$case_dir/qs-saw" "$case_dir/qs-saw" "$case_dir/qs-saw" > "$case_dir/bin/qs"
    chmod +x "$case_dir/bin/flea" "$case_dir/bin/qs"
    (
        fixture_root="$case_dir"
        flea_ui="$case_dir/ui"
        flea_bin="$case_dir/bin/flea"
        flea_log="$case_dir/flea.log"
        run_log="$case_dir/run.log"
        FLEA_UNDO_DIR="$case_dir/undo"
        PATH="$case_dir/bin:$PATH"
        seen_home=""
        case_cap_drops
    ) > "$case_dir/log" 2>&1
    printf '%s\n' "$?" > "$case_dir/rc"
}

# Sample input: the case's folder, the Flea HOME and the data home all on device 11.
same_device() {
    local case_dir="$scratch/same" log wrap home data
    run_case same "$dev_of_folder" "$dev_of_folder"
    log="$case_dir/log"
    [[ "$(cat "$case_dir/rc")" == 0 ]] || { check bad "same: the case stopped before its launch: $(tr '\n' ' ' < "$log")"; return; }
    wrap=$(sed -n 's/^LAUNCH_WRAP //p' "$log")
    # Sample input: env HOME=/home/flea-sandbox/x/cap-drops-home XDG_DATA_HOME=/home/flea-sandbox/x/cap-drops-data
    home=$(grep -o '\bHOME=[^ ]*' <<< "$wrap" | head -n 1)
    data=$(grep -o 'XDG_DATA_HOME=[^ ]*' <<< "$wrap" | head -n 1)
    [[ "$wrap" == env\ * && -n "$home" && -n "$data" ]] \
        && check ok "same: Flea launches under env with a HOME and an XDG_DATA_HOME" \
        || check bad "same: the launch wrap is '$wrap', not env with a HOME and an XDG_DATA_HOME"
    [[ "${home#HOME=}" == "$case_dir"/* && "${data#XDG_DATA_HOME=}" == "$case_dir"/* ]] \
        && check ok "same: both live inside the case's fixture root" \
        || check bad "same: '$home' and '$data' are not both inside $case_dir"
    [[ "${home#HOME=}" != "${data#XDG_DATA_HOME=}" && -d "${data#XDG_DATA_HOME=}" ]] \
        && check ok "same: the data home is a directory of its own, made before the launch" \
        || check bad "same: the data home '${data#XDG_DATA_HOME=}' is missing or is the HOME"
    [[ -n "${home#HOME=}" && "${home#HOME=}" == "$(cat "$case_dir/home-seen" 2>/dev/null)" ]] \
        && check ok "same: the HOME Flea gets is the one fixture_home_make built" \
        || check bad "same: the HOME '${home#HOME=}' is not the fixture home the case built"
    [[ -n "${home#HOME=}" && "$(sed -n 's/^LAUNCH_SAW_HOME //p' "$log")" == "${home#HOME=}" ]] \
        && check ok "same: the real launch starts qs with the fixture HOME" \
        || check bad "same: qs saw HOME '$(sed -n 's/^LAUNCH_SAW_HOME //p' "$log")' after waiting up to ${qs_wait_deadline_ms} ms, not '${home#HOME=}'"
    [[ -n "${data#XDG_DATA_HOME=}" && "$(sed -n 's/^LAUNCH_SAW_DATA //p' "$log")" == "${data#XDG_DATA_HOME=}" ]] \
        && check ok "same: the real launch starts qs with the fixture XDG_DATA_HOME" \
        || check bad "same: qs saw XDG_DATA_HOME '$(sed -n 's/^LAUNCH_SAW_DATA //p' "$log")' after waiting up to ${qs_wait_deadline_ms} ms, not '${data#XDG_DATA_HOME=}'"
    [[ "$(sed -n 's/^LAUNCH_HOME //p' "$log")" == "$HOME" ]] \
        && check ok "same: the harness keeps the operator's HOME" \
        || check bad "same: the harness HOME at the launch is '$(sed -n 's/^LAUNCH_HOME //p' "$log")', not $HOME"
    [[ "$(sed -n 's/^LAUNCH_DATA //p' "$log")" == "${XDG_DATA_HOME-unset}" ]] \
        && check ok "same: the harness environment gains no XDG_DATA_HOME" \
        || check bad "same: the harness XDG_DATA_HOME at the launch is '$(sed -n 's/^LAUNCH_DATA //p' "$log")'"
}

# Sample input: a Flea HOME on device 22 while the folder is on device 11, so GLib would use the folder's mount.
other_device() {
    local scenario="$1" dev_of_home="$2" dev_of_data="$3" diagnostic="$4" case_dir="$scratch/$1"
    run_case "$scenario" "$dev_of_home" "$dev_of_data"
    if [[ "$(cat "$case_dir/rc")" != 1 ]]; then
        check bad "$scenario: the case exited $(cat "$case_dir/rc"), not 1, on devices folder=$dev_of_folder home=$dev_of_home data=$dev_of_data"
    elif grep -q '^LAUNCH' "$case_dir/log"; then
        check bad "$scenario: the case launched Flea over devices that GLib would split"
    elif ! grep -Eq -- "$diagnostic" "$case_dir/log"; then
        check bad "$scenario: no message matching $diagnostic in: $(tr '\n' ' ' < "$case_dir/log")"
    else
        check ok "$scenario: the preflight stops the case before the launch"
    fi
}

# Two tmpfs mounts stand for two devices; gio runs in a user namespace so nothing outside the scratch root is touched.
trash_rule() {
    local root="$scratch/rule" out
    mkdir -p "$root/other" "$root/own"
    # East and the flea-ci unit container both allow it, so a refusal is a broken environment and fails, never skips.
    if ! unshare -Urm true 2>/dev/null; then
        check bad "rule: this environment refuses a user namespace, so the GLib trash rule was not probed"
        return
    fi
    # shellcheck disable=SC2016
    out=$(unshare -Urm bash -c '
        root=$1
        mount -t tmpfs tmpfs "$root/other" && mount -t tmpfs tmpfs "$root/own" || exit 3
        mkdir -p "$root/own/folder" "$root/own/data" "$root/own/home"
        [ "$(stat -c %d "$root/other")" != "$(stat -c %d "$root/own")" ] || exit 4
        printf x > "$root/own/folder/a.txt"
        printf y > "$root/own/folder/b.txt"
        HOME="$root/other" XDG_DATA_HOME="$root/own/data" gio trash "$root/own/folder/a.txt" >/dev/null 2>&1
        [ -e "$root/own/data/Trash/files/a.txt" ] && echo other-device=landed || echo other-device=not-landed
        HOME="$root/own/home" XDG_DATA_HOME="$root/own/data" gio trash "$root/own/folder/b.txt" >/dev/null 2>&1
        [ -e "$root/own/data/Trash/files/b.txt" ] && echo own-device=landed || echo own-device=not-landed
    ' rule "$root" 2>&1)
    [[ "$out" == *other-device=not-landed* ]] \
        && check ok "rule: a file on a device other than HOME's does not land in XDG_DATA_HOME/Trash" \
        || check bad "rule: the probe answered '$out' for a file on a device other than HOME's"
    [[ "$out" == *own-device=landed* ]] \
        && check ok "rule: a file on HOME's device lands in XDG_DATA_HOME/Trash" \
        || check bad "rule: the probe answered '$out' for a file on HOME's device"
}

same_device
other_device home-elsewhere 22 "$dev_of_folder" 'the Flea HOME .* is on device 22 and the folder .* on device 11'
other_device data-elsewhere "$dev_of_folder" 33 'the data home .* is on device 33 and the folder .* on device 11'
trash_rule
printf 'cap-drops-trash: %s checks, %s failed\n' "$checks" "$failed"
(( failed == 0 ))
