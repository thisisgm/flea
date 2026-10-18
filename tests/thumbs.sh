#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
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
# A scratch runtime dir keeps this suite's backends off the operator's session journal.
export XDG_RUNTIME_DIR="$D/runtime"
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
# Before the sandbox exists: without qs the probe below cannot run, so refuse before making anything.
if ! command -v qs >/dev/null; then echo "thumbs.sh: qs is not installed"; exit 1; fi

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

# A generation publishes by rename, so a cached entry that keeps its inode and mtime across the request was not generated again.
cached_stamp() { stat -c '%i %y' "$CACHE/large/$(printf 'file://%s' "$D/files/$1" | md5sum | cut -d' ' -f1).png" 2>/dev/null || echo "missing-$1"; }
stamps_before="$(cached_stamp clip.mp4) $(cached_stamp photo.jpg)"
check "both cached entries are on disk before the request" "0" "$(printf '%s' "$stamps_before" | grep -c missing)"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0,2]}\n{"c":"quit"}\n' "$D/files" | timeout 120 $BIN --backend)
check "a cached row is answered with its file" "2" "$(echo "$out" | grep -c '"file":"/')"
check "and neither cached entry was generated again" "$stamps_before" "$(cached_stamp clip.mp4) $(cached_stamp photo.jpg)"

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

# Issue #39: a 256-megapixel PNG exercises the real decoder beyond the smaller ICC fixture.
python3 tests/thumbs-large.py "$BIN" "$D"
check "a valid 16000 by 16000 PNG thumbnails on three fresh caches" "0" "$?"

# A list replaces the row mapping, so a result for the old listing is dropped rather than reported against the new one.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D/gen" "$D/other" | timeout 120 $BIN --backend)
check "a result for a superseded listing is dropped" "0" "$(echo "$out" | grep -c '"t":"thumbed"')"

# The FIFO queues the victim first; without dedupe the repeated row answers 75 times, caught by the second check, while the first guards against eviction; see AGENTS.md "Thumbnail requests".
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
# The inode below, not a time, proves the marker answered: a lost marker decodes again and publishes a new one.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n{"c":"quit"}\n' "$D/corrupt" | timeout 120 $BIN --backend)
check "the second request is answered from the marker" "1" "$(echo "$out" | grep -c '"row":0,"file":""')"
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

# The pre-linked worker, see AGENTS.md "Thumbnail worker"; tests/thumbs-exec.sh runs this whole file again with it off.
echo "worker mode: ${FLEA_THUMB_WORKER:-on}"
# Sample input: /proc/123/status "PPid:	45", one tab after the colon.
parent_of() { grep '^PPid:' "/proc/$1/status" 2>/dev/null | cut -f2; }
descends_from() {
  local pid=$1 root=$2
  while [ -n "$pid" ] && [ "$pid" -gt 1 ]; do
    [ "$pid" = "$root" ] && return 0
    pid=$(parent_of "$pid")
  done
  return 1
}
# The worker's own comm is flea; bwrap carries --thumb-worker in its argv too and must not be counted.
workers_under() {
  local pid
  for pid in $(pgrep -f -- '--thumb-worker'); do
    [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = flea ] && descends_from "$pid" "$1" && echo "$pid"
  done
}
# Reads backend lines into ANSWER until row $1 is answered or ten seconds pass; never inside $(), which is not promised the coprocess's descriptors.
answer_for() {
  local line
  ANSWER=""
  while IFS= read -r -t 10 line <&"${BK[0]}"; do
    case $line in *'"t":"thumbed","row":'"$1"','*) ANSWER=$line; return 0 ;; esac
  done
  return 1
}
mkdir -p "$D/worker"
for i in 0 1 2; do cp "$FIXTURE/clip_6.mp4" "$D/worker/w$i.mp4"; done
cp "$FIXTURE/clip_6.mp4" "$D/worker/w3-unreadable.mp4"; chmod 000 "$D/worker/w3-unreadable.mp4"
coproc BK { exec timeout 120 $BIN --backend 2>"$D/worker.err"; }
printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0]}\n' "$D/worker" >&"${BK[1]}"
answer_for 0
check "a video is answered with a thumbnail" "1" "$(echo "$ANSWER" | grep -c '"file":"/')"
live=$(workers_under "$BK_PID" | wc -l | tr -d ' ')
if [ "${FLEA_THUMB_WORKER:-on}" = off ]; then
  check "the off switch starts no worker" "0" "$live"
else
  check "the video went through one worker" "1" "$live"
fi
printf '{"c":"thumb","rows":[3]}\n' >&"${BK[1]}"
answer_for 3
check "an unreadable video answers empty" "1" "$(echo "$ANSWER" | grep -c '"file":""')"
w3_key=$(printf 'file://%s' "$D/worker/w3-unreadable.mp4" | md5sum | cut -d' ' -f1)
# The exec path records a failure for a file it cannot read, which makes the off run this check's positive control.
if [ "${FLEA_THUMB_WORKER:-on}" = off ]; then w3_marked=1; else w3_marked=0; fi
check "it has a failure marker only on the exec path, because the worker branch judges nothing" "$w3_marked" "$(ls "$CACHE/fail/flea/$w3_key.png" 2>/dev/null | wc -l | tr -d ' ')"
if [ "${FLEA_THUMB_WORKER:-on}" != off ]; then
  check "and leaves the same worker serving" "1" "$(workers_under "$BK_PID" | wc -l | tr -d ' ')"
  kill -9 $(workers_under "$BK_PID") 2>/dev/null
  printf '{"c":"thumb","rows":[1]}\n' >&"${BK[1]}"
  answer_for 1
  check "a video after the worker died still gets a thumbnail" "1" "$(echo "$ANSWER" | grep -c '"file":"/')"
  check "and no worker is started again" "0" "$(workers_under "$BK_PID" | wc -l | tr -d ' ')"
fi
# Retiring a running worker says so once on stderr; a worker that was never started has nothing to retire.
if [ "${FLEA_THUMB_WORKER:-on}" = off ]; then retired=0; else retired=1; fi
check "the backend says the worker is gone exactly when it retired one" "$retired" "$(grep -c 'flea: the thumbnail worker .*, so videos use the thumbnailer program' "$D/worker.err")"
printf '{"c":"quit"}\n' >&"${BK[1]}"
wait "$BK_PID" 2>/dev/null
# The two paths publish the same image: set_size(N, N) and the film strip are the CLI's -s and -f.
w_key=$(printf 'file://%s' "$D/worker/w2.mp4" | md5sum | cut -d' ' -f1)
coproc BK { exec timeout 120 $BIN --backend 2>/dev/null; }
printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[2]}\n' "$D/worker" >&"${BK[1]}"
answer_for 2
worker_pid=$(workers_under "$BK_PID")
printf '{"c":"quit"}\n' >&"${BK[1]}"
wait "$BK_PID" 2>/dev/null
if [ "${FLEA_THUMB_WORKER:-on}" != off ]; then
  # The worker takes its request socket closing as the end of work, so it must not outlive the backend that started it.
  worker_exit_s=10
  check "a fresh backend made row 2 through one worker" "1" "$(echo "$worker_pid" | grep -c .)"
  check "and that worker ended with its backend" "0" "$(timeout "$worker_exit_s" tail --pid="$worker_pid" -f /dev/null; echo $?)"
fi
mkdir -p "$D/exec-cache"
printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[2]}\n{"c":"quit"}\n' "$D/worker" \
  | XDG_CACHE_HOME="$D/exec-cache" FLEA_THUMB_WORKER=off timeout 120 $BIN --backend >/dev/null 2>&1
# Sample input: a PNG, whose IDAT chunks are the pixels and whose tEXt chunks are "key\0value", a binary walk no plain tool makes.
# Thumb::Mimetype comes from the input's extension, which the worker's /proc/self/fd/3 lacks, so it is left out.
png_facts() {
  python3 - "$1" <<'PY'
import hashlib, struct, sys
data = open(sys.argv[1], "rb").read()
at, pixels, keys = 8, hashlib.sha256(), []
while at < len(data):
    size = struct.unpack(">I", data[at:at + 4])[0]
    kind, body = data[at + 4:at + 8], data[at + 8:at + 8 + size]
    if kind == b"IDAT":
        pixels.update(body)
    elif kind == b"tEXt" and not body.startswith(b"Thumb::Mimetype\0"):
        keys.append(body.decode("latin-1"))
    at += 12 + size
print(pixels.hexdigest(), sorted(keys))
PY
}
worker_png=$CACHE/large/$w_key.png
exec_png=$D/exec-cache/thumbnails/large/$w_key.png
check "both paths published an entry for the same video" "2" "$(ls "$worker_png" "$exec_png" 2>/dev/null | wc -l | tr -d ' ')"
# Only the program writes Thumb::Mimetype, so the key names the path that made each entry.
check "the exec path made its entry" "1" "$(grep -ac 'Thumb::Mimetype' "$exec_png")"
if [ "${FLEA_THUMB_WORKER:-on}" = off ]; then made_by_program=1; else made_by_program=0; fi
check "and the default run made the other through the ${FLEA_THUMB_WORKER:-on} path" "$made_by_program" "$(grep -ac 'Thumb::Mimetype' "$worker_png")"
exec_facts=$(png_facts "$exec_png")
check "the exec entry's chunks were read" "1" "$(echo "$exec_facts" | grep -c '^[0-9a-f]\{64\} ')"
check "the two publish the same image and keys" "$exec_facts" "$(png_facts "$worker_png")"
chmod 600 "$D/worker/w3-unreadable.mp4"

# Issue #171: a registered thumbnailer outside /usr runs in the jail through one read-only bind of its program; see AGENTS.md "Thumbnail sandbox".
solid_png() {
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import struct, sys, zlib
out, (r, g, b) = sys.argv[1], map(int, sys.argv[2:5])
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
raw = b"".join(b"\x00" + bytes([r, g, b]) * 16 for _ in range(16))
open(out, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 16, 16, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
PY
}
# Sample input: "$D/user-a" 1 asks row 1 of $D/user-a/files with $D/user-a/data as the user's XDG_DATA_HOME.
ask_user_thumbnailer() {
  printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[%s]}\n{"c":"quit"}\n' "$1/files" "$2" \
    | XDG_DATA_HOME="$1/data" timeout 120 $BIN --backend 2>/dev/null
}
write_thumbnailer() {
  mkdir -p "$1/data/thumbnailers"
  printf '[Thumbnailer Entry]\nTryExec=%s\nExec=%s %%i %%o %%s\nMimeType=application/x-mswinurl;\n' "$2" "$2" > "$1/data/thumbnailers/url.thumbnailer"
}
key_of() { printf 'file://%s' "$1" | md5sum | cut -d' ' -f1; }
solid_png "$D/png-a.png" 200 30 30
solid_png "$D/png-b.png" 30 30 200
png_a=$(png_facts "$D/png-a.png" | cut -d' ' -f1)
b64_a=$(base64 -w0 "$D/png-a.png")
b64_b=$(base64 -w0 "$D/png-b.png")

# The program is self-contained: its PNG is embedded and only /usr/bin/base64 is borrowed from /usr.
U=$D/user-a
mkdir -p "$U/bin" "$U/files"
printf '#!/bin/sh\nprintf %%s %s | /usr/bin/base64 -d > "$2"\n' "'$b64_a'" > "$U/bin/drive-url-thumbnail"
chmod 755 "$U/bin/drive-url-thumbnail"
write_thumbnailer "$U" "$U/bin/drive-url-thumbnail"
printf '[InternetShortcut]\nURL=https://example.com/\n' > "$U/files/a.url"
cp "$U/files/a.url" "$U/files/b.url"
check "the suite's shared-mime-info types a .url file as application/x-mswinurl" "1" "$(grep -c ':application/x-mswinurl:\*\.url$' /usr/share/mime/globs2)"
out=$(ask_user_thumbnailer "$U" 0)
check "a thumbnailer outside /usr is asked for its row" "1" "$(echo "$out" | grep -c '"row":0,"file":"/')"
user_png=$CACHE/large/$(key_of "$U/files/a.url").png
check "and the thumbnail it wrote is cached with its own pixels" "$png_a" "$(png_facts "$user_png" 2>/dev/null | cut -d' ' -f1)"
check "and no failure marker is recorded" "0" "$([ -e "$CACHE/fail/flea/$(key_of "$U/files/a.url").png" ] && echo 1 || echo 0)"
# A program another user can rewrite is refused before the jail starts, so no marker either: a refusal is not a verdict on the file.
chmod 775 "$U/bin/drive-url-thumbnail"
out=$(ask_user_thumbnailer "$U" 1)
check "a group-writable thumbnailer program is refused" "1" "$(echo "$out" | grep -c '"row":1,"file":""')"
check "and publishes nothing" "0" "$([ -e "$CACHE/large/$(key_of "$U/files/b.url").png" ] && echo 1 || echo 0)"
check "and records no failure marker" "0" "$([ -e "$CACHE/fail/flea/$(key_of "$U/files/b.url").png" ] && echo 1 || echo 0)"

# The same shape as a jail probe: PNG A is written only when a sibling read, a write to the input and a TCP connection were all refused; a file made beside the input is judged from the host.
V=$D/user-b
mkdir -p "$V/bin" "$V/files"
echo "beside the program" > "$V/bin/sibling.txt"
printf '[InternetShortcut]\nURL=https://example.com/\n' > "$V/files/a.url"
python3 - > "$V/port" 2>/dev/null <<'PY' &
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
s.listen(8)
print(s.getsockname()[1], flush=True)
while True:
    s.accept()
PY
listener=$!
trap 'kill "$listener" 2>/dev/null' EXIT
for _ in $(seq 1 50); do [ -s "$V/port" ] && break; sleep 0.1; done
port=$(cat "$V/port")
check "the TCP listener reported its port within five seconds" "1" "$([ -n "$port" ] && echo 1 || echo 0)"
[ -n "$port" ] || { sandbox_remove "$D"; exit 1; }
cat > "$V/bin/probe-thumbnail" <<'PROBE'
#!/usr/bin/bash
verdict=A
cat "${0%/*}/sibling.txt" >/dev/null 2>&1 && verdict=B
: >> "$1" 2>/dev/null && verdict=B
: > "${1%/*}/planted" 2>/dev/null
(exec 3<>/dev/tcp/127.0.0.1/@PORT@) 2>/dev/null && verdict=B
case $verdict in A) png='@A@' ;; *) png='@B@' ;; esac
printf %s "$png" | /usr/bin/base64 -d > "$2"
PROBE
sed -i "s|@PORT@|$port|; s|@A@|$b64_a|; s|@B@|$b64_b|" "$V/bin/probe-thumbnail"
chmod 755 "$V/bin/probe-thumbnail"
write_thumbnailer "$V" "$V/bin/probe-thumbnail"
# The positive controls: outside the jail every one of the three reaches succeeds, so a refusal inside it is the jail's.
check "outside the jail the sibling file is readable" "0" "$(cat "$V/bin/sibling.txt" >/dev/null 2>&1; echo $?)"
check "and the input is writable" "0" "$(: >> "$V/files/a.url" 2>/dev/null; echo $?)"
check "and a file can be created beside the input" "0" "$(: > "$V/files/control" 2>/dev/null; echo $?)"
check "and the listener accepts a TCP connection" "0" "$( (exec 3<>/dev/tcp/127.0.0.1/"$port") 2>/dev/null; echo $?)"
rm -f "$V/files/control"
out=$(ask_user_thumbnailer "$V" 0)
kill "$listener" 2>/dev/null
check "a probe outside /usr is answered" "1" "$(echo "$out" | grep -c '"row":0,"file":"/')"
check "and the jail refused the sibling read, the write to the input and the connection" "$png_a" "$(png_facts "$CACHE/large/$(key_of "$V/files/a.url").png" 2>/dev/null | cut -d' ' -f1)"
check "and the file the probe created beside the input never reached the host" "0" "$([ -e "$V/files/planted" ] && echo 1 || echo 0)"

# Quick Look prefetch: one cache-only ask for the next row per settled rest, none mid-burst.
prefetch_root="$D/qlprefetch"
prefetch_log="$D/qlprefetch.log"
mkdir -p "$prefetch_root/config" "$prefetch_root/runtime" || { sandbox_remove "$D"; exit 1; }
chmod 700 "$prefetch_root/runtime" || { sandbox_remove "$D"; exit 1; }
ln -s "$PWD/tests/thumbs-qlprefetch.qml" "$prefetch_root/config/shell.qml" || { sandbox_remove "$D"; exit 1; }
# The probe plans with production Thumbs and ExtThumbs, so it reads ui/ as flea.
ln -s "$PWD/ui" "$prefetch_root/config/flea" || { sandbox_remove "$D"; exit 1; }
headless_commons "$prefetch_root" || { sandbox_remove "$D"; exit 1; }
ln -s /usr/share/omarchy/shell/Ui "$prefetch_root/config/Ui" || { sandbox_remove "$D"; exit 1; }
( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$D" XDG_RUNTIME_DIR="$prefetch_root/runtime" TMPDIR="$D" \
    XDG_CONFIG_HOME="$D/.config" XDG_STATE_HOME="$D/.local/state" XDG_CACHE_HOME="$D/cache" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    PREVIEW_UI="$PWD/ui" \
    timeout 60 qs -p "$prefetch_root/config" > "$prefetch_log" 2>&1 )
prefetch_pass=$(grep -ac 'THUMBPREFETCH PASS' "$prefetch_log")
prefetch_fail=$(grep -ac 'THUMBPREFETCH FAIL' "$prefetch_log")
prefetch_done=$(grep -ac 'THUMBPREFETCH DONE failures=0$' "$prefetch_log")
check "a rest asks once, nothing asks mid-burst, the trailing rest asks once more" "23" "$prefetch_pass"
check "and the probe reports no failure" "0" "$prefetch_fail"
check "with a clean DONE" "1" "$prefetch_done"
if [ "$prefetch_done" != 1 ]; then
    grep -a -E 'THUMBPREFETCH|TypeError|ReferenceError|ERROR' "$prefetch_log" | head -30
fi

# -A, never ls: the one kind of litter this subsystem leaves is a dotfile temp a bare ls cannot see.
check "no temp file is left in the cache this run filled" "0" "$(ls -A "$CACHE/large" 2>/dev/null | grep -c '^\.flea-')"
sandbox_remove "$D"

# The redirect is what makes this structural rather than a promise, so it is asserted.
check "the operator's own cache is untouched" "$real_before" "$(ls -A "$REAL_CACHE/large" 2>/dev/null | wc -l)"

[ "$fail" -eq 0 ] && echo "thumbs: all checks passed"
exit $fail
