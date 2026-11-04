# GPUI overrides for UISH-07; seeded from the unchanged oracle at 2faf8d099.
case_dirsortstale() {
    local candidate_ui="$flea_ui" candidate_bin="$flea_bin"
    local dir="$fixture_root/dirsortstale"
    local test_ui="$fixture_root/dirsortstale-ui"
    local proxy="$fixture_root/dirsortstale-flea"
    local proxy_py="$fixture_root/dirsortstale-proxy.py"
    local armed="$fixture_root/dirsortstale-armed"
    local events="$fixture_root/dirsortstale-events"
    local source="$dir/aaa" destination="$dir/zzz"
    local old_bytes new_bytes old_state new_state
    local flea_ui="$candidate_ui" flea_bin="$candidate_bin"
    dirsortstale_saved_ui="$candidate_ui"
    dirsortstale_saved_bin="$candidate_bin"
    dirsortstale_test_ui="$test_ui"
    dirsortstale_candidate_bin="$candidate_bin"
    trap dirsortstale_cleanup EXIT

    sandbox_scratch "$dir"
    mkdir -p "$source" "$destination"
    printf 's' > "$source/payload"
    head -c 257 /dev/zero > "$destination/payload"
    old_bytes=$(( $(stat -c %s "$source") + $(stat -c %s "$source/payload") ))
    new_bytes=$(( $(stat -c %s "$destination") + $(stat -c %s "$destination/payload") ))
    [[ "$old_bytes" != "$new_bytes" ]] || fail "dirsortstale: fixture sizes are indistinguishable"

    sandbox_scratch "$test_ui"
    cp -a "$candidate_ui"/. "$test_ui/"
    python3 - "$test_ui/Ipc.qml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "        function dirSizeRequests(): int { return root.backend.dirSizeRequests }\n"
addition = needle + "        function dirSizeState(): string { return JSON.stringify(root.pane.dirSizeState) }\n"
if text.count(needle) != 1:
    raise SystemExit("dirsortstale: copied Ipc.qml getter anchor is not unique")
path.write_text(text.replace(needle, addition))
PY

    sandbox_require "$proxy_py"
    cat > "$proxy_py" <<'PY'
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys

real = os.environ["FLEA_DIRSORT_REAL_BIN"]
armed_path = Path(os.environ["FLEA_DIRSORT_ARMED"])
events_path = Path(os.environ["FLEA_DIRSORT_EVENTS"])
old_bytes = int(os.environ["FLEA_DIRSORT_OLD_BYTES"])

def log(value):
    with events_path.open("a") as stream:
        stream.write(value + "\n")

def emit(line):
    try:
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
    except BrokenPipeError:
        raise OutputClosed

class OutputClosed(Exception):
    pass

def complete_lines(buffer):
    lines = []
    while True:
        end = buffer.find(b"\n")
        if end < 0:
            return lines
        lines.append(bytes(buffer[:end + 1]))
        del buffer[:end + 1]

def read_lines(key, buffer, name):
    try:
        chunk = os.read(key.fd, 65536)
    except OSError as error:
        raise RuntimeError(f"{name} read failed: {error}") from error
    if not chunk:
        if buffer:
            raise RuntimeError(f"{name} closed with an incomplete protocol line")
        return [], True
    buffer.extend(chunk)
    return complete_lines(buffer), False

child = subprocess.Popen([real, "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
selector = selectors.DefaultSelector()
selector.register(sys.stdin.buffer, selectors.EVENT_READ, "ui")
selector.register(child.stdout, selectors.EVENT_READ, "backend")
armed = False
injected = False
queued = []
ui_open = True
backend_open = True
ui_buffer = bytearray()
backend_buffer = bytearray()

def close_child_input():
    try:
        child.stdin.close()
    except (BrokenPipeError, OSError, ValueError):
        pass

try:
    while ui_open or backend_open:
        if not injected and armed_path.exists():
            armed = True
        ready = selector.select(0.1)
        for key, _ in [item for item in ready if item[0].data == "ui"]:
            lines, eof = read_lines(key, ui_buffer, "ui")
            for line in lines:
                if not injected and armed_path.exists():
                    armed = True
                request = {}
                try:
                    request = json.loads(line)
                except (TypeError, ValueError):
                    pass
                if armed and not injected and request.get("c") == "sort":
                    emit((json.dumps({"t": "dirsized", "row": 0, "bytes": old_bytes,
                                      "partial": False, "ms": 0.0}) + "\n").encode())
                    log("inject-before-sort")
                    child.stdin.write(line)
                    child.stdin.flush()
                    log("forward-sort")
                    for pending in queued:
                        emit(pending)
                    queued.clear()
                    injected = True
                    armed = False
                else:
                    child.stdin.write(line)
                    child.stdin.flush()
            if eof:
                selector.unregister(key.fileobj)
                ui_open = False
                close_child_input()
        for key, _ in [item for item in ready if item[0].data == "backend"]:
            lines, eof = read_lines(key, backend_buffer, "backend")
            for line in lines:
                if armed and not injected:
                    queued.append(line)
                else:
                    emit(line)
            if eof:
                selector.unregister(key.fileobj)
                backend_open = False
except OutputClosed:
    ui_open = False
    try:
        sys.stdout.buffer.close()
    except (BrokenPipeError, OSError, ValueError):
        pass
finally:
    close_child_input()
    if child.poll() is None:
        try:
            child.terminate()
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            try:
                child.kill()
            except ProcessLookupError:
                pass
            child.wait()
    selector.close()
PY

    sandbox_require "$proxy"
    cat > "$proxy" <<'SH'
#!/usr/bin/env bash
set -u
if [[ "${1:-}" == "--backend" ]]; then
    exec python3 "$FLEA_DIRSORT_PROXY_PY"
fi
exec "$FLEA_DIRSORT_REAL_BIN" "$@"
SH
    chmod +x "$proxy"

    sandbox_require "$armed"
    sandbox_require "$events"
    : > "$events"
    seed_ui_state "$fixture_root/dirsortstale-state" '{"sort":{"key":"name","reverse":false}}'
    kill_flea
    local -x FLEA_DIRSORT_REAL_BIN="$candidate_bin"
    local -x FLEA_DIRSORT_PROXY_PY="$proxy_py"
    local -x FLEA_DIRSORT_ARMED="$armed"
    local -x FLEA_DIRSORT_EVENTS="$events"
    local -x FLEA_DIRSORT_OLD_BYTES="$old_bytes"
    flea_ui="$test_ui"
    flea_bin="$proxy"
    launch "$dir"
    wait_listing 2
    old_state=$(dirsortstale_wait_size "$old_bytes") || fail "dirsortstale: initial source dirsize never arrived"
    : > "$armed"
    key S >/dev/null
    new_state=$(dirsortstale_wait_size "$new_bytes") || fail "dirsortstale: fresh destination dirsize never arrived"
    grep -Fx 'inject-before-sort' "$events" >/dev/null || fail "dirsortstale: proxy did not inject before sort"
    grep -Fx 'forward-sort' "$events" >/dev/null || fail "dirsortstale: proxy did not forward sort"
    [[ "$old_state" != "$new_state" ]] || fail "dirsortstale: source and destination states were identical"
    printf 'DIRSORTSTALE old=%s new=%s fixture_bytes=%s/%s\n' "$old_state" "$new_state" "$old_bytes" "$new_bytes"
}

case_nosweep() {
    [[ -d "$bench_dir" ]] || fail "the 100,000-file fixture is missing at $bench_dir"
    # Same shape as case_thumbs: the drain the previous backend owes this cache is finished before the baseline.
    local before_large after_large wx wy ww wh burst
    kill_flea
    before_large=$(ls -A "$cache_large" | wc -l)
    launch "$bench_dir"
    wait_listing 100000
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    for burst in $(seq 1 "$scroll_bursts"); do
        omarchy-drive scroll down "$wheel_clicks" >/dev/null
    done
    key G >/dev/null
    sleep 1
    key g >/dev/null
    sleep 1
    after_large=$(ls -A "$cache_large" | wc -l)
    printf 'NOSWEEP requests=%s cursor=%s cache before=%s after=%s\n' \
        "$(ipc thumbRequests)" "$(ipc cursor)" "$before_large" "$after_large"
    shot nosweep
    [[ "$(ipc thumbRequests)" == "0" ]] || fail "a directory of text files produced $(ipc thumbRequests) thumb requests"
    [[ "$before_large" == "$after_large" ]] || fail "the cache grew from $before_large to $after_large"
    [[ "$(ls -A "$cache_large" | grep -c '^\.flea-')" == "0" ]] || fail "a temp file was left in the shared cache"

    # Task 16's twin: every row here is a dirsize candidate; the delta bound stands in for a literal zero, see AGENTS.md "Thumbnail requests".
    local dirsweep_dir before_requests after_requests delta
    dirsweep_dir="$fixture_root/nosweep-dirs"
    if [[ "$(find "$dirsweep_dir" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | wc -l)" -lt 100000 ]]; then
        sandbox_scratch "$dirsweep_dir"
        seq 1 100000 | sed "s#^#$dirsweep_dir/dir_#" | xargs mkdir
    fi
    kill_flea
    launch "$dirsweep_dir"
    wait_listing 100000
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    before_requests=$(ipc dirSizeRequests)
    omarchy-drive scroll down "$fling_clicks" >/dev/null
    sleep 1
    after_requests=$(ipc dirSizeRequests)
    delta=$((after_requests - before_requests))
    printf 'DIRSWEEP fling before=%s after=%s delta=%s cursor=%s\n' \
        "$before_requests" "$after_requests" "$delta" "$(ipc cursor)"
    shot dirsweep
    (( delta >= 1 )) \
        || fail "settling after the fling issued zero dirsize requests, so nothing here proves the gate is even wired"
    (( delta <= 2 )) \
        || fail "one fling issued $delta dirsize requests, not the one settle's worth a debounced viewport should cost"
}

case_hiddenlast() {
    local dir="$fixture_root/hiddenlast" state="$fixture_root/hiddenlast-state" patch
    local l0="$dir" l1="$dir/Alpha" l2="$dir/Alpha/Alpha" l3="$dir/Alpha/Alpha/Alpha"
    local title="Hidden last chooser $$" reply="$fixture_root/hiddenlast-reply.json" picker_pid="" shot_png menus_checks=0
    sandbox_scratch "$dir"
    hiddenlast_fixture "$dir"
    patch='{"hidden":true,"hiddenLast":true,"rememberSort":false,"columnsLimit":4,"sort":{"key":"name","reverse":false},"view":"list"}'
    seed_ui_state "$state" "$patch"
    launch "$l2"
    wait_listing 6

    hiddenlast_expect list "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-list

    hiddenlast_view columns
    hiddenlast_four_columns
    hiddenlast_expect columns-active "$hiddenlast_on" hiddenlast_rows
    hiddenlast_expect columns-parent "$hiddenlast_on" ipc columnNames parent
    hiddenlast_expect columns-grandparent "$hiddenlast_on" ipc columnPeekNames "$l0"
    hiddenlast_expect columns-child "$hiddenlast_on" ipc columnNames child
    shot hiddenlast-columns

    hiddenlast_view grid
    hiddenlast_expect grid "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-grid

    # Ctrl+Return on Alpha opens the deepest copy in a second tab.
    hiddenlast_view list
    seek_row_named Alpha
    key -M ctrl -k Return -m ctrl >/dev/null || fail "hiddenlast: Ctrl+Return was rejected"
    wait_tabs 2 "Ctrl+Return on Alpha"
    wait_path "$l3"
    hiddenlast_expect tab "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-tab

    # The setting goes off in the running window: the plain order returns in the tab, the first tab and a column, with no relaunch.
    env XDG_STATE_HOME="$state" "$flea_bin" --ui-state '{"hiddenLast":false}' >/dev/null || fail "hiddenlast: the state write failed"
    hiddenlast_expect tab-off "$hiddenlast_off" hiddenlast_rows
    click_tab 0
    wait_path "$l2"
    hiddenlast_expect list-off "$hiddenlast_off" hiddenlast_rows
    hiddenlast_view columns
    hiddenlast_expect columns-parent-off "$hiddenlast_off" ipc columnNames parent
    shot hiddenlast-off

    # Both dual panes, each from its own listing.
    kill_flea
    seed_ui_state "$state-dual" "$(jq -cn --arg l "$l1" --arg r "$l2" '{hidden:true,hiddenLast:true,rememberSort:false,sort:{key:"name",reverse:false},view:"dual",dual:{paths:[$l,$r],focus:0}}')"
    launch "$l1"
    menus_expect dualState '.active and .focused == 0 and all(.panes[]; .total == 6 and (.loading | not))' 'both dual listings landed'
    hiddenlast_expect dual-left "$hiddenlast_on" hiddenlast_rows
    key -k Tab >/dev/null
    menus_expect dualState '.focused == 1' 'the second pane has the keys'
    hiddenlast_expect dual-right "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-dual

    # The chooser: its list, then its header sort, which used to forget the setting.
    kill_flea
    FLEA_UI="$flea_ui" FLEA_BIN="$flea_bin" FLEA_PICKER="$(jq -cn --arg f "$l2" --arg t "$title" '{mode:"open",folder:$f,title:$t}')" FLEA_PICKER_REPLY="$reply" \
        flea_test_exec qs -p "$flea_ui/boot/picker.qml" >"$fixture_root/hiddenlast-picker.log" 2>&1 </dev/null &
    for _attempt in $(seq 1 300); do
        omarchy-drive windows --json | jq -e --arg title "$title" '[.windows[] | select(.title == $title)] | length == 1' >/dev/null && break
        sleep 0.05
    done
    omarchy-drive focus "$title" >/dev/null || fail "hiddenlast: the chooser never opened"
    for _attempt in $(seq 1 100); do
        picker_pid=$(flea_pids | head -1)
        [[ -n "$picker_pid" ]] && break
        sleep 0.05
    done
    [[ -n "$picker_pid" ]] || fail "hiddenlast: no chooser process"
    omarchy-drive key --window "$title" . >/dev/null || fail "hiddenlast: the chooser would not show hidden files"
    hiddenlast_expect chooser "$hiddenlast_on" hiddenlast_picker_rows "$picker_pid"
    shot_png="$evidence_dir/hiddenlast-chooser.png"
    mkdir -p "$evidence_dir"
    [[ ! -e "$shot_png" && ! -L "$shot_png" ]] || fail "hiddenlast: refusing existing evidence $shot_png"
    omarchy-drive shot "$shot_png" "$title" >/dev/null || fail "hiddenlast: the chooser shot failed"
    printf 'SHOT %s\n' "$shot_png"
    omarchy-drive key --window "$title" S >/dev/null || fail "hiddenlast: the chooser would not reverse its sort"
    hiddenlast_picker_reversed "$picker_pid" "$hiddenlast_on"
    hiddenlast_expect chooser-sorted "$hiddenlast_on_desc" hiddenlast_picker_rows "$picker_pid"
    shot_png="$evidence_dir/hiddenlast-chooser-sorted.png"
    [[ ! -e "$shot_png" && ! -L "$shot_png" ]] || fail "hiddenlast: refusing existing evidence $shot_png"
    omarchy-drive shot "$shot_png" "$title" >/dev/null || fail "hiddenlast: the sorted chooser shot failed"
    printf 'SHOT %s\n' "$shot_png"
    kill_flea
    printf 'HIDDENLAST list=ok columns=ok grid=ok tab=ok off=ok dual=ok chooser=ok\n'
}
