# GPUI overrides for UISH-05; seeded from the unchanged oracle at 2faf8d099.
case_select() {
    local dir="$fixture_root/select"
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"

    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    # The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
    QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" FLEA_PATH="$dir" FLEA_SELECT="$dir/b.txt" FLEA_BIN="$flea_bin" \
        flea_test_exec qs -p "$flea_ui/boot" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    wait_listing 3
    [[ "$(ipc path)" == "$dir" ]] || fail "select: opened $(ipc path), not $dir"
    local want_index
    want_index=$(row_index_of "b.txt")
    [[ "$(ipc cursor)" == "$want_index" ]] || fail "select: cursor is $(ipc cursor), not row $want_index"
    [[ "$(ipc selectionCount)" == "1" ]] || fail "select: selectionCount is $(ipc selectionCount), not 1"
    [[ "$(ipc selectedIndices)" == "$want_index" ]] || fail "select: selectedIndices is $(ipc selectedIndices), not $want_index"

    # A missing target still opens its directory, with nothing selected.
    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    # The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
    QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" FLEA_PATH="$dir" FLEA_SELECT="$dir/does-not-exist.txt" FLEA_BIN="$flea_bin" \
        flea_test_exec qs -p "$flea_ui/boot" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    wait_listing 3
    [[ "$(ipc path)" == "$dir" ]] || fail "select: a missing target opened $(ipc path), not $dir"
    [[ "$(ipc selectionCount)" == "0" ]] || fail "select: a missing target still selected $(ipc selectionCount)"

    printf 'SELECT reveal=ok missing=ok\n'
    kill_flea
}

case_renderer() {
    kill_flea
    local dir="$fixture_root/renderer"
    sandbox_scratch "$dir"
    local log="$dir/shell.log"
    local relaunched="$dir/relaunch.log"
    : > "$relaunched"
    printf '#!/bin/sh\nprintf "RAN %%s\\n" "$*" >> %q\n' "$relaunched" > "$dir/flea-stub"
    chmod +x "$dir/flea-stub"
    # The marker is set, so the renderer's own name is the only thing standing between this and a retry.
    QSG_RHI_BACKEND=opengl FLEA_RENDERER_AUTOMATIC=1 \
        __EGL_VENDOR_LIBRARY_FILENAMES="$dir/no-such-egl-vendor.json" \
        FLEA_PATH="$dir" FLEA_BIN="$dir/flea-stub" \
        flea_test_exec qs -p "$flea_ui/boot" > "$log" 2>&1 </dev/null &
    local waited
    for waited in $(seq 1 200); do
        grep -aq 'graphics backend opengl failed' "$log" && break
        sleep 0.1
    done
    kill_flea
    printf 'RENDERER ran=%q\n' "$(tr '\n' ' ' < "$relaunched")"
    grep -aq 'graphics backend opengl failed' "$log" \
        || fail "no scene-graph error reached ui/boot/shell.qml, so its Connections never held the window"
    # A scene-graph failure now starts no backend at all, so counting FLEA_BIN is a zero denominator.
    local retried
    retried=$(grep -c -- '--gui' "$relaunched" || true)
    [[ "$retried" == "0" ]] || fail "the retry fired for a renderer the operator named: $(cat "$relaunched")"
    # The argv side of that arm, from a root carrying ui/boot's own two symlinks: the helper implicitly imports ui/, whose qmldir singletons import qs.Commons, and qs: resolves against the root.
    local proberoot="$dir/retry-cfg" probe="$dir/retry-probe.log"
    mkdir -p "$proberoot"
    cp -a "$flea_ui/boot/Commons" "$flea_ui/boot/Ui" "$proberoot/" \
        || fail "the boot root has no Commons or Ui to copy, so this probe would test its own fake root"
    cp "$repo/tests/renderer-retry.qml" "$proberoot/probe.qml"
    : > "$probe"
    env RETRY_HELPER="$flea_ui/RendererRetry.qml" RETRY_BACKEND=vulkan \
        FLEA_RENDERER_AUTOMATIC=1 FLEA_BIN="$dir/flea-stub" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
        timeout 20 qs -p "$proberoot/probe.qml" > "$probe" 2>&1
    local want='RETRY argv ["/usr/bin/env","QSG_RHI_BACKEND=opengl","'"$dir/flea-stub"'","--gui"]'
    grep -aqF "$want" "$probe" \
        || fail "the retry helper did not answer with the argv Renderer.js names: $(grep -a RETRY "$probe" | head -2)"
}
