#!/bin/bash
# Asserts tools/flea-file-budget rejects an oversized file and accepts a normal tree.
set -u
# The cd comes first: a trap installed above it fires against the caller's working directory when
# the cd fails, which is a relative-path rm -rf in somebody else's tree.
cd "$(dirname "$0")/.." || exit 1
repo=$PWD

# Scratch stays under the marked .superpowers/g10l parent, never in shared /tmp.
mkdir -p "$repo/.superpowers/g10l" || exit 1
scratch=$(mktemp -d "$repo/.superpowers/g10l/budget.XXXXXX") || exit 1
# Cleanup accepts only an absolute path under this suite's marked scratch parent.
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep"; exit 1 ;;
esac
touch "$scratch/.flea-test-sandbox" || exit 1
trap 'case "$scratch" in /*/.superpowers/g10l/budget.?*) [ -n "$scratch" ] && [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label: expected exit $expected, got $actual"
    fail=1
  else
    echo "ok   $label"
  fi
}

# Read-only against the real tree, which is the gate this suite exists to keep.
"$repo/tools/flea-file-budget" >/dev/null 2>&1
check "clean tree passes" 0 $?

mkdir -p "$scratch/src" "$scratch/ui" "$scratch/tests"
( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
check "an empty tree passes too" 0 $?

# A literal one past the Rust hard cap of 400, so the test stays independent.
oversized_lines=401
seq 1 $oversized_lines | sed 's/^/\/\/ line /' > "$scratch/src/oversized.rs"
( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
check "oversized file fails" 1 $?

# The duplicate-ceiling gate reads a hardcoded list, so a scratch-root copy of the tool names one file twice and runs there.
case "$scratch" in /*/.superpowers/g10l/budget.?*) [ -n "$scratch" ] && [ -f "$scratch/.flea-test-sandbox" ] && rm -f -- "$scratch/src/oversized.rs" ;; esac
sed 's|^known_over_cap="shelf/Model.js 701$|&\nshelf/Model.js 701|' "$repo/tools/flea-file-budget" > "$scratch/dup-budget"
if cmp -s "$repo/tools/flea-file-budget" "$scratch/dup-budget"; then
  echo "FAIL duplicate fixture: the sed did not duplicate a known_over_cap entry"
  fail=1
fi
chmod +x "$scratch/dup-budget"
dup_out=$(cd "$scratch" && "$scratch/dup-budget" 2>&1)
check "a duplicated ceiling fails" 1 $?
case $dup_out in
  *"DUPLICATE ceiling: shelf/Model.js"*) echo "ok   duplicate is named" ;;
  *) echo "FAIL duplicate is named: got '$dup_out'"; fail=1 ;;
esac

# The crates scanner reads the index, so the scratch list.rs is staged and counted at the tool's recorded 538 lines.
mkdir -p "$scratch/crates/flea-gui/src"
git -C "$scratch" init -q || exit 1
seq 1 538 | sed 's/^/\/\/ line /' > "$scratch/crates/flea-gui/src/list.rs"
git -C "$scratch" add crates || exit 1
( cd "$scratch" && "$repo/tools/flea-file-budget" > crates-control.log 2>&1 )
check "a tracked crates file at its recorded 538 lines passes" 0 $?
printf '// growth\n' >> "$scratch/crates/flea-gui/src/list.rs"
( cd "$scratch" && "$repo/tools/flea-file-budget" > crates-mutant.log 2>&1 )
check "a tracked crates file past its record fails" 1 $?
if grep -qF 'GREW      crates/flea-gui/src/list.rs: 539 lines, recorded 538' "$scratch/crates-mutant.log"; then
  echo "ok   crates growth names its measured record"
else
  echo "FAIL crates growth did not name its measured record"
  fail=1
fi
seq 1 400 | sed 's/^/\/\/ line /' > "$scratch/crates/flea-gui/src/new.rs"
printf '// line\n' > "$scratch/crates/flea-gui/src/list.rs"
git -C "$scratch" add crates || exit 1
( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
check "a new tracked crates file at the unchanged hard cap passes" 0 $?
printf '// growth\n' >> "$scratch/crates/flea-gui/src/new.rs"
( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
check "a new tracked crates file past the hard cap fails" 1 $?

# Git's quoted name output must not hide a tracked Rust file with whitespace in its name.
seq 1 400 | sed 's/^/\/\/ line /' > "$scratch/crates/flea-gui/src/new.rs"
odd_name="$scratch/crates/flea-gui/src/with space"$'\n'"and-line.rs"
seq 1 400 | sed 's/^/\/\/ line /' > "$odd_name"
git -C "$scratch" add crates || exit 1
( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
check "a tracked crates filename with spaces and a newline passes at its cap" 0 $?
printf '// growth\n' >> "$odd_name"
( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
check "whitespace cannot hide a tracked crates file past the hard cap" 1 $?

# A tracked crates file gone from the worktree fails by its path, never as a silent missing count.
gone="$scratch/crates/flea-gui/src/gone.rs"
seq 1 3 | sed 's/^/\/\/ line /' > "$gone"
git -C "$scratch" add crates || exit 1
case "$gone" in
  "$scratch"/crates/flea-gui/src/gone.rs) [ -f "$scratch/.flea-test-sandbox" ] && rm -f -- "$gone" ;;
esac
( cd "$scratch" && "$repo/tools/flea-file-budget" > deleted.log 2>&1 )
check "a tracked crates file deleted from the worktree fails" 1 $?
case $(cat "$scratch/deleted.log") in
  *"UNREADABLE crates/flea-gui/src/gone.rs"*) echo "ok   deleted file is named" ;;
  *) echo "FAIL deleted file is named: got '$(cat "$scratch/deleted.log")'"; fail=1 ;;
esac

exit $fail
