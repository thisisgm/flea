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
export XDG_DATA_HOME="$scratch/data" XDG_CONFIG_HOME="$scratch/config" XDG_CACHE_HOME="$scratch/cache"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" || exit 1
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
sed 's|^shelf/Model.js 701$|&\nshelf/Model.js 701|' "$repo/tools/flea-file-budget" > "$scratch/dup-budget"
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

# Exercise measured source bytes at each ratcheted debt, without reading the registry into the expected result.
git -C "$scratch" init -q || exit 1
for file in src/backend/thumbworker.rs ui/PreviewMarkdown.qml; do
  measured=$(wc -l < "$repo/$file") || exit 1
  if [ "$measured" -le 400 ]; then
    echo "FAIL measured-debt fixture: $file no longer exceeds the 400-line hard cap"
    exit 1
  fi
  mkdir -p "$scratch/$(dirname "$file")" || exit 1
  cp "$repo/$file" "$scratch/$file" || exit 1
  git -C "$scratch" add -- "$file" || exit 1
  ( cd "$scratch" && "$repo/tools/flea-file-budget" > measured-control.log 2>&1 )
  check "$file at its measured $measured lines passes" 0 $?
  printf '// growth\n' >> "$scratch/$file"
  ( cd "$scratch" && "$repo/tools/flea-file-budget" > measured-mutant.log 2>&1 )
  check "$file one line past its measured record fails" 1 $?
  if grep -qF "GREW      $file: $((measured + 1)) lines, recorded $measured, cap 400" "$scratch/measured-mutant.log"; then
    echo "ok   $file growth names its measured record"
  else
    echo "FAIL $file growth did not name its measured record"
    fail=1
  fi
  cp "$repo/$file" "$scratch/$file" || exit 1
done

# Former exemptions below the hard cap receive the ordinary 400-line boundary.
for file in crates/flea-gui/src/state_tests.rs crates/flea-gui/src/list/header.rs; do
  mkdir -p "$scratch/$(dirname "$file")" || exit 1
  cp "$repo/$file" "$scratch/$file" || exit 1
  git -C "$scratch" add -- "$file" || exit 1
  ( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
  check "$file at its measured size passes" 0 $?
  seq 1 400 | sed 's/^/\/\/ line /' > "$scratch/$file"
  ( cd "$scratch" && "$repo/tools/flea-file-budget" >/dev/null 2>&1 )
  check "$file at the unchanged hard cap passes" 0 $?
  printf '// growth\n' >> "$scratch/$file"
  ( cd "$scratch" && "$repo/tools/flea-file-budget" > former-debt-mutant.log 2>&1 )
  check "$file past the hard cap fails" 1 $?
  if grep -qFx "OVER CAP  $file: 401 lines, cap 400" "$scratch/former-debt-mutant.log"; then
    echo "ok   $file growth receives the ordinary hard cap"
  else
    echo "FAIL $file growth did not name the ordinary hard cap"
    fail=1
  fi
  cp "$repo/$file" "$scratch/$file" || exit 1
done

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
