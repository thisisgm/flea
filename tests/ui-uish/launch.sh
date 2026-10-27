# GPUI overrides for UISH-05; seeded from the unchanged oracle at 2faf8d099.
case_select() {
    local dir="$fixture_root/select"
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"

    # The target reaches the launcher as argv, the same way `flea --gui --select <target> <dir>` does.
    launch "$dir" "$dir/b.txt"
    wait_listing 3
    [[ "$(ipc path)" == "$dir" ]] || fail "select: opened $(ipc path), not $dir"
    local want_index
    want_index=$(row_index_of "b.txt")
    [[ "$(ipc cursor)" == "$want_index" ]] || fail "select: cursor is $(ipc cursor), not row $want_index"
    [[ "$(ipc selectionCount)" == "1" ]] || fail "select: selectionCount is $(ipc selectionCount), not 1"
    [[ "$(ipc selectedIndices)" == "$want_index" ]] || fail "select: selectedIndices is $(ipc selectedIndices), not $want_index"

    # A missing target still opens its directory, with nothing selected.
    launch "$dir" "$dir/does-not-exist.txt"
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
    local log="$dir/window.log" fallback="$dir/fallback.log" message waited
    # Sample input: pub const MESSAGE: &str = "flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui";
    message=$(grep -o 'pub const MESSAGE: &str = "[^"]*"' "$repo/crates/flea-gui/src/renderer.rs" | cut -d'"' -f2)
    [[ "$message" == "flea: "* ]] || fail "crates/flea-gui/src/renderer.rs names no flea: line for a window no driver could open, found '$message'"
    # No Vulkan driver and no EGL vendor, so neither backend GPUI tries in turn can open the window; the Qt-era renderer names stay set as an operator's shell may have them.
    QSG_RHI_BACKEND=opengl FLEA_RENDERER_AUTOMATIC=1 \
        VK_DRIVER_FILES="$dir/no-such-icd.json" VK_ICD_FILENAMES="$dir/no-such-icd.json" \
        __EGL_VENDOR_LIBRARY_FILENAMES="$dir/no-such-egl-vendor.json" \
        FLEA_PATH="$dir" FLEA_BIN="$flea_bin" \
        flea_test_exec "$flea_bin" --gui "$dir" > "$log" 2>&1 </dev/null &
    local launcher=$!
    for waited in $(seq 1 200); do
        grep -aqF -- "$message" "$log" && break
        sleep 0.1
    done
    # The launcher returns once the window's process group has drained, so a retry would have printed by then.
    for waited in $(seq 1 400); do
        kill -0 "$launcher" 2>/dev/null || break
        sleep 0.1
    done
    kill_flea
    local failed
    failed=$(grep -acF -- "$message" "$log" || true)
    printf 'RENDERER failed=%s\n' "$failed"
    [[ "$failed" != 0 ]] \
        || fail "no graphics failure reached flea-gui's startup, so a window no driver could open was never caught: $(tail -n 3 "$log" | tr '\n' ' ')"
    [[ "$failed" == 1 ]] || fail "the window was started again after its graphics failed, the failure was said $failed times"
    # Sample input: "flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui" then "No GPU adapters found".
    local cause
    cause=$(grep -aA1 -F -- "$message" "$log" | sed -n 2p)
    printf 'RENDERER cause=%q\n' "$cause"
    # Every error gpui-pre-wgpu returns for a refused window names an adapter, a surface, a device or its alpha modes.
    grep -qiE 'adapter|surface|device|alpha mode' <<< "$cause" \
        || fail "the graphics failure named no backend cause under its message: $(tail -n 3 "$log" | tr '\n' ' ')"
    # Control: with the box's own drivers the window opens on Vulkan, so GL below is the fallback, not the only driver.
    local control="$dir/control.log"
    FLEA_GPUI_CAPTURE=1 FLEA_PATH="$dir" FLEA_BIN="$flea_bin" \
        flea_test_exec "$flea_bin" --gui "$dir" > "$control" 2>&1 </dev/null &
    for waited in $(seq 1 200); do
        grep -aq '^GPUI_RENDERER ' "$control" && break
        sleep 0.1
    done
    kill_flea
    printf 'RENDERER control=%s\n' "$(grep -a -m1 '^GPUI_RENDERER ' "$control" | cut -d' ' -f2)"
    grep -aq '^GPUI_RENDERER backend=vulkan ' "$control" \
        || fail "this box opened no window on Vulkan, so a GL window with Vulkan removed would prove no fallback: $(grep -a -m1 '^GPUI_RENDERER ' "$control" || tail -n 3 "$control" | tr '\n' ' ')"
    # The automatic fallback is in-process: Vulkan with no driver falls to GL in the same window, never a relaunch.
    QSG_RHI_BACKEND=vulkan FLEA_GPUI_CAPTURE=1 \
        VK_DRIVER_FILES="$dir/no-such-icd.json" VK_ICD_FILENAMES="$dir/no-such-icd.json" \
        FLEA_PATH="$dir" FLEA_BIN="$flea_bin" \
        flea_test_exec "$flea_bin" --gui "$dir" > "$fallback" 2>&1 </dev/null &
    for waited in $(seq 1 200); do
        grep -aq '^GPUI_RENDERER ' "$fallback" && break
        sleep 0.1
    done
    kill_flea
    printf 'RENDERER fallback=%s\n' "$(grep -a -m1 '^GPUI_RENDERER ' "$fallback" | cut -d' ' -f2)"
    grep -aq '^GPUI_RENDERER backend=gl ' "$fallback" \
        || fail "a window with no Vulkan driver did not fall back to GL in-process: $(grep -a -m1 '^GPUI_RENDERER ' "$fallback" || tail -n 3 "$fallback" | tr '\n' ' ')"
    ! grep -aqF -- "$message" "$fallback" || fail "a window GL could open still said no graphics driver could open it"
}
