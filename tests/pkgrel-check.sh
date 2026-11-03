#!/usr/bin/env bash
# Drives tools/flea-pkgrel-check against throwaway git repos; needs no qs.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

command -v git >/dev/null 2>&1 || { echo "pkgrel-check.sh: git is not on PATH" >&2; exit 1; }

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-pkgrel-check.XXXXXX") || exit 1
case $test_root in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$test_root', refusing to own it"; exit 1 ;;
esac
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

tool=$PWD/tools/flea-pkgrel-check
[ -x "$tool" ] || { echo "FAIL pkgrel-check tool is missing or not executable"; exit 1; }
fail=0
n=0

# A repo with both PKGBUILDs, the root one and flea-bin's, at one version, committed and tagged.
new_repo() {
  local dir="$1" ver="$2" rel="$3"
  mkdir -p "$dir/packaging/flea-bin" || return 1
  printf 'pkgname=flea\npkgver=%s\npkgrel=%s\n' "$ver" "$rel" > "$dir/PKGBUILD"
  printf 'pkgname=flea-bin\npkgver=%s\npkgrel=%s\n' "$ver" "$rel" > "$dir/packaging/flea-bin/PKGBUILD"
  (cd "$dir" && git init -q -b main . && git config user.name GM \
    && git config user.email gianmarcomorales@icloud.com && git add . && git commit -qm seed) || return 1
}

# Same, then one version bump on top, untagged.
bump_repo() {
  local dir="$1" oldver="$2" oldrel="$3" newver="$4" newrel="$5"
  new_repo "$dir" "$oldver" "$oldrel" || return 1
  (cd "$dir" && git tag "v$oldver" \
    && sed -i "s/^pkgver=.*/pkgver=$newver/; s/^pkgrel=.*/pkgrel=$newrel/" PKGBUILD packaging/flea-bin/PKGBUILD \
    && git add . && git commit -qm bump) || return 1
}

# Same, but the two PKGBUILDs land on different pkgrels.
bump_repo_split() {
  local dir="$1" oldver="$2" newver="$3" flearel="$4" binrel="$5"
  new_repo "$dir" "$oldver" 1 || return 1
  (cd "$dir" && git tag "v$oldver" \
    && sed -i "s/^pkgver=.*/pkgver=$newver/" PKGBUILD packaging/flea-bin/PKGBUILD \
    && sed -i "s/^pkgrel=.*/pkgrel=$flearel/" PKGBUILD \
    && sed -i "s/^pkgrel=.*/pkgrel=$binrel/" packaging/flea-bin/PKGBUILD \
    && git add . && git commit -qm bump) || return 1
}

run_tool() {
  (cd "$1" && "$tool" ${2:-HEAD} >"$test_root/out.txt" 2>&1)
}

check_rc() {
  local label="$1" want="$2" got="$3"
  n=$((n + 1))
  if [ "$got" = "$want" ]; then
    echo "ok   $label"
  else
    echo "FAIL $label: want exit $want, got $got: $(cat "$test_root/out.txt")"
    fail=1
  fi
}

check_names() {
  local label="$1"
  shift
  n=$((n + 1))
  local miss=""
  for want in "$@"; do
    grep -qF "$want" "$test_root/out.txt" || miss="$miss [$want]"
  done
  if [ -z "$miss" ]; then
    echo "ok   $label"
  else
    echo "FAIL $label: output misses$miss: $(cat "$test_root/out.txt")"
    fail=1
  fi
}

# 1. A pkgver bump that keeps pkgrel 2 fails.
bump_repo "$test_root/bump2" 0.3.7 1 0.3.8 2
run_tool "$test_root/bump2"; rc=$?
check_rc "a pkgver bump with pkgrel 2 fails" 1 "$rc"
check_names "the failure names the file, its pkgrel and both versions" \
  "FAIL PKGBUILD has pkgrel 2" "0.3.7 to 0.3.8"

# 2. The same bump reset to pkgrel 1 passes.
bump_repo "$test_root/bump1" 0.3.7 1 0.3.8 1
run_tool "$test_root/bump1"; rc=$?
check_rc "a pkgver bump with pkgrel 1 passes" 0 "$rc"
check_names "the pass says what it checked" "pkgver 0.3.7 to 0.3.8"

# 3. The same pkgver rebuilt at pkgrel 2 passes.
bump_repo "$test_root/same" 0.3.7 1 0.3.7 2
run_tool "$test_root/same"; rc=$?
check_rc "the same pkgver with pkgrel 2 passes" 0 "$rc"
check_names "the rebuild pass names the unchanged version" "pkgver 0.3.7 unchanged"

# 4. No previous tag is an error: no tag means tags were never fetched, not a free pass.
new_repo "$test_root/notag" 0.3.8 2
(cd "$test_root/notag" && git commit -q --allow-empty -m second)
run_tool "$test_root/notag"; rc=$?
check_rc "no previous tag errors" 2 "$rc"
check_names "the error says to fetch tags" "flea-pkgrel-check: ERROR" "fetch tags"

# 5. PKGBUILDs that disagree on pkgver fail.
new_repo "$test_root/split" 0.3.7 1
(cd "$test_root/split" && git tag v0.3.7 \
  && sed -i 's/^pkgver=.*/pkgver=0.3.8/' packaging/flea-bin/PKGBUILD \
  && git add . && git commit -qm split)
run_tool "$test_root/split"; rc=$?
check_rc "PKGBUILDs that disagree on pkgver fail" 1 "$rc"
check_names "the disagreement names both files and both versions" \
  "FAIL PKGBUILD says pkgver 0.3.7" "packaging/flea-bin/PKGBUILD says pkgver 0.3.8"

# 6. A pkgver bump with flea-bin at pkgrel 2 fails on flea-bin.
bump_repo_split "$test_root/bumpbin" 0.3.7 0.3.8 1 2
run_tool "$test_root/bumpbin"; rc=$?
check_rc "a pkgver bump with flea-bin at pkgrel 2 fails" 1 "$rc"
check_names "the failure names flea-bin, its pkgrel and both versions" \
  "packaging/flea-bin/PKGBUILD has pkgrel 2" "0.3.7 to 0.3.8"

# 7. Too many args errors.
(cd "$test_root/bump1" && "$tool" HEAD extra >"$test_root/out.txt" 2>&1)
rc=$?
check_rc "too many args errors" 2 "$rc"
check_names "the usage error names the tool" "flea-pkgrel-check: ERROR" "usage"

# 8. A commit the checkout does not hold errors.
run_tool "$test_root/bump1" nosuchref; rc=$?
check_rc "a bad commit errors" 2 "$rc"
check_names "the bad commit names itself" "flea-pkgrel-check: ERROR" "nosuchref"

# 9. A PKGBUILD without pkgrel errors.
new_repo "$test_root/norel" 0.3.8 1
(cd "$test_root/norel" && git tag v0.3.8 \
  && sed -i '/^pkgrel=/d' PKGBUILD \
  && git add . && git commit -qm drop)
run_tool "$test_root/norel"; rc=$?
check_rc "a PKGBUILD without pkgrel errors" 2 "$rc"
check_names "the unreadable field names the file" "flea-pkgrel-check: ERROR cannot read pkgrel from PKGBUILD at"

if [ "$fail" -eq 0 ]; then
  echo "pkgrel-check: all $n checks passed"
fi
exit "$fail"
