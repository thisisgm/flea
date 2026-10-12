#!/usr/bin/env bash
# Real browser window and backend without a compositor; wl-copy records only Copy as text.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
# GVfs must leave with this temporary home, so later suites never inherit its trash store through D-Bus.
if [ "${FLEA_HUNT_PRIVATE_BUS:-}" != 1 ]; then
    command -v dbus-run-session >/dev/null || { echo 'FAIL dbus-run-session is unavailable'; exit 1; }
    exec dbus-run-session -- env FLEA_HUNT_PRIVATE_BUS=1 bash tests/menu-clipboard-hunt.sh "$@"
fi
python3 tests/menu-clipboard-checks-test.py || exit 1
command -v qs >/dev/null || { echo 'FAIL qs is unavailable'; exit 1; }
sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-menu-clipboard-hunt.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT
mkdir -p "$test_root"/{home,state,data,cache,runtime,config,bin,source,copy-dest,cut-dest,pasteas-dest,pasteas-absolute-dest,pasteas-hard-dest,original-dest} || exit 1
chmod 700 "$test_root/runtime" || exit 1
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
ln -s "$(readlink -f ui/boot/Commons)" "$test_root/config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
# The window loads its tab catcher from the shell directory, so the probe ships it as the product does.
ln -s "$PWD/ui/boot/fleatab.qml" "$test_root/config/fleatab.qml" || exit 1
cp tests/menu-clipboard-hunt.qml "$test_root/config/shell.qml" || exit 1
printf 'alpha contents\n' > "$test_root/source/alpha.txt"
printf 'beta contents\n' > "$test_root/source/beta.txt"
python3 tests/menu-clipboard-checks.py record fixture "$test_root/source" "$test_root/source-bytes.json" || exit 1
for link_dest in pasteas pasteas-absolute pasteas-hard; do
    printf 'canary contents\n' > "$test_root/$link_dest-dest/canary.txt"
done
ln -s ../source/alpha.txt "$test_root/original-dest/file-link" || exit 1
ln -s / "$test_root/original-dest/root-link" || exit 1
# Copy as still publishes text through wl-copy; file Copy and Cut must never send URIs here.
cat > "$test_root/bin/wl-copy" <<'PY'
#!/usr/bin/env python3
import json, os, sys
path = os.environ["FLEA_HUNT_CLIPBOARD"]
text = sys.stdin.read()
with open(path, "w") as output:
    output.write(text)
with open(path + ".calls", "a") as output:
    output.write(json.dumps({"args": sys.argv[1:], "text": text}) + "\n")
PY
chmod +x "$test_root/bin/wl-copy" || exit 1
# Intercept terminal launch only; the candidate owns every backend and state request.
cat > "$test_root/bin/flea-hunt" <<'SH'
#!/bin/sh
if [ "${1:-}" = --terminal ]; then
    printf '%s\n' "$2" >> "$FLEA_HUNT_CLIPBOARD.terminals"
    exit 0
fi
exec "$FLEA_HUNT_REAL_BIN" "$@"
SH
chmod +x "$test_root/bin/flea-hunt" || exit 1
failed=0
checks=0
failures=0
# Native ui:clipboard in tests/ui.sh owns fresh-window system Copy/Cut paste, including bytes and all selected destinations.
for action in copy cut copyas-list copyas-grid copyas-columns terminal pasteas pasteas-absolute pasteas-hard original; do
    rm -rf "$test_root/source/sub"
    printf 'alpha contents\n' > "$test_root/source/alpha.txt"
    printf 'beta contents\n' > "$test_root/source/beta.txt"
    if [[ "$action" == copyas-* ]]; then
        mkdir -p "$test_root/source/sub"
        mv "$test_root/source/alpha.txt" "$test_root/source/sub/alpha.txt"
        mv "$test_root/source/beta.txt" "$test_root/source/sub/beta.txt"
    fi
    start="$test_root/source"
    [ "$action" = original ] && start="$test_root/original-dest"
    dest="$test_root/$action-dest"
    [ "$action" = terminal ] && dest="$test_root/copy-dest"
    : > "$test_root/clipboard"
    : > "$test_root/clipboard.calls"
    : > "$test_root/clipboard.terminals"
    output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CONFIG_HOME="$test_root/home/config" \
        XDG_DATA_HOME="$test_root/data" XDG_CACHE_HOME="$test_root/cache" XDG_RUNTIME_DIR="$test_root/runtime" \
        PATH="$test_root/bin:$PATH" FLEA_BIN="$test_root/bin/flea-hunt" FLEA_HUNT_REAL_BIN="$PWD/target/debug/flea" FLEA_PATH="$start" \
        FLEA_HUNT_ACTION="$action" FLEA_HUNT_DEST="$dest" FLEA_HUNT_CLIPBOARD="$test_root/clipboard" \
        FLEA_HUNT_SOURCE="$test_root/source" FLEA_HUNT_CHECKS="$PWD/tests/menu-clipboard-checks.py" \
        FLEA_HUNT_SOURCE_BYTES="$test_root/source-bytes.json" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 timeout 30 qs -p "$test_root/config" 2>&1)
    code=$?
    printf '%s\n' "$output" | grep -a 'CLIPHUNT'
    probe_checks=$(printf '%s\n' "$output" | grep -acE 'CLIPHUNT (PASS|FAIL)' || true)
    probe_failures=$(printf '%s\n' "$output" | grep -ac 'CLIPHUNT FAIL' || true)
    checks=$((checks + probe_checks + 1))
    failures=$((failures + probe_failures))
    if [ "$code" -ne 143 ] || ! printf '%s\n' "$output" | grep -aq "CLIPHUNT DONE action=$action"; then
        printf 'FAIL %s probe did not complete: exit=%s\n' "$action" "$code"
        printf '%s\n' "$output" | tail -12
        failed=1
        failures=$((failures + 1))
    fi
    if [[ "$action" == copy || "$action" == cut ]]; then
        checks=$((checks + 1))
        if ! python3 tests/menu-clipboard-checks.py publication "$action" "$test_root/clipboard.calls" "$test_root/source"; then
            failed=1
            failures=$((failures + 1))
        fi
    fi
    if printf '%s\n' "$output" | grep -aq 'CLIPHUNT FAIL'; then failed=1; fi
    if [[ "$action" == copyas-* ]]; then
        checks=$((checks + 1))
        # Sample input: {"text":"/source/sub/alpha.txt\n/source/sub/beta.txt"} is one Copy as publication.
        # Files live in sub/ under the listed directory, so Relative path is sub/name and Name is the leaf.
        python3 - "$test_root/clipboard.calls" "$test_root/source" <<'PY'
import json, pathlib, sys
calls = [json.loads(line)["text"] for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
root = pathlib.Path(sys.argv[2])
want = {str(root / "sub" / "alpha.txt"), str(root / "sub" / "beta.txt")}
paths = calls[0].split("\n") if calls else []
if set(paths) != want or len(paths) != 2:
    print("FAIL Copy as Path did not copy both nested files:", repr(calls))
    sys.exit(1)
relative = "\n".join("sub/" + pathlib.Path(p).name for p in paths)
names = "\n".join(pathlib.Path(p).name for p in paths)
stems = "\n".join(pathlib.Path(p).stem for p in paths)
folder = str(root / "sub")
if relative == names:
    print("FAIL Relative path matched Name; nested files required:", repr(relative))
    sys.exit(1)
expected = ["\n".join(paths), relative, names, stems, folder + "\n" + folder,
            "\n".join(pathlib.Path(p).as_uri() for p in paths),
            "\n".join("'" + p + "'" for p in paths), "\n".join(paths)]
if calls != expected:
    print("FAIL Copy as leaves or Ctrl+Shift+C did not copy the whole selection:", repr(calls))
    sys.exit(1)
print("PASS seven Copy as leaves and Ctrl+Shift+C copied both selected files")
PY
        if [ "$?" -ne 0 ]; then failed=1; failures=$((failures + 1)); fi
    elif [[ "$action" == terminal ]]; then
        checks=$((checks + 1))
        if ! python3 tests/menu-clipboard-checks.py terminals "$action" "$test_root/clipboard.terminals" "$start" "$dest"; then
            failed=1
            failures=$((failures + 1))
        fi
    elif [[ "$action" != original ]]; then
        for file in alpha.txt beta.txt; do
            checks=$((checks + 1))
            if [ ! -f "$test_root/$action-dest/$file" ]; then
                printf 'FAIL %s destination lacks %s\n' "$action" "$file"
                failed=1
                failures=$((failures + 1))
            fi
        done
        checks=$((checks + 1))
        if ! python3 tests/menu-clipboard-checks.py files "$action" "$test_root/source" "$dest" "$test_root/source-bytes.json"; then
            failed=1
            failures=$((failures + 1))
        fi
    fi
    # Offscreen cannot set window masks; every other runtime error is significant.
    warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN|ERROR' \
        | grep -vF 'This plugin does not support setting window masks' || true)
    checks=$((checks + 1))
    if [ -n "$warnings" ]; then
        printf 'FAIL %s runtime warnings\n%s\n' "$action" "$warnings"
        failed=1
        failures=$((failures + 1))
    fi
done
printf 'menu-clipboard-hunt: %s checks, %s failed\n' "$checks" "$failures"
[ "$failed" -eq 0 ]
