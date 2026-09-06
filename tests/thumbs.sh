#!/usr/bin/env bash
# Drives the real backend against the media fixture and asserts the thumbnail contract end to end.
set -u
set -o pipefail
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/release/flea
FIXTURE="${FLEA_MEDIA_DIR:-$FIXTURE_ROOT/flea-media-btrfs}"
# A scratch copy, so nothing this suite generates lands against a file the operator's cache knows.
D=$FIXTURE_ROOT/flea-thumbs-test-$$
# The cache this suite fills is its own, redirected inside that sandbox: src/backend/thumbcache.rs
# honours XDG_CACHE_HOME, so nothing here reads or writes the operator's real cache at all.
export XDG_CACHE_HOME="$D/cache"
CACHE="$D/cache/thumbnails"
# The operator's own, read twice and never written, only to prove this run left it alone.
REAL_CACHE="$HOME/.cache/thumbnails"
sandbox_cache_require "$REAL_CACHE/large"
real_before=$(ls -A "$REAL_CACHE/large" 2>/dev/null | wc -l)
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

[ -d "$FIXTURE" ] || { echo "thumbs.sh: the media fixture is missing at $FIXTURE"; exit 1; }
[ -x "$BIN" ] || { echo "thumbs.sh: $BIN is missing, run cargo build --release"; exit 1; }

sandbox_make "$D"
mkdir -p "$D/files" "$D/gen" "$D/other"
cp "$FIXTURE/photo_0.jpg" "$D/files/photo.jpg"
cp "$FIXTURE/clip_6.mp4" "$D/files/clip.mp4"
cp "$FIXTURE/notes_9.txt" "$D/files/notes.txt"
# A name no earlier run cached, so the superseded-listing case below is always a real miss.
cp "$FIXTURE/photo_0.jpg" "$D/gen/gen.jpg"

# Directories sort first, so the three media files get a directory of their own and the row order is clip.mp4, notes.txt, photo.jpg.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0,1,2]}\n{"c":"quit"}\n' "$D/files" | timeout 120 $BIN --backend)
check "a video generated a thumbnail" "1" "$(echo "$out" | grep -c '"row":0,"file":"/')"
# ffmpegthumbnailer writes a Thumb::URI and a Thumb::MTime of its own, so the entry we publish must not end up with two of either.
video_png=$(echo "$out" | grep -o '"row":0,"file":"[^"]*"' | cut -d'"' -f6)
check "the published entry carries one Thumb::URI" "1" "$(grep -ao 'Thumb::URI' "$video_png" | wc -l | tr -d ' ')"
check "and one Thumb::MTime" "1" "$(grep -ao 'Thumb::MTime' "$video_png" | wc -l | tr -d ' ')"
check "a text row answered with no file" "1" "$(echo "$out" | grep -c '"row":1,"file":""')"
check "an image generated a thumbnail" "1" "$(echo "$out" | grep -c '"row":2,"file":"/')"
check "every requested row was answered" "3" "$(echo "$out" | grep -c '"t":"thumbed"')"

# A cache hit is one read and one parse, tens of microseconds, against the 75 ms and up that a real decode costs here.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0,2]}\n{"c":"quit"}\n' "$D/files" | timeout 120 $BIN --backend)
check "a cached row is answered without generating it" "2" "$(echo "$out" | grep -cE '"file":"/[^"]*","ms":[0-4]\.[0-9]+}')"

# The only case that decodes a real 6000 by 3375 image, 20.25 megapixels and 81 MB of RGBA, through the product path, so it is what says the address-space cap is large enough for one: it reddens at --as=536870912 and it cannot go red on issue #17 itself, because this box passes the same pixels at the old 1 GiB cap; see AGENTS.md "Thumbnail sandbox".
if command -v magick >/dev/null; then
  mkdir -p "$D/icc"
  base64 -d tests/fixtures/srgb-iec61966-2.1.icc.b64 > "$D/srgb.icc"
  magick -size 6000x3375 'gradient:#2b5876-#d6a45f' -profile "$D/srgb.icc" "$D/icc/large.jpg"
  check "the 6000 by 3375 JPEG carries @Yiin's 3144-byte ICC profile from PR #39" "3144" "$(magick "$D/icc/large.jpg" "$D/extracted.icc" 2>/dev/null && wc -c < "$D/extracted.icc" | tr -d ' ')"
  icc_key=$(printf 'file://%s' "$D/icc/large.jpg" | md5sum | cut -d' ' -f1)
  out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"quit"}\n' "$D/icc" | timeout 120 $BIN --backend)
  check "a 20-megapixel decode fits inside the address-space cap" "1" "$(echo "$out" | grep -c '"row":0,"file":"/')"
  # The positive control on this key and this cache root; the corrupt-file case below pins the fail/flea path the negative check names.
  check "and the same key names the thumbnail it published" "1" "$([ -e "$CACHE/large/$icc_key.png" ] && echo 1 || echo 0)"
  check "and records no failure marker against it" "0" "$([ -e "$CACHE/fail/flea/$icc_key.png" ] && echo 1 || echo 0)"
else
  echo "skip the 20-megapixel decode: magick is absent and imagemagick is an optdepends"
fi

# A list replaces the row mapping, so a result for the old listing is dropped rather than reported against the new one.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D/gen" "$D/other" | timeout 120 $BIN --backend)
check "a result for a superseded listing is dropped" "0" "$(echo "$out" | grep -c '"t":"thumbed"')"

# The flood is larger than thumbs.rs's MAX_QUEUE of 70, so without the dedupe it evicts the victim row's queued job; see AGENTS.md "Thumbnail requests".
mkdir -p "$D/flood"
for i in 0 1 2 3 4 5; do cp "$FIXTURE/clip_6.mp4" "$D/flood/v$i.mp4"; done
flood='0,1,2,3,5'
for i in $(seq 1 75); do flood="$flood,4"; done
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[%s]}\n{"c":"quit"}\n' "$D/flood" "$flood" | timeout 300 $BIN --backend)
check "a repeated row does not evict another row's job" "1" "$(echo "$out" | grep -c '"row":5,"file":"/')"
check "and the repeated row itself is still answered once" "1" "$(echo "$out" | grep -c '"row":4,"file":"/')"

# glycin exits 0 having written nothing when it cannot decode the bytes, so that verdict has to be recorded or the file is retried forever.
mkdir -p "$D/corrupt"
head -c 4096 /dev/urandom > "$D/corrupt/broken.jpg"
broken_key=$(printf 'file://%s' "$D/corrupt/broken.jpg" | md5sum | cut -d' ' -f1)
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"quit"}\n' "$D/corrupt" | timeout 120 $BIN --backend)
check "an undecodable file answers empty" "1" "$(echo "$out" | grep -c '"row":0,"file":""')"
check "and the decoder verdict is recorded" "0" "$([ -e "$CACHE/fail/flea/$broken_key.png" ] && echo 0 || echo 1)"
# record_failure publishes by rename, so a second job would leave a different inode here; an unchanged one proves no child ran.
marker_inode=$(stat -c %i "$CACHE/fail/flea/$broken_key.png" 2>/dev/null || echo missing-before)
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"quit"}\n' "$D/corrupt" | timeout 120 $BIN --backend)
check "the second request is answered from the marker" "1" "$(echo "$out" | grep -c '"row":0,"file":"","ms":0\.')"
check "and no second child ran" "$marker_inode" "$(stat -c %i "$CACHE/fail/flea/$broken_key.png" 2>/dev/null || echo missing-after)"

# A fifo named like a video is not a regular file: the 10 s bound is well under the 20 s job timeout it used to burn.
mkdir -p "$D/special"
mkfifo "$D/special/pipe.mp4"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"quit"}\n' "$D/special" | timeout 10 $BIN --backend)
check "a fifo is answered at once, not after the job timeout" "1" "$(echo "$out" | grep -c '"row":0,"file":""')"
check "and no row offers a fifo as thumbnailable" "false" "$(echo "$out" | sed -n 2p | grep -oE '"t":(true|false)' | sed -n 1p | cut -d: -f2)"

# %i and %u must BOTH name the bound path: ffmpegthumbnailer takes %i, and every still image and PDF takes %u.
# Rows of files/ are clip.mp4, notes.txt, photo.jpg, so 0 is the %i user and 2 is the %u user.
ln -s "$D/files" "$D/vialink"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0,2]}\n{"c":"quit"}\n' "$D/vialink" | timeout 120 $BIN --backend)
check "a video under a symlinked directory thumbnails" "1" "$(echo "$out" | grep -c '"row":0,"file":"/')"
check "an image under a symlinked directory thumbnails" "1" "$(echo "$out" | grep -c '"row":2,"file":"/')"

# The same two spellings again, this time reached through a symlink to the file rather than to its parent.
mkdir -p "$D/vialinkfile"
ln -s "$D/files/clip.mp4" "$D/vialinkfile/linkclip.mp4"
ln -s "$D/files/photo.jpg" "$D/vialinkfile/linkpic.jpg"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0,1]}\n{"c":"quit"}\n' "$D/vialinkfile" | timeout 120 $BIN --backend)
check "a symlink to a video thumbnails" "1" "$(echo "$out" | grep -c '"row":0,"file":"/')"
check "a symlink to an image thumbnails" "1" "$(echo "$out" | grep -c '"row":1,"file":"/')"

# The sandbox is mandatory: with bwrap off PATH the job is refused, never run unconfined; see AGENTS.md "Thumbnail sandbox".
mkdir -p "$D/nopath" "$D/unconfined"
cp "$FIXTURE/photo_0.jpg" "$D/unconfined/u.jpg"
key=$(printf 'file://%s' "$D/unconfined/u.jpg" | md5sum | cut -d' ' -f1)
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"quit"}\n' "$D/unconfined" | timeout 120 env PATH="$D/nopath" $BIN --backend 2>/dev/null)
check "a missing sandbox refuses the job" "1" "$(echo "$out" | grep -c '"row":0,"file":""')"
check "and publishes nothing to the shared cache" "0" "$([ -e "$CACHE/large/$key.png" ] && echo 1 || echo 0)"
check "and records no failure marker" "0" "$([ -e "$CACHE/fail/flea/$key.png" ] && echo 1 || echo 0)"

# A cancel-all must leave the map holding exactly what is still running, or every cancelled row is skipped for good.
mkdir -p "$D/cancelall"
for i in $(seq 0 11); do ln "$FIXTURE/photo_0.jpg" "$D/cancelall/c$i.jpg"; done
rows=$(seq -s, 0 11)
out=$(printf '{"c":"list","path":"%s","first":20}\n{"c":"thumb","rows":[%s]}\n{"c":"thumbcancel","rows":[]}\n{"c":"thumb","rows":[%s]}\n{"c":"quit"}\n' "$D/cancelall" "$rows" "$rows" | timeout 300 $BIN --backend)
check "a row cancelled by a cancel-all can be asked for again" "12" "$(echo "$out" | grep -o '"row":[0-9]*,"file":"/' | sort -u | wc -l | tr -d ' ')"

# The map is bounded by the pool, not trimmed from the front, so a viewport larger than the queue still answers every row.
mkdir -p "$D/many"
for i in $(seq 0 79); do ln "$FIXTURE/photo_0.jpg" "$D/many/m$i.jpg"; done
rows=$(seq -s, 0 79)
out=$(printf '{"c":"list","path":"%s","first":100}\n{"c":"thumb","rows":[%s]}\n{"c":"quit"}\n' "$D/many" "$rows" | timeout 300 $BIN --backend)
check "every row of a request larger than the queue is answered" "80" "$(echo "$out" | grep -o '"row":[0-9]*' | sort -u | wc -l | tr -d ' ')"

# The trace is a diagnostic on stderr, so a client reading stdout must never see it; see AGENTS.md "Thumbnail trace".
# A cache hit is answered without a job and a job is what the trace reports on, so each run below asks for a file no run has cached.
mkdir -p "$D/probe"
for i in 0 1 2; do cp "$FIXTURE/photo_0.jpg" "$D/probe/p$i.jpg"; done
ask() { printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[%s]}\n{"c":"quit"}\n' "$D/probe" "$1"; }
out=$(ask 0 | FLEA_THUMB_TRACE=1 timeout 120 $BIN --backend 2>/dev/null)
check "the trace never reaches stdout" "0" "$(echo "$out" | grep -c 'trace')"
err=$(ask 1 | FLEA_THUMB_TRACE=1 timeout 120 $BIN --backend 2>&1 >/dev/null)
check "the trace reaches stderr when asked for" "1" "$(echo "$err" | grep -c 'trace row=1 ')"
off=$(ask 2 | timeout 120 $BIN --backend 2>&1 >/dev/null)
check "and nothing at all when it is not" "0" "$(echo "$off" | grep -c 'trace')"

# -A, never ls: the one kind of litter this subsystem leaves is a dotfile temp a bare ls cannot see.
check "no temp file is left in the cache this run filled" "0" "$(ls -A "$CACHE/large" 2>/dev/null | grep -c '^\.flea-')"
sandbox_remove "$D"

# The redirect is what makes this structural rather than a promise, so it is asserted.
check "the operator's own cache is untouched" "$real_before" "$(ls -A "$REAL_CACHE/large" 2>/dev/null | wc -l)"

[ "$fail" -eq 0 ] && echo "thumbs: all checks passed"
exit $fail
