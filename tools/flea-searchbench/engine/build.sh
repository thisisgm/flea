#!/bin/bash
set -euo pipefail
export LC_ALL=C.UTF-8
unset LANGUAGE
H=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
UNIT=$(dirname -- "$H")
W=$(cd -- "$H/../../.." && pwd -P)

if [[ ${1:-} != --inside ]]; then
    SOURCES=${FINDBENCH_SOURCES:-$(dirname -- "$W")/findbench/src}
    [[ $SOURCES = /* && -d $SOURCES ]] || { echo 'absolute source root required' >&2; exit 2; }
    SOURCES=$(cd -- "$SOURCES" && pwd -P)
    [[ $(git -C "$W" rev-parse HEAD) = "${FINDBENCH_FLEA_REV:-$(git -C "$W" rev-parse HEAD)}" ]] || exit 2
    git -C "$W" diff --quiet
    git -C "$W" diff --cached --quiet
    for source in fff fsearch fsearch-nd; do
        case $source in
            fff) expected=d978416f77aec3f9928d56e00c9e56ba39ca1fda ;;
            fsearch) expected=cb6cd67f9ef12fedcf6a160b6a38faf492f7c989 ;;
            fsearch-nd) expected=88e63fdf850cf7749deae8198b187accd21f8ba5 ;;
        esac
        [[ $(git -C "$SOURCES/$source" rev-parse HEAD) = "$expected" ]] || exit 2
        [[ -z $(git -C "$SOURCES/$source" status --porcelain) ]] || exit 2
    done
    # Current rival builds come from a second clean root at the pins in rival-pins.tsv; /sources stays historical.
    CURRENT=${FINDBENCH_CURRENT_SOURCES:?absolute current rival source root required}
    [[ $CURRENT = /* && -d $CURRENT ]] || exit 2
    CURRENT=$(cd -- "$CURRENT" && pwd -P)
    while IFS=$'\t' read -r source expected; do
        [[ $source = flea-0.3.12 ]] && continue
        [[ $(git -C "$CURRENT/$source" rev-parse HEAD) = "$expected" ]] || exit 2
        [[ -z $(git -C "$CURRENT/$source" status --porcelain) ]] || exit 2
    done < "$H/rival-pins.tsv"
    anchor=$(awk -F'\t' '$1 == "flea-0.3.12" {print $2}' "$H/rival-pins.tsv")
    [[ $(git -C "$W" rev-parse --verify "$anchor^{commit}") = "$anchor" ]] || exit 2
    BUILD=$(mktemp -d "$UNIT/build.XXXXXXXX")
    printf '%s\n' findbench1 > "$BUILD/.findbench-root"
    printf '%s\n' "$SOURCES" > "$BUILD/source-root.txt"
    printf 'BUILD_ROOT=%s\n' "$BUILD"
    git -C "$W" ls-files -z > "$BUILD/source-files.nul"
    git -C "$W" rev-parse HEAD > "$BUILD/flea-commit.txt"
    git -C "$W" rev-parse HEAD^{tree} > "$BUILD/flea-tree.txt"
    printf '%s\n' "$CURRENT" > "$BUILD/current-source-root.txt"
    for source in fff fsearch fsearch-nd; do printf '%s\t%s\n' "$source" "$(git -C "$CURRENT/$source" rev-parse HEAD)"; done > "$BUILD/current-sources.tsv"
    mkdir "$BUILD/anchor-source"
    git -C "$W" archive "$anchor" | tar -x -C "$BUILD/anchor-source"
    printf '%s\n' "$anchor" > "$BUILD/anchor-commit.txt"
    sha256sum "$H/flea-metrics.patch" > "$BUILD/metrics-patch.sha256"
    python3 -B "$H/provenance.py" write "$H" "$BUILD"
    image_tag=localhost/flea-findbench1:arch
    podman build --tag "$image_tag" --file "$H/Containerfile" "$H" > "$BUILD/image-build.log" 2>&1
    podman image inspect --format '{{.Id}}' "$image_tag" > "$BUILD/image-id"
    image_id=$(cat "$BUILD/image-id")
    inside=/work/.superpowers/fb0/$(basename -- "$BUILD")
    podman run --rm --userns=keep-id --cap-drop=all --security-opt=no-new-privileges \
        --volume "$W:/work:ro" --volume "$UNIT:/work/.superpowers/fb0:rw" \
        --volume "$SOURCES:/sources:ro" --volume "$CURRENT:/current:ro" "$image_id" \
        /bin/bash /work/.superpowers/fb0/engine/build.sh --inside "$inside" \
        > "$BUILD/build.log" 2>&1
    printf 'BUILD_ROOT=%s\n' "$BUILD"
    exit 0
fi

BUILD=${2:?marked build root required}
[[ $BUILD = "$UNIT"/build.* && -f $BUILD/.findbench-root ]] || exit 2
mkdir -p "$BUILD/tmp" "$BUILD/cargo" "$BUILD/xdg/config" "$BUILD/xdg/cache" "$BUILD/xdg/data"
export TMPDIR="$BUILD/tmp" CARGO_HOME="$BUILD/cargo"
export XDG_CONFIG_HOME="$BUILD/xdg/config" XDG_CACHE_HOME="$BUILD/xdg/cache" XDG_DATA_HOME="$BUILD/xdg/data"
python3 -B "$H/provenance.py" check "$H" "$BUILD"
cp -a "$H" "$BUILD/project"
cp /current/fff/Cargo.lock "$BUILD/project/Cargo.lock"
sed -i 's|path = "/sources/fff/crates/fff-core"|path = "/current/fff/crates/fff-core"|' "$BUILD/project/Cargo.toml"
grep -q '"/current/fff/crates/fff-core"' "$BUILD/project/Cargo.toml"
sed -i 's|"/sources/fsearch-nd/|"/current/fsearch-nd/|' "$BUILD/project/rust/nd.rs"
[[ $(grep -c '"/current/fsearch-nd/' "$BUILD/project/rust/nd.rs") = 3 ]] || exit 2
cp -a "$H" "$BUILD/project-historical"
cp /sources/fff/Cargo.lock "$BUILD/project-historical/Cargo.lock"
# The workspace lock now carries the GUI crates; the zero-dependency engine takes the one-package lock of the anchor.
git_lock_packages=$(grep -c '^\[\[package\]\]' "$BUILD/anchor-source/Cargo.lock")
[[ $git_lock_packages = 1 ]] || exit 2
cp "$BUILD/anchor-source/Cargo.lock" "$BUILD/project/flea-engine/Cargo.lock"
# The metrics seam is applied only to a fresh marked scratch copy, never the worktree.
python3 - "$W" "$BUILD" <<'PY'
import os, pathlib, shutil, sys
source, build = map(pathlib.Path, sys.argv[1:])
if not build.is_absolute() or not (build / '.findbench-root').is_file():
    raise SystemExit('marked absolute build root required')
destination = build / 'flea-source'
destination.mkdir()
paths = (build / 'source-files.nul').read_bytes().split(b'\0')
for raw in paths:
    if not raw:
        continue
    rel = pathlib.Path(os.fsdecode(raw))
    original = source / rel
    target = destination / rel
    target.parent.mkdir(parents=True, exist_ok=True)
    if original.is_symlink():
        os.symlink(os.readlink(original), target)
    else:
        shutil.copyfile(original, target)
PY
patch --directory "$BUILD/flea-source" -p1 < "$H/flea-metrics.patch"
# The 0.3.12 anchor takes the reviewed seam it was written against, on its own tagged source.
[[ $(cat "$BUILD/anchor-commit.txt") = f21789e56cc4b0877caa3156fff7bce52a8dd9c2 ]] || exit 2
patch --directory "$BUILD/anchor-source" -p1 < "$H/flea-metrics.patch.v0.3.12"
mkdir "$BUILD/anchor-engine"
cp "$H/flea-engine/Cargo.toml" "$BUILD/anchor-engine/Cargo.toml"
cp "$BUILD/anchor-source/Cargo.lock" "$BUILD/anchor-engine/Cargo.lock"
python3 - "$BUILD/project/flea-engine/Cargo.toml" "$BUILD/flea-source/src/main.rs" "$W/Cargo.toml" <<'PY'
import pathlib, re, sys, tomllib
manifest = pathlib.Path(sys.argv[1])
text = manifest.read_text()
root = tomllib.loads(pathlib.Path(sys.argv[3]).read_text())
package = root['package']
for key in ['version', 'edition', 'rust-version']:
    # A key the package inherits ({workspace = true}) takes the workspace's value.
    value = root['workspace']['package'][key] if package[key] == {'workspace': True} else package[key]
    text = re.sub(r'^' + re.escape(key) + r' = "[^"]+"', key + ' = "' + value + '"', text, count=1, flags=re.MULTILINE)
text = text.replace('path = "../../../../src/main.rs"', 'path = "' + sys.argv[2] + '"')
library = pathlib.Path(sys.argv[2]).with_name('lib.rs')
if library.is_file():
    text += '\n[lib]\nname = "flea"\npath = "' + str(library) + '"\n'
manifest.write_text(text)
PY
candidate_version=$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["package"]["version"])' "$W/Cargo.toml")
sed -i 's/^version = "0\.3\.12"$/version = "'"$candidate_version"'"/' "$BUILD/project/flea-engine/Cargo.lock"
grep -qx 'version = "'"$candidate_version"'"' "$BUILD/project/flea-engine/Cargo.lock"
python3 - "$BUILD/anchor-engine/Cargo.toml" "$BUILD/anchor-source/src/main.rs" "$BUILD/anchor-source/Cargo.toml" <<'PY'
import pathlib, re, sys, tomllib
manifest = pathlib.Path(sys.argv[1])
text = manifest.read_text()
root = tomllib.loads(pathlib.Path(sys.argv[3]).read_text())
package = root['package']
for key in ['version', 'edition', 'rust-version']:
    # A key the package inherits ({workspace = true}) takes the workspace's value.
    value = root['workspace']['package'][key] if package[key] == {'workspace': True} else package[key]
    text = re.sub(r'^' + re.escape(key) + r' = "[^"]+"', key + ' = "' + value + '"', text, count=1, flags=re.MULTILINE)
text = text.replace('path = "../../../../src/main.rs"', 'path = "' + sys.argv[2] + '"')
library = pathlib.Path(sys.argv[2]).with_name('lib.rs')
if library.is_file():
    text += '\n[lib]\nname = "flea"\npath = "' + str(library) + '"\n'
manifest.write_text(text)
PY
rustc -Vv > "$BUILD/rustc.txt"
cargo -V > "$BUILD/cargo.txt"
pacman -Q > "$BUILD/packages.txt"
uname -a > "$BUILD/kernel.txt"
pkg-config --modversion gtk+-3.0 gio-unix-2.0 pango libpcre2-8 icu-uc > "$BUILD/c-libraries.txt"
# Seed the standalone lock from the pinned workspace; retain the final resolved lock and its diff.
cargo fetch --manifest-path "$BUILD/project/Cargo.toml"
diff -u /current/fff/Cargo.lock "$BUILD/project/Cargo.lock" > "$BUILD/lock-diff.txt" || [[ $? = 1 ]]
cargo fetch --manifest-path "$BUILD/project-historical/Cargo.toml"
diff -u /sources/fff/Cargo.lock "$BUILD/project-historical/Cargo.lock" > "$BUILD/lock-diff-historical.txt" || [[ $? = 1 ]]
cargo metadata --offline --locked --format-version 1 --manifest-path "$BUILD/project/Cargo.toml" > "$BUILD/cargo-metadata.json"
cargo tree --offline --locked -e normal --manifest-path "$BUILD/project/Cargo.toml" > "$BUILD/cargo-tree.txt"
/usr/bin/time -v -o "$BUILD/rust-build.time" \
    cargo build --release --offline --locked --manifest-path "$BUILD/project/Cargo.toml" --target-dir "$BUILD/target"
/usr/bin/time -v -o "$BUILD/flea-build.time" \
    cargo build --release --offline --locked --manifest-path "$BUILD/project/flea-engine/Cargo.toml" --target-dir "$BUILD/flea-target"
/usr/bin/time -v -o "$BUILD/rust-historical-build.time" \
    cargo build --release --offline --locked --bin fff-driver --manifest-path "$BUILD/project-historical/Cargo.toml" --target-dir "$BUILD/target-historical"
/usr/bin/time -v -o "$BUILD/flea-anchor-build.time" \
    cargo build --release --offline --locked --manifest-path "$BUILD/anchor-engine/Cargo.toml" --target-dir "$BUILD/flea-anchor-target"
mapfile -t names < "$H/fsearch-driver/sources.txt"
c_sources=()
for name in "${names[@]}"; do c_sources+=("/current/fsearch/src/$name"); done
read -r -a c_flags <<< "$(pkg-config --cflags gtk+-3.0 gio-unix-2.0 pango libpcre2-8 icu-uc)"
read -r -a c_libs <<< "$(pkg-config --libs gtk+-3.0 gio-unix-2.0 pango libpcre2-8 icu-uc)"
/usr/bin/time -v -o "$BUILD/c-build.time" \
    cc -std=gnu11 -O3 -DNDEBUG -D_GNU_SOURCE -D_FILE_OFFSET_BITS=64 -DPCRE2_CODE_UNIT_WIDTH=8 \
    -ffunction-sections -fdata-sections -I"$H/fsearch-driver" -I/current/fsearch/src \
    "${c_flags[@]}" "$H/fsearch-driver/main.c" "${c_sources[@]}" \
    -Wl,--gc-sections "${c_libs[@]}" -lm -o "$BUILD/target/release/fsearch-driver"
binaries=("$BUILD"/target/release/*-driver "$BUILD/target-historical/release/fff-driver" "$BUILD/flea-target/release/flea-engine" "$BUILD/flea-anchor-target/release/flea-engine")
stat --printf='%n\t%s\n' "${binaries[@]}" > "$BUILD/binary-bytes.tsv"
sha256sum "${binaries[@]}" > "$BUILD/binaries.sha256"
find "$BUILD/flea-source" -type f -print0 | sort -z | xargs -0 sha256sum > "$BUILD/source.sha256"
find "$BUILD/anchor-source" -type f -print0 | sort -z | xargs -0 sha256sum > "$BUILD/anchor-source.sha256"
printf '%s\n' complete > "$BUILD/BUILD-COMPLETE"

