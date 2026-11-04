#!/bin/bash
# Real workspace gates and a strict-Clippy warning mutant run through the suite broker.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
mkdir -p .superpowers/g10l || exit 1
root=$(mktemp -d "$PWD/.superpowers/g10l/gates.XXXXXX") || exit 1
touch "$root/.flea-test-sandbox" || exit 1
trap 'case "$root" in /*/.superpowers/g10l/gates.?*) [ -n "$root" ] && [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac' EXIT
failed=0
export CARGO_TARGET_DIR="$root/target"
export CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_PROFILE_TEST_DEBUG=line-tables-only

gate() {
  local name=$1
  shift
  "$@" > "$root/$name.log" 2>&1
  gate_rc=$?
  cat "$root/$name.log"
  printf 'GATE %s rc=%s\n' "$name" "$gate_rc"
  [ "$gate_rc" -eq 0 ] || failed=1
}

gate fmt-crates cargo fmt --package flea-gui -- --check
gate system-font cargo test --locked -p flea-gui --lib font_gate -- --nocapture
# A filter that matches no test exits 0, so the surface verdict line must be there too.
if ! grep -qx 'FONT_GATE failed_surfaces=\[\]' "$root/system-font.log"; then
  echo 'GATE system-font has no FONT_GATE failed_surfaces=[] line'
  failed=1
fi
gate clippy tools/flea-clippy tools/clippy-baseline.txt "$root/clippy"
if [ -f "$root/clippy/clippy-counts.txt" ]; then
  sed 's/^/BASELINE /' "$root/clippy/clippy-counts.txt"
fi
if [ -f "$root/clippy/clippy-gui.log" ]; then
  tail -20 "$root/clippy/clippy-gui.log"
fi
gate gpui-lint tools/flea-gpui-lint
gate frame-gate cargo test --locked -p flea-gui --lib vf_frames -- --nocapture
# A name filter that matches fewer tests still exits 0, so a passing run must report every frame test.
frame_tests=34
# test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 3083 filtered out; finished in 1.45s
if [ "$gate_rc" -eq 0 ] && ! grep -q "^test result: ok\. $frame_tests passed; 0 failed; 0 ignored;" "$root/frame-gate.log"; then
  result=$(grep '^test result:' "$root/frame-gate.log")
  echo "FAIL frame-gate exited 0 but expected $frame_tests passed frame tests, got: ${result:-no test result line}"
  failed=1
fi
gate gate-regressions python3 -B tests/gpui_gates_regressions.py

# Only the private copy is mutated, and it takes every top-level input except VCS, build and scratch state, so the wrapper judges the same tree.
mkdir "$root/mutant" || exit 1
for entry in .[!.]* *; do
  case $entry in .git|.superpowers|target|.flea-local) continue ;; esac
  [ -e "$entry" ] || continue
  cp -R "$entry" "$root/mutant/" || exit 1
done
python3 - "$root" <<'GATES_PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
assert root.is_absolute() and (root / ".flea-test-sandbox").is_file()
source = root / "mutant/crates/flea-gui/src/main.rs"
text = source.read_text()
assert text.count("fn main() {") == 1
source.write_text(text.replace("fn main() {", "fn main() {\n    let g10l_injected_warning = ();", 1))
GATES_PY
copy_rc=$?
if [ "$copy_rc" -eq 0 ]; then
  # The mutant goes through the release wrapper, so its strict flea-gui call is the one that judges it.
  (cd "$root/mutant" && tools/flea-clippy tools/clippy-baseline.txt "$root/mutant-clippy") > "$root/mutant.log" 2>&1
  rc=$?
  # The expected rejection stays out of stdout: the suite runner reads any FAIL line as a failed suite.
  if [ "$rc" -ne 0 ] && grep -qF 'clippy flea-gui: FAIL' "$root/mutant.log" && grep -q '^error: unused variable: `g10l_injected_warning`' "$root/mutant-clippy/clippy-gui.log" 2>/dev/null; then
    echo "ok strict flea-gui warning mutant rejected by tools/flea-clippy; rc=$rc"
  else
    cat "$root/mutant.log"
    echo "FAIL strict flea-gui mutant was not rejected by tools/flea-clippy for its injected error; rc=$rc"
    failed=1
  fi
else
  echo "FAIL strict flea-gui mutant could not be staged; rc=$copy_rc"
  failed=1
fi
exit "$failed"
