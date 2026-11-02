#!/usr/bin/env bash
# tools/flea-aur-versions is checked here from saved RPC replies, so no network is needed.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
repo=$PWD
tool=$PWD/tools/flea-aur-versions
checks=0
failed=0
check() {
    checks=$((checks + 1))
    if [ "$2" = "$3" ]; then printf 'ok   %s\n' "$1"; else printf 'FAIL %s: expected [%s] got [%s]\n' "$1" "$2" "$3"; failed=$((failed + 1)); fi
}

# The wanted pkgrel comes from a real commit, so the early cases run inside a pkgrel 1 checkout.
pbox=$(mktemp -d "${TMPDIR:-/tmp}/flea-aurpass.XXXXXX") || exit 1
trap 'rm -rf -- "$pbox"' EXIT
mkdir -p "$pbox/packaging/flea" "$pbox/packaging/flea-bin" || exit 1
printf 'pkgname=flea\npkgver=0.3.7\npkgrel=1\n' > "$pbox/packaging/flea/PKGBUILD"
printf 'pkgname=flea-bin\npkgver=0.3.7\npkgrel=1\n' > "$pbox/packaging/flea-bin/PKGBUILD"
(cd "$pbox" && git init -q -b main . && git config user.name GM \
  && git config user.email gianmarcomorales@icloud.com && git add . && git commit -qm seed) || exit 1
psha=$(git -C "$pbox" rev-parse HEAD) || exit 1
pshort=${psha:0:7}
printf '{"version":5,"type":"multiinfo","resultcount":3,"results":[{"Name":"flea","Version":"0.3.7-1"},{"Name":"flea-bin","Version":"0.3.7-1"},{"Name":"flea-git","Version":"0.3.7.r0.g%s-1"}]}' "$pshort" > "$pbox/pass.json"

out=$(cd "$pbox" && "$tool" --json "$pbox/pass.json" 0.3.7 "$psha" 2>&1)
status=$?
check "the pass fixture exits 0" "0" "$status"
check "the pass fixture prints one line per package" "3" "$(printf '%s\n' "$out" | grep -c ': ok ')"

for pkg in flea flea-bin flea-git; do
    # Each case mismatches only on the package it names, so flea-git matches the fresh commit.
    sed "s/gabc1234/g$pshort/" "$repo/tests/aur-versions-mismatch-${pkg//-/}.json" > "$pbox/mismatch-$pkg.json" || exit 1
    out=$(cd "$pbox" && "$tool" --json "$pbox/mismatch-$pkg.json" 0.3.7 "$psha" 2>&1)
    status=$?
    check "the $pkg mismatch exits nonzero" "1" "$status"
    check "only $pkg mismatches" "$pkg" "$(printf '%s\n' "$out" | grep ': MISMATCH ' | cut -d: -f1)"
done
out=$(cd "$pbox" && "$tool" --json "$pbox/mismatch-flea-bin.json" 0.3.7 "$psha" 2>&1)
check "the missing package is named" "flea-bin: MISMATCH missing from the AUR reply, want 0.3.7-1" "$(printf '%s\n' "$out" | grep '^flea-bin:')"
out=$(cd "$pbox" && "$tool" --json "$repo/tests/aur-versions-mismatch-fleagit.json" 0.3.7 "$psha" 2>&1)
check "the stale flea-git line names both versions" "flea-git: MISMATCH got 0.3.6.r0.g98404bc-1, want 0.3.7.r0.g$pshort-1" "$(printf '%s\n' "$out" | grep '^flea-git:')"

# A rebuild keeps its pkgrel above 1, so the wanted versions come from the tag's own PKGBUILDs.
rbox=$(mktemp -d "${TMPDIR:-/tmp}/flea-aurrebuild.XXXXXX") || exit 1
trap 'rm -rf -- "$pbox" "$rbox"' EXIT
mkdir -p "$rbox/packaging/flea" "$rbox/packaging/flea-bin" || exit 1
printf 'pkgname=flea\npkgver=0.3.7\npkgrel=2\n' > "$rbox/packaging/flea/PKGBUILD"
printf 'pkgname=flea-bin\npkgver=0.3.7\npkgrel=2\n' > "$rbox/packaging/flea-bin/PKGBUILD"
(cd "$rbox" && git init -q -b main . && git config user.name GM \
  && git config user.email gianmarcomorales@icloud.com && git add . && git commit -qm rebuild) || exit 1
rsha=$(git -C "$rbox" rev-parse HEAD) || exit 1
rshort=${rsha:0:7}
printf '{"version":5,"type":"multiinfo","resultcount":3,"results":[{"Name":"flea","Version":"0.3.7-2"},{"Name":"flea-bin","Version":"0.3.7-2"},{"Name":"flea-git","Version":"0.3.7.r0.g%s-1"}]}' "$rshort" > "$rbox/rebuild.json"
out=$(cd "$rbox" && "$tool" --json "$rbox/rebuild.json" 0.3.7 "$rsha" 2>&1)
status=$?
check "a pkgrel 2 rebuild expects its own pkgrel" "0" "$status"
check "the rebuild prints one line per package" "3" "$(printf '%s\n' "$out" | grep -c ': ok ')"

tools/flea-aur-versions >/dev/null 2>&1; check "no args exits 2" "2" "$?"
out=$(cd "$pbox" && "$tool" --json "$pbox/pass.json" 0.3 "$psha" 2>&1)
status=$?
check "a short version exits 2 with its diagnostic" "2:flea-aur-versions: '0.3' is not X.Y.Z" "$status:$out"
tools/flea-aur-versions --json tests/aur-versions-pass.json 0.3.7 abc >/dev/null 2>&1; check "a short commit exits 2" "2" "$?"
tools/flea-aur-versions --json tests/aur-versions-pass.json 0.3.7 abc1234zz >/dev/null 2>&1; check "a non-hex commit tail exits 2" "2" "$?"
out=$(cd "$pbox" && "$tool" --json "$repo/tests/does-not-exist.json" 0.3.7 "$psha" 2>&1)
check "a missing reply exits 2" "2" "$?"

# An unreadable commit fails closed instead of falling back to pkgrel 1.
badrel=deadbeef0000000
out=$(cd "$pbox" && "$tool" --json "$pbox/pass.json" 0.3.7 "$badrel" 2>&1)
status=$?
check "an unreadable commit exits 2" "2" "$status"
check "the unreadable commit names itself and its PKGBUILD" "flea-aur-versions: cannot read pkgrel from packaging/flea/PKGBUILD at $badrel" "$(printf '%s\n' "$out" | grep '^flea-aur-versions:')"

printf 'aur-versions: %d check(s), %d failed\n' "$checks" "$failed"
[ "$failed" -eq 0 ]
