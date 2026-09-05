#!/bin/bash
# The committed ui/js/Keymap.js must be exactly what the generator produces from keys.toml.
set -u
cd "$(dirname "$0")/.." || exit 1

tmp=$(mktemp)
probe_dir=$(mktemp -d)
# Same check budget.sh carries, because a finding on one instance is a finding on the class: GNU
# mktemp honours a relative TMPDIR verbatim, so both paths are checked absolute and two components
# deep before the trap that deletes them is installed.
for p in "$tmp" "$probe_dir"; do
  case $p in
    /*/*) ;;
    *) echo "FAIL: mktemp gave '$p', which is not an absolute path two components deep"; exit 1 ;;
  esac
done
trap 'rm -f "$tmp"; rm -rf "$probe_dir"' EXIT

./tools/flea-keymap-gen "$tmp" || { echo "FAIL the generator did not run"; exit 1; }

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
