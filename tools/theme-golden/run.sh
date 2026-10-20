#!/bin/bash
# Run on the controller's Omarchy machine; all generated files stay in a fresh marked scratch root.
set -euo pipefail
export LC_ALL=C.UTF-8
unset LANGUAGE
W=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
commons=${1:?usage: run.sh OMARCHY_SHELL FROZEN_UI [PALETTE_ROOT...]}
oracle=${2:?usage: run.sh OMARCHY_SHELL FROZEN_UI [PALETTE_ROOT...]}
shift 2
[[ "$commons" = /* && "$oracle" = /* ]] || { echo "absolute source paths required" >&2; exit 2; }
[[ -f "$commons/Commons/Color.qml" && -f "$oracle/ThemeColors.qml" ]] || exit 2
command -v qs >/dev/null || { echo "Quickshell is required" >&2; exit 2; }
mkdir -p -- "$W/.superpowers/g05"
scratch=$(mktemp -d "$W/.superpowers/g05/golden.XXXXXX")
[[ "$scratch" = "$W"/.superpowers/g05/golden.?* ]] || exit 2
printf '%s\n' 'g05 theme golden scratch' > "$scratch/.g05-test-root"
mkdir -p -- "$scratch/home/.local/state/omarchy/current/theme" "$scratch/home/.local/state/flea" \
    "$scratch/cache" "$scratch/config" "$scratch/runtime"
chmod 700 -- "$scratch/runtime"
printf '%s\n' 'background = "#101315"' 'foreground = "#cacccc"' > "$scratch/home/.local/state/omarchy/current/theme/colors.toml"
printf '%s\n' '{"display":{"textSize":{"mode":"system"}}}' > "$scratch/home/.local/state/flea/ui.json"
cp -- "$W/tools/theme-golden/dump.qml" "$scratch/shell.qml"
ln -s -- "$commons/Commons" "$scratch/Commons"
ln -s -- "$oracle" "$scratch/Oracle"
if [ $# -eq 0 ]; then
    set -- "${OMARCHY_PATH:-$HOME/.local/share/omarchy}/themes" "$HOME/.config/omarchy/themes"
fi
python3 "$W/tools/theme-golden/manifest.py" "$scratch" "$commons" "$@"
set +e
HOME="$scratch/home" XDG_CONFIG_HOME="$scratch/config" XDG_CACHE_HOME="$scratch/cache" \
    XDG_STATE_HOME="$scratch/home/.local/state" XDG_RUNTIME_DIR="$scratch/runtime" FLEA_GOLDEN_MANIFEST="$scratch/palettes.json" \
    QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=offscreen \
    timeout --signal=TERM 120s qs -p "$scratch" > "$scratch/qs.log" 2>&1
status=$?
set -e
if [ "$status" -eq 124 ]; then
    cat "$scratch/qs.log" >&2
    printf 'QML golden dump timed out after 120 seconds; inspect %s/qs.log\n' "$scratch" >&2
    exit 124
fi
python3 - "$scratch" "$status" <<'PY'
import json
import pathlib
import re
import sys

scratch = pathlib.Path(sys.argv[1])
status = int(sys.argv[2])
log = (scratch / 'qs.log').read_text()
lines = log.splitlines()
def fail(message):
    print(log, file=sys.stderr)
    raise SystemExit(f'{message}; qs exit {status}; inspect {scratch}/qs.log')

errors = [line for line in lines if 'THEME_GOLDEN_FAIL' in line or
          ('THEME_GOLDEN\t' not in line and re.search(r'\b(?:Type|Reference|Syntax|Range|Eval|URI)?Error:', line))]
if errors:
    fail('QML golden dump failed')
rows = [line.split('THEME_GOLDEN\t', 1)[1] for line in lines if 'THEME_GOLDEN\t' in line]
done = [line for line in lines if 'THEME_GOLDEN_DONE' in line]
if len(done) != 1 or not rows:
    fail('incomplete QML dump')
expected = {p['name'] for p in json.loads((scratch / 'palettes.json').read_text())}
cases = {(r.split('\t')[0], r.split('\t')[1]) for r in rows}
if cases != {(name, str(stop)) for name in expected for stop in (12, 14)}:
    raise SystemExit('palette/stop coverage mismatch')
if len(rows) != len(set('\t'.join(r.split('\t')[:3]) for r in rows)):
    raise SystemExit('duplicate token rows')
print('# QML golden, live Omarchy Commons plus frozen Flea QML, tag rule 2026-10-17')
print('# palette\tstop\ttoken\tvalue; colors are #AARRGGBB; input.* retains reproducible inputs')
print('\n'.join(rows))
print(f'evidence: {scratch}', file=sys.stderr)
PY
