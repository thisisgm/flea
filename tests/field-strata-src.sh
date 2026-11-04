#!/bin/bash
# Gates tools/flea-field-bench's explicit or cargo-derived STRATA_SRC and exact binary-to-source version match.
set -u
cd "$(dirname "$0")/.." || exit 1
bench="$PWD/tools/flea-field-bench"

bad=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; bad=1
  else
    echo "ok   $label"
  fi
}

block=$(sed -n '/^# strata provenance begin$/,/^# strata provenance end$/p' "$bench")
[ -n "$block" ] || { echo "FAIL: no strata provenance block in $bench"; exit 1; }

# A mktemp -d root of the test's own, removed by its one checked path on the way out.
scratch=$(mktemp -d) || exit 1
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep"; exit 1 ;;
esac
: > "$scratch/.flea-test-sandbox"
trap 'case $scratch in /?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT

# A source tree with a Cargo.toml naming $2, and a strata stub that prints the line $3 for --version.
make_tree() {
  mkdir -p "$1"
  printf '[package]\nname = "strata"\nversion = "%s"\n' "$2" > "$1/Cargo.toml"
}
make_stub() {
  mkdir -p "$(dirname "$1")"
  printf '#!/bin/sh\necho "%s"\n' "$2" > "$1"
  chmod +x "$1"
}

# One case per subshell: fail() ends that case alone, and the bench's own block runs as the bench has it.
run_case() {
  (
    fail() { printf 'refused: %s\n' "$*"; exit 3; }
    eval "$block"
    STRATA_BIN=$1; STRATA_SRC=$2
    strata_provenance
    printf 'accepted: %s\n' "$STRATA_SRC"
  )
}

# The build tree names 1.0.0, and its binary reports 1.0.0, so STRATA_SRC comes from the target path.
make_tree "$scratch/a" 1.0.0
make_stub "$scratch/a/target/release/strata" "strata 1.0.0"
out=$(run_case "$scratch/a/target/release/strata" ""); rc=$?
check "a target/release binary derives its tree and is accepted" "accepted: $scratch/a" "$out"
check "the derived case exits zero" 0 "$rc"

# A binary built from v0.19.0 sources cannot pass for the v1.0.0 tree it is named against.
make_tree "$scratch/b" 1.0.0
make_stub "$scratch/b/target/release/strata" "strata 0.19.0"
out=$(run_case "$scratch/b/target/release/strata" ""); rc=$?
check "a binary reporting another version is refused" "refused: $scratch/b/target/release/strata reports 'strata 0.19.0', not version 1.0.0 of STRATA_SRC=$scratch/b, so the manifest would name the wrong source" "$out"
check "the refusal exits nonzero" 3 "$rc"

# A version that merely starts with the tree's digits is another version.
make_tree "$scratch/c" 1.0.1
make_stub "$scratch/c/target/release/strata" "strata 1.0.10"
out=$(run_case "$scratch/c/target/release/strata" ""); rc=$?
check "1.0.10 does not pass for a 1.0.1 tree" 3 "$rc"

# A prerelease suffix is another version, and a local suffix matches only itself.
make_tree "$scratch/h" 1.0.0
make_stub "$scratch/h/target/release/strata" "strata 1.0.0-beta.2"
out=$(run_case "$scratch/h/target/release/strata" ""); rc=$?
check "1.0.0-beta.2 does not pass for a 1.0.0 tree" 3 "$rc"
make_tree "$scratch/i" 1.0.0+git
make_stub "$scratch/i/target/release/strata" "strata 1.0.0+git"
out=$(run_case "$scratch/i/target/release/strata" ""); rc=$?
check "1.0.0+git passes for a 1.0.0+git tree" "accepted: $scratch/i" "$out"
check "the exact suffix case exits zero" 0 "$rc"

# A binary outside any cargo target/release names no tree, so the run requires STRATA_SRC.
make_stub "$scratch/d/bin/strata" "strata 1.0.0"
out=$(run_case "$scratch/d/bin/strata" ""); rc=$?
check "a binary outside target/release refuses to guess its tree" \
  "refused: STRATA_BIN=$scratch/d/bin/strata is not in a cargo target/release, so the tree it was built from is unknown: set STRATA_SRC" "$out"

# Named explicitly, the same binary is accepted once its tree reports the same version.
make_tree "$scratch/e" 1.0.0
out=$(run_case "$scratch/d/bin/strata" "$scratch/e"); rc=$?
check "an explicit STRATA_SRC with a matching version is accepted" "accepted: $scratch/e" "$out"

# A stub that reports nothing cannot be checked, so the run is refused rather than trusted.
make_stub "$scratch/f/target/release/strata" ""
make_tree "$scratch/f" 1.0.0
out=$(run_case "$scratch/f/target/release/strata" ""); rc=$?
check "a binary that reports no version is refused" 3 "$rc"

# A tree with no version in its Cargo.toml cannot be compared, so it is refused.
mkdir -p "$scratch/g"; printf '[package]\nname = "strata"\n' > "$scratch/g/Cargo.toml"
make_stub "$scratch/g/target/release/strata" "strata 1.0.0"
out=$(run_case "$scratch/g/target/release/strata" ""); rc=$?
check "a tree with no version is refused" 3 "$rc"

# A nightly checked in its own subshell still hands its derived tree to the manifest.
archive=$(sed -n '/^# archive provenance begin$/,/^# archive provenance end$/p' "$bench")
make_tree "$scratch/n" 2.0.0
make_stub "$scratch/n/target/release/strata" "strata 2.0.0"
out=$( fail() { printf 'refused: %s\n' "$*"; exit 3; }; only_has() { [ "$1" = strata-nightly ]; }
       eval "$block"; B5_RIVAL_MANIFEST= STRATA_BIN= STRATA_SRC= STRATA_NIGHTLY_SRC= STRATA_NIGHTLY_BIN=$scratch/n/target/release/strata
       eval "$archive"; printf '%s|%s|%s' "$STRATA_NIGHTLY_SRC" "$STRATA_SRC" "$(bash -c 'printf %s "$STRATA_NIGHTLY_SRC"')" )
check "a derived nightly tree is exported and leaves STRATA_SRC alone" "$scratch/n||$scratch/n" "$out"

# A sibling without the provenance marker must give way to the tree's current manifest tool.
export MANIFEST_SELECTION=$(sed -n '/^# manifest selection begin$/,/^# manifest selection end$/p' "$bench")
mkdir -p "$scratch/sibling"
select_manifest() { FLEA_REPO=$PWD bash -c 'eval "$MANIFEST_SELECTION"; printf "%s" "$MANIFEST_TOOL"' "$scratch/sibling/flea-field-bench"; }
printf '#!/bin/bash\nexit 0\n' > "$scratch/sibling/flea-bench-manifest"
chmod +x "$scratch/sibling/flea-bench-manifest"
check "an old sibling manifest tool falls back to the tree's tool" "$PWD/tools/flea-bench-manifest" "$(select_manifest)"
cp tools/flea-bench-manifest "$scratch/sibling/flea-bench-manifest"
check "a current sibling manifest tool is preferred" "$scratch/sibling/flea-bench-manifest" "$(select_manifest)"
chmod -x "$scratch/sibling/flea-bench-manifest"
check "a non-executable sibling manifest tool falls back" "$PWD/tools/flea-bench-manifest" "$(select_manifest)"

# The bench and standalone manifest must compare resolved trees while preserving an empty anchor.
repo_resolve=$(grep '^FLEA_REPO=' "$bench")
[ -n "$repo_resolve" ] || { echo "FAIL: no FLEA_REPO resolution in $bench"; exit 1; }
anchor_setup=$(sed -n '/^# anchor setup begin$/,/^# anchor setup end$/p' "$bench")
anchor_src=$(sed -n '/^anchor_line() {$/,/^}$/p' tools/flea-bench-manifest)
mkdir -p "$scratch/tree" "$scratch/other"
ln -s "$scratch/tree" "$scratch/tree-link"
bench_anchor_case() { ( FLEA_REPO=$1; ANCHOR_REPO=$2; eval "$repo_resolve"; eval "$anchor_setup"; printf '%s|%s|%s' "$WITH_ANCHOR" "$FLEA_REPO" "$ANCHOR_REPO"; ); }
manifest_anchor_case() { ( FLEA_REPO=$1; ANCHOR_REPO=$2; export GIT_CEILING_DIRECTORIES=$scratch; eval "$anchor_src"; anchor_line; ); }
check "a trailing slash on the same bench tree creates no anchor" "no|$scratch/tree|$scratch/tree" "$(bench_anchor_case "$scratch/tree" "$scratch/tree/")"
check "a symlink to the same bench tree creates no anchor" "no|$scratch/tree|$scratch/tree" "$(bench_anchor_case "$scratch/tree-link" "$scratch/tree")"
check "an empty bench anchor remains empty" "no|$scratch/tree|" "$(bench_anchor_case "$scratch/tree" '')"
check "a separate bench tree keeps its anchor" "yes|$scratch/tree|$scratch/other" "$(bench_anchor_case "$scratch/tree" "$scratch/other/")"
check "the manifest treats a trailing slash as the same tree" "no anchor arm ran in this run" "$(manifest_anchor_case "$scratch/tree" "$scratch/tree/")"
check "the manifest treats a symlink as the same tree" "no anchor arm ran in this run" "$(manifest_anchor_case "$scratch/tree" "$scratch/tree-link")"
check "the manifest preserves an empty anchor" "no anchor arm ran in this run" "$(manifest_anchor_case "$scratch/tree" '')"
check "the manifest records a separate resolved tree" "$scratch/other at unreadable untagged" "$(manifest_anchor_case "$scratch/tree" "$scratch/other/")"

[ "$bad" = 0 ] && echo "field-strata-src: all passed" || echo "field-strata-src: failures above"
exit "$bad"
