#!/bin/bash
# The committed ui/js/Keymap.js must be exactly what the generator produces from keys.toml.
set -u
cd "$(dirname "$0")/.." || exit 1

probe_dir=$(mktemp -d /tmp/flea-keymap.XXXXXXXX) || exit 1
tmp=$probe_dir/Keymap.js
cleanup() {
  # The test owns this mktemp root; every deletion must still pass its absolute-path guard.
  case "$probe_dir" in /tmp/flea-keymap.?*) ;; *) return 1 ;; esac
  local path=$probe_dir
  case "$path" in "$probe_dir"|"$probe_dir"/*) rm -rf -- "$path" ;; *) return 1 ;; esac
}
trap cleanup EXIT

./tools/flea-keymap-gen "$tmp" || { echo "FAIL the generator did not run"; exit 1; }

# SettingsKeys.html says conflicts fail the build, and until now they did not: a second mac ctrl-1
# claiming viewGrid emitted two overlay rows, exited 0, and let the first silently win. The broken
# table is a copy inside the probe dir, so the one this repo ships is never edited to prove this.
conflict=$probe_dir/conflict.toml
cp keys.toml "$conflict"
cat >> "$conflict" <<'CONFLICT'

[[preset]]
name = "mac"
mods = "ctrl"
key = "1"
keys = "ctrl-1"
action = "viewGrid"
label = "grid view"
CONFLICT
if ./tools/flea-keymap-gen "$probe_dir/conflict.js" "$conflict" 2>"$probe_dir/conflict.err"; then
  echo "FAIL the generator accepted a preset claiming ctrl-1 twice"
  exit 1
fi
if grep -q 'the mac preset claims ctrl-1 twice, for viewList and viewGrid' "$probe_dir/conflict.err"; then
  echo "ok   a chord claimed twice inside one preset fails the build, naming both actions"
else
  echo "FAIL the duplicate chord was refused without naming the preset, the chord and both actions:"
  cat "$probe_dir/conflict.err"
  exit 1
fi

# A mistyped name emits a comparison against undefined, which is false forever and diffs clean.
if ! command -v qml6 >/dev/null; then
  echo "FAIL qml6 is not installed, cannot check the key names in keys.toml"
  exit 1
fi

{
  echo "import QtQuick"
  echo "Item { Component.onCompleted: {"
  echo "    var bad = []"
  grep -o 'Qt\.Key_[A-Za-z0-9_]*' "$tmp" | sort -u | while read -r name; do
    echo "    if ($name === undefined) bad.push(\"$name\")"
  done
  echo "    for (var i = 0; i < bad.length; i++) console.log(\"FAIL \" + bad[i] + \" is not a Qt key\")"
  echo "    Qt.exit(bad.length === 0 ? 0 : 1)"
  echo "} }"
} > "$probe_dir/probe.qml"

# qml6 routes console.log to the systemd journal, not stderr, unless told otherwise.
if QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 qml6 "$probe_dir/probe.qml" 2>&1; then
  echo "ok   every key name in keys.toml is a real Qt key"
else
  echo "FAIL keys.toml names a key Qt does not define"
  exit 1
fi

# The pointer table's own "does" legend, checked against the rows under it rather than read: it has
# named an effect no row uses and missed two that rows do, through two rounds of editing that line.
# grep drops the empty token a trailing space on that line would otherwise sort to the front of.
legend=$(sed -n 's/^# does   *//p' keys.toml | tr ' ' '\n' | grep -v '^$' | sort -u | tr '\n' ' ')
effects=$(sed -n 's/^does = "\(.*\)"$/\1/p' keys.toml | sort -u | tr '\n' ' ')
# The denominator, because both extractions are sed against a format: shift either line's spelling
# and that side yields nothing, and with both shifted empty equalled empty and this printed ok.
effect_count=$(printf '%s' "$effects" | wc -w)
if [ "$effect_count" -eq 0 ]; then
  echo "FAIL the pointer rows yielded no 'does' value at all, so the legend check compared nothing"
  exit 1
fi
if [ "$legend" = "$effects" ]; then
  echo "ok   the pointer legend names exactly the effects its own rows use"
else
  echo "FAIL the pointer legend reads '$legend' and the rows use '$effects'"
  exit 1
fi

if diff -u ui/js/Keymap.js "$tmp"; then
  echo "ok   ui/js/Keymap.js matches keys.toml"
  exit 0
fi
echo "FAIL ui/js/Keymap.js is stale, run ./tools/flea-keymap-gen"
exit 1
