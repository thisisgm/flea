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
  local name=$1 rc
  shift
  "$@" > "$root/$name.log" 2>&1
  rc=$?
  cat "$root/$name.log"
  printf 'GATE %s rc=%s\n' "$name" "$rc"
  [ "$rc" -eq 0 ] || failed=1
}

gate fmt-crates cargo fmt --package flea-gui -- --check
gate clippy tools/flea-clippy tools/clippy-baseline.txt "$root/clippy"
if [ -f "$root/clippy/clippy-counts.txt" ]; then
  sed 's/^/BASELINE /' "$root/clippy/clippy-counts.txt"
fi
if [ -f "$root/clippy/clippy-gui.log" ]; then
  tail -20 "$root/clippy/clippy-gui.log"
fi
gate gpui-lint tools/flea-gpui-lint

# Only the private copy is mutated; no source file in the worktree is overwritten by a test.
python3 - "$root" <<'PY'
from pathlib import Path
import shutil
import sys
root = Path(sys.argv[1])
assert root.is_absolute() and (root / ".flea-test-sandbox").is_file()
mutant = root / "mutant"
mutant.mkdir()
for name in ("Cargo.toml", "Cargo.lock"):
    shutil.copy2(name, mutant / name)
for name in ("src", "crates", "ui", "tests", "shelf"):
    shutil.copytree(name, mutant / name, symlinks=True)
source = mutant / "crates/flea-gui/src/main.rs"
text = source.read_text()
assert text.count("fn main() {") == 1
source.write_text(text.replace("fn main() {", "fn main() {\n    let g10l_injected_warning = ();", 1))
PY
copy_rc=$?
if [ "$copy_rc" -eq 0 ]; then
  cargo clippy --locked --manifest-path "$root/mutant/Cargo.toml" -p flea-gui --all-targets -- -D warnings > "$root/mutant.log" 2>&1
  rc=$?
  if [ "$rc" -ne 0 ] && grep -q 'unused variable: `g10l_injected_warning`' "$root/mutant.log"; then
    echo "ok strict flea-gui warning mutant rejected; cargo rc=$rc"
  else
    cat "$root/mutant.log"
    echo "FAIL strict flea-gui mutant did not fail for its injected warning; cargo rc=$rc"
    failed=1
  fi
else
  echo "FAIL strict flea-gui mutant could not be staged; rc=$copy_rc"
  failed=1
fi
exit "$failed"
