#!/bin/bash
# Usage: pk2-fixture.sh [name]; prints the fixed root.
set -euo pipefail
name="${1:-pk2}"
export FLEA_FIXTURE_ROOT="${FLEA_FIXTURE_ROOT:-/home/flea-sandbox/$name}"
. "$(dirname -- "$0")/../../../tools/flea-sandbox-guard"
sandbox_root_ok
root=$SANDBOX_ROOT
if [ -e "$root" ]; then
    [[ -d $root && -f $root/$SANDBOX_MARKER && ! -L $root/$SANDBOX_MARKER ]] \
        || sandbox_refuse "existing fixture root is not owned: $root"
else
    [ -d "$(dirname -- "$root")" ] || sandbox_refuse "fixture parent missing: $root"
    mkdir -- "$root"
    : > "$root/$SANDBOX_MARKER"
fi
# Refuse links and special files before any fixture content changes, including the bulk names.
unsafe=$(find "$root" -mindepth 1 \( -type l -o \( ! -type d ! -type f \) -o \( -type f -links +1 \) \) -print -quit)
[ -z "$unsafe" ] || sandbox_refuse "fixture contains a link or special file: $unsafe"
fixture_dir() {
    sandbox_require "$1"
    mkdir -p -- "$SANDBOX_PATH"
}
fixture_bytes() {
    sandbox_require "$1"
    head -c "$2" /dev/zero | tr '\0' "$3" > "$SANDBOX_PATH"
}
for dir in v/work data home recent many; do fixture_dir "$root/$dir"; done
for dir in A Code Docs Empty Locked Many Media; do fixture_dir "$root/v/work/$dir"; done
fixture_bytes "$root/v/work/notes.txt" 99 n
fixture_bytes "$root/v/work/todo.md" 48 t
touch -d "2025-06-15 12:00:00" "$root/v/work" "$root/v/work/"*
# Eight recents, stamps falling, SI byte counts.
declare -A recent_size=( [report.pdf]=21800 [notes.txt]=99 [photo.png]=2500000 [readme.md]=48 [main.rs]=156 [tone.ogg]=6900 [sunset.jpg]=6000 [todo.md]=48 )
i=0
bookmarks=""
for name in report.pdf notes.txt photo.png readme.md main.rs tone.ogg sunset.jpg todo.md; do
    fixture_bytes "$root/recent/$name" "${recent_size[$name]}" x
    stamp="2025-06-15T12:00:0$((7 - i))Z"
    bookmarks="$bookmarks<bookmark href=\"file://$root/recent/$name\" visited=\"$stamp\"/>"
    i=$((i + 1))
done
sandbox_require "$root/data/recently-used.xbel"
printf '<?xml version="1.0" encoding="UTF-8"?><xbel version="1.0">%s</xbel>' "$bookmarks" > "$SANDBOX_PATH"
touch -d "2025-06-15 12:00:00" "$root/recent/"*
if [ ! -f "$root/many/file_100000.txt" ]; then
    sandbox_require "$root/many"
    seq -f 'file_%06g.txt' 1 100000 | (cd "$SANDBOX_PATH" && xargs touch -d "2025-06-15 12:00:00")
fi
printf '%s\n' "$root"
