# The real native parent-row click, with the same per-frame delegate census as columnsback's move probe.
colhold_gpui() {
    local dir="$fixture_root/colhold" wx wy cx cy deadline
    local -x FLEA_COLUMNS_CENSUS=1
    sandbox_scratch "$dir"
    mkdir -p "$dir/a/child" "$dir/b/child"
    printf 'a\n' > "$dir/a/child/a.txt"
    printf 'b\n' > "$dir/b/child/b.txt"
    seed_ui_state "$fixture_root/colhold-state" '{"keys":"default","view":"columns"}'
    launch "$dir/a"
    cap_resize 1400 700
    wait_listing 1
    deadline=$(( $(date +%s%3N) + async_wait_ms ))
    until [[ "$(ipc columnNames child)" == a.txt ]]; do
        (( $(date +%s%3N) < deadline )) || fail 'colhold: the first child never drew its row'
        sleep 0.05
    done
    shot colhold-before
    read -r cx cy <<< "$(ipc columnParentRowCentre 1)"
    [[ -n "$cx" && -n "$cy" ]] || fail 'colhold: sibling has no parent-row pointer target'
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    shot colhold-issued
    noblank_landed "$dir/b" 1
    deadline=$(( $(date +%s%3N) + async_wait_ms ))
    until [[ "$(ipc columnNames child)" == b.txt ]]; do
        (( $(date +%s%3N) < deadline )) || fail 'colhold: the new child never drew its row'
        sleep 0.05
    done
    shot colhold-landed
    kill_flea
    python3 "$repo/tests/lib/flea_colhold_frames.py" "$flea_log" "$evidence_dir/colhold-frames.jsonl" "$dir/a" "$dir/b" \
        || fail 'colhold: a non-empty column drew no row'
}

case_colhold() {
    if [[ "${test_frontend:-gpui}" == gpui ]]; then colhold_gpui; return; fi
    local dir="$fixture_root/colhold" copy="$fixture_root/colhold-ui" saved_ui="$flea_ui" wx wy cx cy frames
    sandbox_scratch "$dir"
    sandbox_scratch "$copy"
    mkdir -p "$dir/a/child" "$dir/b/child"
    printf 'a\n' > "$dir/a/child/a.txt"
    printf 'b\n' > "$dir/b/child/b.txt"
    cp -a "$flea_ui/." "$copy/"
    cp "$repo/tests/columnsframes.js" "$copy/js/ColumnsFrames.js"
    python3 - "$copy/ColumnsArea.qml" <<'PY'
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
text = 'import "js/ColumnsFrames.js" as ColumnFrames\n' + text
at = text.rfind('}')
text = text[:at] + '''    Connections {
        target: root.Window.window
        function onAfterAnimating() {
            if (root.visible) console.log("COLHOLD FRAME " + JSON.stringify(ColumnFrames.census(root)))
        }
    }
''' + text[at:]
path.write_text(text)
PY
    seed_ui_state "$fixture_root/colhold-state" '{"keys":"default","view":"columns"}'
    flea_ui="$copy"
    launch "$dir/a"
    wait_listing 1
    settle
    [[ "$(ipc columnNames child)" == a.txt ]] || fail "colhold: the first child never drew its row"
    shot colhold-before
    read -r cx cy <<< "$(ipc columnParentRowCentre 1)"
    [[ -n "$cx" && -n "$cy" ]] || fail "colhold: sibling has no parent-row pointer target"
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    shot colhold-issued
    noblank_landed "$dir/b" 1
    settle
    [[ "$(ipc columnNames child)" == b.txt ]] || fail "colhold: the new child never drew its row"
    shot colhold-landed
    frames="$evidence_dir/colhold-frames.jsonl"
    # Sample input: COLHOLD FRAME {"path":"/fixture/b","columns":[{"rows":2,"firstName":"a"},{"rows":1,"firstName":"child"},{"rows":1,"firstName":"b.txt"}],"hasThird":true}
    python3 - "$flea_log" "$frames" "$dir/a" "$dir/b" <<'PY'
import json
import pathlib
import sys
log, output, before, after = sys.argv[1:]
frames = []
for line in pathlib.Path(log).read_text(errors="replace").splitlines():
    if "COLHOLD FRAME " in line:
        frames.append(json.loads(line.split("COLHOLD FRAME ", 1)[1]))
start = next((i for i, f in enumerate(frames) if f["path"] == before and len(f["columns"]) == 3 and all(c["rows"] for c in f["columns"])), None)
assert start is not None, "colhold: no settled original frame"
end = next((i for i in range(start + 1, len(frames)) if frames[i]["path"] == after and len(frames[i]["columns"]) == 3 and frames[i]["columns"][-1]["firstName"] == "b.txt"), None)
assert end is not None, "colhold: no settled completed frame"
seen = frames[start:end + 1]
pathlib.Path(output).write_text("".join(json.dumps(f) + "\n" for f in seen))
for i, frame in enumerate(seen):
    assert len(frame["columns"]) == 3 and all(c["rows"] > 0 for c in frame["columns"]), f"colhold: frame {i + 1} drew an empty non-empty column: {frame}"
    assert frame["hasThird"], f"colhold: frame {i + 1} drew no third column: {frame}"
print(f"COLHOLD PASS parent sibling click: {len(seen)} frames, no empty column")
PY
    [[ "$?" == 0 ]] || fail "colhold: a non-empty column drew no row"
    kill_flea
    flea_ui="$saved_ui"
}
