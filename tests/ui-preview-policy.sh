#!/usr/bin/env bash
# Sourced by ui.sh after ui-menus.sh and ui-permissions.sh; IPC only observes native actions.
# shellcheck disable=SC2034,SC2154 # ui.sh supplies state; shared guards use the case's menu_box.

thumbnailpolicy_expect() {
    local expression="$1" label="$2" observed deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        observed=$(ipc thumbnailPolicyState) || fail "thumbnailpolicy: observer failed"
        if jq -e "$expression" <<< "$observed" >/dev/null; then
            printf 'THUMBNAIL_POLICY %s %s\n' "$label" "$observed"
            return
        fi
        sleep 0.05
    done
    local diagnostic
    for diagnostic in "$menu_box"/decoder-*.log; do
        [[ -f "$diagnostic" ]] || continue
        menus_guard "$diagnostic"
        printf 'THUMBNAIL_DECODER_LOG %s\n' "$diagnostic"
        cat "$diagnostic"
    done
    fail "thumbnailpolicy: $label: expected $expression; observed $observed"
}

thumbnailpolicy_release() {
    [[ -n "${menu_box:-}" ]] || return
    menus_guard "$menu_box/released"
    python3 - "$menu_box" <<'PY'
import os, pathlib, sys
root = pathlib.Path(sys.argv[1])
assert root.is_absolute() and root.resolve() == root and (root / '.flea-test-sandbox').is_file()
(root / 'released').touch()
for gate in (root / 'gates').iterdir():
    assert gate.resolve().is_relative_to(root) and gate.is_fifo()
    descriptor = os.open(gate, os.O_RDWR | os.O_NONBLOCK)
    try:
        os.write(descriptor, b'1')
    finally:
        os.close(descriptor)
PY
}

thumbnailpolicy_helper_inventory() {
    local expected="$1" actual
    actual=$(jq -sc 'map(.name) | sort' "$menu_box/helpers.jsonl") || fail "thumbnailpolicy: helper log is invalid"
    [[ "$actual" == "$expected" ]] || fail "thumbnailpolicy: expected helper starts $expected; observed $actual"
    printf 'THUMBNAIL_HELPERS %s\n' "$actual"
}

thumbnailpolicy_wrapper() {
    menus_guard "$menu_box/bin/prlimit"
    cat > "$menu_box/bin/prlimit" <<'PY'
#!/usr/bin/python3
import json, os, pathlib, sys, time

arguments = sys.argv[1:]
binds = [arguments[index + 1:index + 3] for index, value in enumerate(arguments) if value == '--bind']
if binds:
    root = pathlib.Path(os.environ['FLEA_THUMB_POLICY_BOX'])
    assert root.is_absolute() and root.resolve() == root and (root / '.flea-test-sandbox').is_file()
    assert len(binds) == 1 and len(binds[0]) == 2 and binds[0][0] == binds[0][1]
    output = pathlib.Path(binds[0][0])
    assert output.resolve().is_relative_to(root / 'cache/thumbnails/large') and output.is_file()
    inputs = [arguments[index + 1:index + 3] for index, value in enumerate(arguments) if value == '--ro-bind']
    source = pathlib.Path(inputs[-1][0])
    assert inputs[-1][0] == inputs[-1][1] and source.resolve().parent == root / 'listing' and source.is_file()
    name = source.name
    assert name in ['a-ready.png', 'b-block1.png', 'b-block2.png', 'b-block3.png', 'b-block4.png', 'c-image.png', 'd-video.mp4']
    record = {'name': name, 'pid': os.getpid(), 'parent': os.getppid(), 'argv': arguments, 'started': time.monotonic()}
    descriptor = os.open(root / 'helpers.jsonl', os.O_WRONLY | os.O_APPEND)
    try:
        os.write(descriptor, (json.dumps(record) + '\n').encode())
    finally:
        os.close(descriptor)
    if name.startswith('b-block') and not (root / 'released').exists():
        with (root / 'gates' / name).open('rb', buffering=0) as gate:
            assert gate.read(1) == b'1'
    diagnostic = os.open(root / ('decoder-' + name + '.log'), os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    os.dup2(diagnostic, 2)
    os.close(diagnostic)
# Keep the production resource limits, bwrap jail and selected decoder unchanged after the gate.
os.execv('/usr/bin/prlimit', ['/usr/bin/prlimit', *arguments])
PY
    chmod +x "$menu_box/bin/prlimit" || fail "thumbnailpolicy: helper gate is not executable"
}

case_thumbnailpolicy() (
    local menu_box="" mode path index cached digest baseline backend helper_count deadline permissions_listing tool off_state
    local -a backends
    local expected_initial='["a-ready.png","b-block1.png","b-block2.png","b-block3.png","b-block4.png"]'
    local expected_complete='["a-ready.png","b-block1.png","b-block2.png","b-block3.png","b-block4.png","c-image.png","d-video.mp4"]'
    [[ -x /usr/bin/prlimit && -x /usr/bin/python3 ]] || fail "thumbnailpolicy: production prlimit or Python is unavailable"
    for tool in magick ffmpeg; do
        command -v "$tool" >/dev/null || fail "thumbnailpolicy: real fixture tool is unavailable: $tool"
    done
    sandbox_require "$fixture_root"
    trap 'thumbnailpolicy_release' EXIT
    for mode in list grid columns; do
        menu_box=$(mktemp -d "$fixture_root/thumbnail-policy-$mode.XXXXXXXX") || fail "thumbnailpolicy: fixture creation failed"
        permissions_listing="$menu_box/listing"
        printf 'native thumbnail policy fixture\n' > "$menu_box/.flea-test-sandbox"
        [[ "$menu_box" == "$(realpath -e "$menu_box")" ]] || fail "thumbnailpolicy: fixture path is not canonical"
        for path in listing sources state config cache data bin gates; do
            menus_guard "$menu_box/$path"
            mkdir "$menu_box/$path" || fail "thumbnailpolicy: cannot create $path"
        done
        menus_guard "$menu_box/helpers.jsonl"
        : > "$menu_box/helpers.jsonl"
        menus_guard "$menu_box/sources/image.png"
        magick -size 64x48 xc:steelblue "$menu_box/sources/image.png" || fail "thumbnailpolicy: image creation failed"
        menus_guard "$menu_box/sources/video.mp4"
        ffmpeg -nostdin -v error -f lavfi -i 'testsrc2=size=64x48:rate=24' -t 1 -pix_fmt yuv420p "$menu_box/sources/video.mp4" \
            || fail "thumbnailpolicy: video creation failed"
        menus_guard "$menu_box/listing/a-ready.png"
        cp "$menu_box/sources/image.png" "$menu_box/listing/a-ready.png" || fail "thumbnailpolicy: warm image setup failed"
        thumbnailpolicy_wrapper
        export FLEA_THUMB_POLICY_BOX="$menu_box"
        export XDG_CONFIG_HOME="$menu_box/config" XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
        export PATH="$menu_box/bin:$PATH"
        seed_ui_state "$menu_box/state" "{\"view\":\"$mode\",\"keys\":\"default\",\"preview\":{\"column\":true,\"loadOn\":\"manual\",\"thumbnails\":\"media\"}}"
        launch "$menu_box/listing"
        wait_listing 1
        thumbnailpolicy_expect '.files["0"] | type == "string" and length > 0' "$mode warms a real completed thumbnail"
        cached=$(ipc thumbnailPolicyState | jq -er '.files["0"]')
        menus_guard "$cached"
        digest=$(sha256sum < "$cached") || fail "thumbnailpolicy: completed cache file is unreadable"
        thumbnailpolicy_helper_inventory '["a-ready.png"]'
        kill_flea

        # Four production workers are held before decoding; two further visible rows remain queued.
        for index in 1 2 3 4; do
            menus_guard "$menu_box/listing/b-block$index.png"
            cp "$menu_box/sources/image.png" "$menu_box/listing/b-block$index.png" || fail "thumbnailpolicy: blocking source setup failed"
            menus_guard "$menu_box/gates/b-block$index.png"
            mkfifo "$menu_box/gates/b-block$index.png" || fail "thumbnailpolicy: gate creation failed"
        done
        menus_guard "$menu_box/listing/c-image.png"
        menus_guard "$menu_box/listing/d-video.mp4"
        cp "$menu_box/sources/image.png" "$menu_box/listing/c-image.png" || fail "thumbnailpolicy: queued image setup failed"
        cp "$menu_box/sources/video.mp4" "$menu_box/listing/d-video.mp4" || fail "thumbnailpolicy: queued video setup failed"
        "$flea_bin" --ui-state '{"preview":{"thumbnails":"off"}}' >/dev/null || fail "thumbnailpolicy: disabled launch state failed"
        launch "$menu_box/listing"
        wait_listing 7
        permissions_viewport 1280 900
        if [[ "$mode" == columns ]]; then
            key -M ctrl -k Space -m ctrl >/dev/null
            thumbnailpolicy_expect '.previewIndex == 0 and (.previewPath | endswith("/a-ready.png")) and .previewReady' 'Columns decodes the completed cached thumbnail while row thumbnails are off'
        fi
        settings_open_key
        settings_section preview
        settings_focus_row preview.thumbnails
        key l >/dev/null
        settings_wait_value '.preview.thumbnails == "images"'
        key l >/dev/null
        settings_wait_value '.preview.thumbnails == "media"'
        thumbnailpolicy_expect '(.pending | sort) == [1,2,3,4,5,6] and (.files["0"] | type == "string" and length > 0)' "$mode has running and queued native thumbnail work"
        off_state='.pending == [] and (.files | keys) == ["0"]'
        if [[ "$mode" == columns ]]; then
            key -k Escape >/dev/null
            key -k Down >/dev/null
            key -M ctrl -k Space -m ctrl >/dev/null
            thumbnailpolicy_expect '.previewIndex == 1 and (.previewPath | endswith("/b-block1.png")) and (.previewReady | not) and .files["1"] == null' 'Columns loads the selected identity and waits for its gated thumbnail'
            settings_open_key
            settings_focus_row preview.thumbnails
            off_state='.pending == [1] and (.files | keys) == ["0","1"]'
        fi
        mapfile -t backends < <(backend_pids)
        [[ "${#backends[@]}" == 1 ]] || fail "thumbnailpolicy: expected one owned backend"
        backend="${backends[0]}"
        permissions_backend_owned "$backend" || fail "thumbnailpolicy: backend ownership changed"
        deadline=$((SECONDS + 5))
        while (( SECONDS < deadline )); do
            helper_count=$(jq -s 'length' "$menu_box/helpers.jsonl") || fail "thumbnailpolicy: invalid helper log"
            [[ "$helper_count" == 5 ]] && break
            sleep 0.05
        done
        thumbnailpolicy_helper_inventory "$expected_initial"
        python3 - "$menu_box/helpers.jsonl" "$backend" <<'PY' || fail "thumbnailpolicy: running helper attribution failed"
import json, os, pathlib, sys
rows = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
for row in rows:
    if row['name'].startswith('b-block'):
        assert row['parent'] == int(sys.argv[2])
        process = pathlib.Path('/proc', str(row['pid']))
        assert process.stat().st_uid == os.getuid()
        assert pathlib.Path(sys.argv[1]).parent.as_posix().encode() + b'/bin/prlimit' in (process / 'cmdline').read_bytes().split(b'\0')
print('THUMBNAIL_GATE_OWNERS ' + json.dumps(rows))
PY
        baseline=$(ipc thumbnailPolicyState | jq -c '{contentY,cursor,previewIndex,previewPath,previewReady}')
        key h >/dev/null
        settings_wait_value '.preview.thumbnails == "images"'
        thumbnailpolicy_expect '.mode == "images" and (.pending | sort) == [1,2,3,4,5] and (.files | has("6") | not)' "$mode cancels the queued video without scrolling"
        key h >/dev/null
        settings_wait_value '.preview.thumbnails == "off"'
        thumbnailpolicy_expect ".mode == \"off\" and $off_state" "$mode cancels unowned pending rows and retains completed cache and preview ownership"
        [[ "$(ipc thumbnailPolicyState | jq -er '.files["0"]')" == "$cached" && "$(sha256sum < "$cached")" == "$digest" ]] \
            || fail "thumbnailpolicy: disabling thumbnails replaced or discarded completed cache"
        [[ "$(ipc thumbnailPolicyState | jq -c '{contentY,cursor,previewIndex,previewPath,previewReady}')" == "$baseline" ]] \
            || fail "thumbnailpolicy: changing policy moved the listing or changed loaded preview ownership"
        thumbnailpolicy_release || fail "thumbnailpolicy: running decoder release failed"
        thumbnailpolicy_expect '.pending == [] and (.files | keys) == ["0","1","2","3","4"] and all(.files[]; type == "string" and length > 0)' "$mode lets running real helpers finish while canceled jobs stay absent"
        if [[ "$mode" == columns ]]; then
            thumbnailpolicy_expect '.previewIndex == 1 and (.previewPath | endswith("/b-block1.png")) and .previewReady' 'Columns decodes its owned thumbnail after gate release'
            omarchy-drive wait ipc -p "$flea_ui" flea columnThumbShown true --timeout 15 >/dev/null \
                || fail "thumbnailpolicy: completed selected thumbnail is not drawn in the Columns frame"
            baseline=$(jq -c '.previewReady = true' <<< "$baseline") || fail "thumbnailpolicy: invalid preview baseline"
        fi
        thumbnailpolicy_helper_inventory "$expected_initial"
        for index in 0 1 2 3 4 5 6; do
            [[ -z "$(ipc rowThumb "$index")" ]] || fail "thumbnailpolicy: Off still draws a thumbnail at row $index"
        done
        shot "thumbnail-policy-$mode-off"
        key l >/dev/null
        settings_wait_value '.preview.thumbnails == "images"'
        key l >/dev/null
        settings_wait_value '.preview.thumbnails == "media"'
        thumbnailpolicy_expect '.pending == [] and (.files | keys) == ["0","1","2","3","4","5","6"] and all(.files[]; type == "string" and length > 0)' "$mode re-enables only canceled work and receives real image/video output"
        thumbnailpolicy_helper_inventory "$expected_complete"
        [[ "$(sha256sum < "$cached")" == "$digest" ]] || fail "thumbnailpolicy: re-enabling regenerated completed cache"
        [[ "$(ipc thumbnailPolicyState | jq -c '{contentY,cursor,previewIndex,previewPath,previewReady}')" == "$baseline" ]] \
            || fail "thumbnailpolicy: re-enabling changed the viewport or loaded preview"
        key -k Escape >/dev/null
        for index in 0 1 2 3 4 5 6; do
            deadline=$((SECONDS + 15))
            while [[ "$(ipc rowThumbReady "$index")" != true ]] && (( SECONDS < deadline )); do sleep 0.05; done
            [[ "$(ipc rowThumbReady "$index")" == true ]] || fail "thumbnailpolicy: row $index never decoded its enabled thumbnail"
        done
        shot "thumbnail-policy-$mode-restored"
        printf 'THUMBNAIL_POLICY_HELPER_LOG mode=%s\n' "$mode"
        cat "$menu_box/helpers.jsonl"
        kill_flea
    done
    trap - EXIT
)
