#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# The persistent SVG cache through the real helper: a repeat answers from disk with no helper, and no other theme or advance is served it.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
command -v qs >/dev/null || { echo "figure-store.sh: qs is not installed"; exit 1; }
resolve_qjs() {
    if [ -n "${FLEA_QJS:-}" ] && [ "${FLEA_QJS#/}" != "${FLEA_QJS}" ] && [ -x "${FLEA_QJS}" ]; then
        printf '%s\n' "${FLEA_QJS}"
    elif [ -x /usr/bin/qjs ]; then
        printf '%s\n' /usr/bin/qjs
    elif [ -x "$PWD/.superpowers/tools/qjs" ]; then
        printf '%s\n' "$PWD/.superpowers/tools/qjs"
    else
        return 1
    fi
}
qjs=$(resolve_qjs) || { echo "figure-store.sh: no qjs (FLEA_QJS, /usr/bin/qjs)"; exit 1; }
fleabin="$PWD/target/debug/flea"
[ -x "$fleabin" ] || { echo "figure-store.sh: no debug binary at $fleabin, run cargo build first"; exit 1; }
export FLEA_UI="$PWD/ui"
# The SVG cache is what is under test, so the bytecode cache and its background build stay off.
export FLEA_FIGURE_CACHE=svg

test_root="$FIXTURE_ROOT/flea-figure-store-$$"
sandbox_make "$test_root"
trap 'sandbox_remove "$test_root"' EXIT
mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/figure-store.qml "$test_root/config/shell.qml" || exit 1

# The jail the helper runs in needs user namespaces; a box without them cannot draw a figure, and says so.
probe=$(printf '%s\n' '{"id":1,"kind":"math","source":"x","display":false,"theme":{"bg":"#101315","fg":"#c0caf5"}}' | FLEA_QJS="$qjs" "$fleabin" --figure-helper 2>&1)
if ! printf '%s\n' "$probe" | grep -q '"svg"'; then
    if printf '%s\n' "$probe" | grep -q '^bwrap: No permissions to create new namespace'; then
        echo "figure-store.sh: SKIP no user namespaces in this container, so no jailed helper to draw with"
        exit 0
    fi
    printf 'figure-store.sh: FAIL the helper did not answer: %s\n' "$probe"
    exit 1
fi

# A store whose reply the held case withholds: with a marker file armed, the stub's next store start waits on a gate fifo before it execs Flea's own store.
mkdir -p "$test_root/gate-bin" || exit 1
mkfifo "$test_root/gate" || exit 1
cat > "$test_root/gate-bin/flea" <<STUB
#!/bin/bash
if [ "\$1" = "--figure-store" ] && [ -e "$test_root/hold" ]; then
    rm -f "$test_root/hold"
    read -r _ < "$test_root/gate"
fi
exec "$fleabin" "\$@"
STUB
chmod +x "$test_root/gate-bin/flea" || exit 1
# A document with one maths and one Mermaid figure no other case draws, for the pane that draws it and the fresh pane that only warms.
printf '# Placed\n\n```math\n\\frac{p}{q}+\\sqrt{r}\n```\n\n```mermaid\nflowchart TD\n    P --> Q\n```\n' > "$test_root/placed.md"

# Figures only inside an item, a quote, a quote in an item, an item in that quote and a quote in an item in a quote, then the same with a top-level figure.
cat > "$test_root/nested.md" <<'NESTED'
# Nested

- point
  ```math
  \frac{n}{m}
  ```

> ```mermaid
> flowchart TD
>     N --> M
> ```

- outer
  > inner
  >
  > - leaf
  >   ```math
  >   \sqrt{u}+v
  >   ```
  >
  > ```mermaid
  > flowchart TD
  >     U --> V
  > ```

> - held
>   > ```math
>   > \sqrt{z}-y
>   > ```
NESTED
cat > "$test_root/mixed.md" <<'MIXED'
# Mixed

```math
\frac{t}{w}
```

- point
  ```math
  \frac{s}{o}
  ```

> ```mermaid
> flowchart TD
>     X --> Y
> ```

- outer
  > inner
  >
  > - leaf
  >   ```math
  >   \sqrt{k}+j
  >   ```
  >
  > ```mermaid
  > flowchart TD
  >     J --> K
  > ```

> - held
>   > ```math
>   > \sqrt{g}-h
>   > ```
MIXED

output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_BIN="$test_root/gate-bin/flea" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    FLEA_FIGURE_STORE_DOC="$test_root/placed.md" FLEA_FIGURE_NEST_DOC="$test_root/nested.md" FLEA_FIGURE_MIXED_DOC="$test_root/mixed.md" FLEA_STORE_HOLD="$test_root/hold" FLEA_STORE_GATE="$test_root/gate" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 120 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$output" | grep -aoE 'FIGURE_STORE .*'
# Sample input: FIGURE_STORE 31 checks, 0 failed.
expected=31
if ! printf '%s\n' "$output" | grep -qE "FIGURE_STORE $expected checks, 0 failed$"; then
    printf 'figure-store.sh: FAIL expected %s checks, 0 failed\n' "$expected"
    printf '%s\n' "$output" | grep -aE 'ERROR|TypeError|ReferenceError|flea:' | head -10
    exit 1
fi
platform_warning='This plugin does not support setting window masks'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
if [ -n "$warnings" ]; then
    printf 'figure-store.sh: FAIL the harness logged a warning\n%s\n' "$warnings" | head -10
    exit 1
fi
echo "figure-store: $expected check(s), 0 failed"

# A store that reads every line and never answers: the same service, with Flea's own binary behind a stub whose store mode only reads.
mkdir -p "$test_root/hung-config" "$test_root/hung-bin" || exit 1
ln -s "$PWD/ui" "$test_root/hung-config/flea" || exit 1
headless_commons "$test_root" "$test_root/hung-config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/hung-config/Ui" || exit 1
cp tests/figure-store-hung.qml "$test_root/hung-config/shell.qml" || exit 1
cat > "$test_root/hung-bin/flea" <<STUB
#!/bin/bash
[ "\$1" = "--figure-store" ] && exec cat > /dev/null
exec "$fleabin" "\$@"
STUB
chmod +x "$test_root/hung-bin/flea" || exit 1
output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_BIN="$test_root/hung-bin/flea" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 120 qs -p "$test_root/hung-config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$output" | grep -aoE 'FIGURE_STORE_HUNG .*'
# Sample input: FIGURE_STORE_HUNG 3 checks, 0 failed.
hung_expected=3
if ! printf '%s\n' "$output" | grep -qE "FIGURE_STORE_HUNG $hung_expected checks, 0 failed$"; then
    printf 'figure-store.sh: FAIL the hung store was not failed: expected %s checks, 0 failed\n' "$hung_expected"
    printf '%s\n' "$output" | grep -aE 'ERROR|TypeError|ReferenceError|flea:' | head -10
    exit 1
fi
echo "figure-store: $hung_expected hung-store check(s), 0 failed"

# Two drains the service must tell apart: a store that answers every line and ignores EOF, and one that commits its puts slowly after EOF and then exits.
mkdir -p "$test_root/drain-config" "$test_root/drain-bin" || exit 1
ln -s "$PWD/ui" "$test_root/drain-config/flea" || exit 1
headless_commons "$test_root" "$test_root/drain-config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/drain-config/Ui" || exit 1
cp tests/figure-store-drain.qml "$test_root/drain-config/shell.qml" || exit 1
# The hung stub never ends; the slow stub keeps every line, answers a miss for each get, waits out its commit after EOF and only then replays the lines into Flea's own store, so a kill before the end loses the put. Its first start only is slow.
cat > "$test_root/drain-bin/flea" <<STUB
#!/bin/bash
if [ "\$1" = "--figure-store" ]; then
    if [ "\$FLEA_DRAIN_MODE" = hang ]; then
        # Sample input: {"op":"get","id":3,"key":"..."} answers {"id":3,"miss":true}; a put has no id and no reply.
        sed -u -n 's/.*"id":\([0-9]*\).*/{"id":\1,"miss":true}/p'
        exec sleep 120
    elif [ ! -e "$test_root/slow-used" ]; then
        : > "$test_root/slow-used"
        # Sample input: {"op":"get","id":4,"key":"..."} answers {"id":4,"miss":true} and every line, puts included, is kept for the replay.
        tee "$test_root/slow-lines" | sed -u -n 's/.*"id":\([0-9]*\).*/{"id":\1,"miss":true}/p'
        sleep 4
        exec "$fleabin" "\$@" < "$test_root/slow-lines"
    fi
fi
exec "$fleabin" "\$@"
STUB
chmod +x "$test_root/drain-bin/flea" || exit 1
run_drain() {
    output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
        HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
        XDG_RUNTIME_DIR="$test_root/runtime" FLEA_BIN="$test_root/drain-bin/flea" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
        FLEA_DRAIN_MODE="$1" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
        timeout 120 qs -p "$test_root/drain-config" 2>&1 ) 2>/dev/null )
    printf '%s\n' "$output" | grep -aoE 'FIGURE_STORE_DRAIN .*'
    # Sample input: FIGURE_STORE_DRAIN 5 checks, 0 failed.
    if ! printf '%s\n' "$output" | grep -qE "FIGURE_STORE_DRAIN $2 checks, 0 failed$"; then
        printf 'figure-store.sh: FAIL the %s drain: expected %s checks, 0 failed\n' "$1" "$2"
        printf '%s\n' "$output" | grep -aE 'ERROR|TypeError|ReferenceError|flea:' | head -10
        drain_failed=1
        return
    fi
    echo "figure-store: $2 $1 drain check(s), 0 failed"
}
drain_failed=0
run_drain hang 5
# The slow drain reads its put back from the store directory the stub's real store wrote.
run_drain slow 4
[ "$drain_failed" = 0 ] || exit 1
