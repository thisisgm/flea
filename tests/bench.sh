#!/bin/bash
# Gates tools/flea-bench-manifest, the record that makes a bench number citable, and the two values
# the harness derives from its own entrant table to feed it. Nothing here launches an entrant: this
# suite exists because the field bench had no gate at all and its first manifest reported Flea as
# quickshell 0.3.1.
set -u
# The cd comes first: a trap installed above it fires against the caller's working directory when
# the cd fails, which is a relative-path rm -rf in somebody else's tree.
cd "$(dirname "$0")/.." || exit 1
repo=$PWD
tool="$repo/tools/flea-bench-manifest"

# The fixture is a mktemp -d of its own, which is what hard rule 9 allows outright. Nothing here
# reads or writes the real fixture root, and the only delete is checked absolute below.
scratch=$(mktemp -d) || exit 1
# GNU mktemp -d honours a relative TMPDIR verbatim, so the one delete this suite makes is checked
# absolute and two components deep here, before the trap that runs it is installed.
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep"; exit 1 ;;
esac
: > "$scratch/.flea-test-sandbox"
# The manifest defaults FLEA_REPO to the operator's checkout; this suite binds no build of the host's.
export FLEA_REPO="$scratch/no-flea-tree"
trap 'case $scratch in /*/*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}
holds() {
  local label="$1" needle="$2" file="$3"
  if grep -q -- "$needle" "$file"; then echo "ok   $label"; else
    echo "FAIL $label: '$needle' is not in $file"; fail=1
  fi
}
lacks() {
  local label="$1" needle="$2" file="$3"
  if grep -q -- "$needle" "$file"; then
    echo "FAIL $label: '$needle' should not be in $file"; fail=1
  else echo "ok   $label"; fi
}

fixture="$scratch/fix"
mkdir -p "$fixture"
touch "$fixture/a.jpg" "$fixture/b.jpg" "$fixture/c.png" "$fixture/d.txt" "$fixture/README"
payload=$(ls -U "$fixture" | wc -l)
# Strata runs as two required rows, stable and nightly, each its own executable.
mkdir -p "$scratch/strata-stable" "$scratch/strata-nightly"
printf '#!/bin/sh\necho "strata 1.1.0"\n' > "$scratch/strata-stable/strata"
printf '#!/bin/sh\necho "strata 1.0.1-nightly.20261018"\n' > "$scratch/strata-nightly/strata"
chmod +x "$scratch/strata-stable/strata" "$scratch/strata-nightly/strata"
export STRATA_BIN="$scratch/strata-stable/strata" STRATA_NIGHTLY_BIN="$scratch/strata-nightly/strata"

"$tool" >/dev/null 2>&1
check "no arguments is a usage error" 2 $?
"$tool" "$scratch/m1.md" "$scratch/nothing-here" >/dev/null 2>&1
check "a missing fixture is refused" 1 $?

# Named beside $scratch/run.csv below, because flea-bench-report finds a manifest by the CSV's own
# name. Every entrant that run holds is listed here too: the report refuses an id it has no kind for.
man="$scratch/run.manifest.md"
printf '%s\n' "flea|gui|qs" "dolphin|gui|dolphin" "definitely-not-a-file-manager|gui|definitely-not-a-file-manager" \
  "strata|gui|strata" "strata-nightly|gui|strata" "yazi|tui|yazi" "xplr|tui|xplr" \
  | EXPECT_FILES="$payload" "$tool" "$man" "$fixture" >/dev/null 2>&1
check "a normal run writes a manifest" 0 $?

holds "the payload is counted from the fixture" "fixture payload: $payload visible entries" "$man"
holds "the histogram counts the jpgs it holds" "2 jpg" "$man"
mkdir -p "$scratch/latin1"
touch "$scratch/latin1/$(printf 'caf\351.jpg')"
printf '%s\n' "flea|gui|qs" | "$tool" "$scratch/latin1.md" "$scratch/latin1" >/dev/null 2>&1
holds "the histogram counts a Latin-1 filename as jpg" "1 jpg" "$scratch/latin1.md"
holds "and the one file with no extension at all" "1 no extension" "$man"
holds "an entrant that is not installed is named" "NOT INSTALLED" "$man"
holds "flea's row is its own source, not a package" "^flea .* source " "$man"
# The defect this check exists for: an untrimmed id missed its own case arm, so flea fell to the
# packaged arm and pacman -Qo on its launcher reported it as quickshell.
lacks "and flea never reports itself as its launcher" "flea.*quickshell" "$man"
holds "a package that is not installed reads absent" " absent" "$man"

# The same rows indented, which is how they arrive when they are grepped out of the harness rather
# than expanded from its array. This is the whole of the trim regression.
indented="$scratch/indented.md"
printf '%s\n' '  flea|gui|qs' '  dolphin|gui|dolphin' | "$tool" "$indented" "$fixture" >/dev/null 2>&1
holds "an indented row still reaches its own arm" "^flea .* source " "$indented"
lacks "and is not silently mis-attributed" "flea.*quickshell" "$indented"

# The denominator and the run conditions, both added because a number nobody can re-derive is a
# number nobody can check. The fixture holds one txt of its five files.
holds "the denominator excludes what nobody thumbnails" "thumbnailable denominator: 4" "$man"
holds "and names what it excluded" "1 .txt files" "$man"
holds "the load average is recorded, not assumed" "load average at start:" "$man"
holds "and the payload assertion is recorded as having passed" "payload assertion: PASSED" "$man"
# The line used to write PASSED from the presence of EXPECT_FILES alone, so a wrong count still read
# as a pass. It compares now, and this is the check that says so.
wrongman="$scratch/wrong.md"
printf '%s\n' "flea|gui|qs" | EXPECT_FILES=999 "$tool" "$wrongman" "$fixture" >/dev/null 2>&1
holds "a payload that does not match is recorded as a failure" "payload assertion: \*\*FAILED\*\*" "$wrongman"
lacks "and is not also recorded as a pass" "payload assertion: PASSED" "$wrongman"

# tools/flea-bench-keys. The shell key derivation is proved to match the backend's by
# tests/thumbs.sh, which asks the real binary for a thumbnail and finds it at the key this same
# printf-and-md5sum produces. What is proved here is that the map and the classifier agree with the
# fixture they are given.
keys="$repo/tools/flea-bench-keys"
map="$scratch/fix.map"
"$keys" map "$fixture" "$map" >/dev/null 2>&1
check "a map builds from a plain fixture" 0 $?
check "one line per fixture file" "$payload" "$(wc -l < "$map")"
check "and the extensions are counted off the names" "2" "$(cut -d' ' -f2 "$map" | grep -c '^jpg$')"
check "a name with no dot maps to none" "1" "$(cut -d' ' -f2 "$map" | grep -c '^none$')"

# The refusal that keeps a wrong key from looking like a zero result for every entrant at once.
mkdir -p "$scratch/needs-encoding"
touch "$scratch/needs-encoding/two words.jpg"
"$keys" map "$scratch/needs-encoding" "$scratch/enc.map" >/dev/null 2>&1
check "a name needing percent-encoding is refused" 1 $?

# An entrant that thumbnailed both jpgs and the png, and nothing else.
produced="$scratch/produced.keys"
grep -E ' (jpg|png)$' "$map" | cut -d' ' -f1 | LC_ALL=C sort -u > "$produced"
line=$("$keys" classify "$map" "$produced")
case $line in
  *"jpg=2"*) echo "ok   the classifier counts the jpgs" ;;
  *) echo "FAIL the classifier said '$line'"; fail=1 ;;
esac
case $line in
  *"txt=0"*) echo "ok   and a format nobody produced still gets a row" ;;
  *) echo "FAIL a zero format has no row in '$line'"; fail=1 ;;
esac
case $line in
  *"unknown=0"*) echo "ok   with nothing unexplained" ;;
  *) echo "FAIL unknown is not zero in '$line'"; fail=1 ;;
esac

# A key the fixture cannot explain means the cache was not clean, so the row is not comparable.
printf '%s\n' "ffffffffffffffffffffffffffffffff" >> "$produced"
LC_ALL=C sort -u -o "$produced" "$produced"
case $("$keys" classify "$map" "$produced") in
  *"unknown=1"*) echo "ok   a key the fixture does not explain is named" ;;
  *) echo "FAIL an unexplained key was folded into the total"; fail=1 ;;
esac

: > "$scratch/empty.keys"
check "an entrant that produced nothing is a result, not an error" 0 \
  "$("$keys" classify "$map" "$scratch/empty.keys" >/dev/null 2>&1; echo $?)"

# The close pass. Its CSV header and every row are GENERATED from the bench's own header line, so
# the fixture cannot fall a column behind the artefact it tests. It did exactly that: preview_ms was
# appended as column 16, this fixture stayed at 15, its rows still ended in ",yes", and the suite
# stayed green over a refusal that had stopped firing in production.
bench_header=$(sed -n 's/^echo "\(id,run,[^"]*\)".*/\1/p' "$repo/tools/flea-field-bench")
[ -n "$bench_header" ] || { echo "FAIL: could not read the bench's own CSV header"; fail=1; }
declare -A CELL
emit_row() {
  local out="" name
  # The report counts only write_clean 1 rows, so a fixture row is a counted run unless it says otherwise.
  CELL[write_clean]=${CELL[write_clean]-1}
  while IFS= read -r name; do
    out="${out:+$out,}${CELL[$name]:--}"
  done < <(printf '%s\n' "$bench_header" | tr ',' '\n')
  printf '%s\n' "$out"
  CELL=()
}

csv="$scratch/run.csv"
keys_dir="$scratch/run.keys"
mkdir -p "$keys_dir"
cp "$map" "$keys_dir/fixture.map"
{
  printf '%s\n' "$bench_header"
  CELL=([id]=flea [run]=1 [thumbs_n]=36 [thumbs_by_format]="jpg=2;png=0;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=dolphin [run]=1 [thumbs_n]=790 [thumbs_by_format]="jpg=2;png=1;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=strata [run]=1 [thumbs_n]=unmeasurable [thumbs_by_format]=unmeasurable [ranked]="unranked: nothing reached the thumbnail cache so its work was not measured" [preview_ms]=-)
  emit_row
  CELL=([id]=yazi [run]=1 [thumbs_n]=n/a [thumbs_by_format]=n/a [ranked]="n/a: tui in kitty" [preview_ms]=745)
  emit_row
  CELL=([id]=xplr [run]=1 [thumbs_n]=n/a [thumbs_by_format]=n/a [ranked]="n/a: tui in kitty" [preview_ms]=-1)
  emit_row
} > "$csv"
# The generated fixture has to be exactly as wide as the header it was generated from, or the
# generation is the next defect rather than the fix for the last one.
want_cols=$(printf '%s\n' "$bench_header" | tr ',' '\n' | wc -l)
got_cols=$(sed -n '2p' "$csv" | tr ',' '\n' | wc -l)
check "the synthetic run is as wide as the bench's own header" "$want_cols" "$got_cols"

# One refused key for dolphin, so the three-state line has something in every state to report.
# It sits on an entrant that also produced thumbnails, because that is the only shape a refusal
# marker occurs in: an entrant the cache never saw at all leaves no marker either.
grep ' jpg$' "$map" | head -1 | cut -d' ' -f1 > "$keys_dir/dolphin-run1.fail.keys"

"$tool" "$man" close "$csv" >/dev/null 2>&1
check "the manifest closes against a finished run" 0 $?
holds "an entrant's produced formats are named" "flea produced jpg 2" "$man"
holds "a format nobody reached is never-attempted, not absent" "flea .*never attempted .*png 1" "$man"
holds "a refusal marker is its own state" "dolphin produced .*; refused jpg 1" "$man"
holds "an entrant the cache never saw makes no format claim" "strata: nothing reached the thumbnail cache" "$man"
lacks "and is never recorded as having produced nothing" "strata produced nothing" "$man"
holds "a TUI is not given a format line it cannot earn" "yazi: a TUI previews the cursor file" "$man"
holds "every unranked row is repeated as a refusal" "strata run 1: unranked" "$man"
holds "including the TUI group's" "yazi run 1: n/a: tui in kitty" "$man"
holds "and a ten-times work gap refuses the comparison outright" "differ in thumbnail work by 21x" "$man"
lacks "so the manifest never calls that run comparable" "every row in this run is comparable" "$man"
holds "an entrant with no preview feature is stated, not left as a sentinel" "xplr: no image preview feature" "$man"
"$tool" "$man" close "$scratch/nothing.csv" >/dev/null 2>&1
check "closing against a missing run is refused" 1 $?
# A second close would append a second set of blocks rather than replace the first, and a manifest
# with two Run close sections is a document a reader has to reconcile.
"$tool" "$man" close "$csv" >/dev/null 2>&1
check "closing an already-closed manifest is refused" 1 $?
check "and it still carries exactly one close section" 1 "$(grep -c '^## Run close' "$man")"
# An unset or missing FLEA_QMLCACHE reads absent in the close section, never a 0.
nocache_man="$scratch/nocache.manifest.md"
printf '%s\n' "flea|gui|qs" "dolphin|gui|dolphin" "definitely-not-a-file-manager|gui|definitely-not-a-file-manager" \
  "strata|gui|strata" "strata-nightly|gui|strata" "yazi|tui|yazi" "xplr|tui|xplr" \
  | EXPECT_FILES="$payload" "$tool" "$nocache_man" "$fixture" >/dev/null 2>&1
env -u FLEA_QMLCACHE "$tool" "$nocache_man" close "$csv" >/dev/null 2>&1
check "a close with no cache variable still closes" 0 $?
holds "an unset FLEA_QMLCACHE reads absent, never 0" "tested Flea's own QML cache at end: absent entries in an unset path" "$nocache_man"
missing_man="$scratch/missingcache.manifest.md"
printf '%s\n' "flea|gui|qs" "dolphin|gui|dolphin" "definitely-not-a-file-manager|gui|definitely-not-a-file-manager" \
  "strata|gui|strata" "strata-nightly|gui|strata" "yazi|tui|yazi" "xplr|tui|xplr" \
  | EXPECT_FILES="$payload" "$tool" "$missing_man" "$fixture" >/dev/null 2>&1
FLEA_QMLCACHE="$scratch/no-such-dir" "$tool" "$missing_man" close "$csv" >/dev/null 2>&1
check "a close with a missing cache dir still closes" 0 $?
holds "a missing cache dir reads absent too" "tested Flea's own QML cache at end: absent entries in $scratch/no-such-dir" "$missing_man"

# The anchor and TUI arms, each recorded from its own repo rather than the package.
command -v git >/dev/null 2>&1 || { echo "FAIL git is required for the anchor manifest checks"; fail=1; }
flea_repo="$scratch/flea-repo"
anchor_repo="$scratch/anchor-repo"
mkdir -p "$flea_repo" "$anchor_repo"
# Fixture repos read no operator config, so a signing or hook setting cannot fail their commits.
fixture_git() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c user.email=t@t -c user.name=t "$@"; }
fixture_git -C "$flea_repo" init -q
fixture_git -C "$anchor_repo" init -q
printf 'x\n' > "$flea_repo/f" && printf 'x\n' > "$anchor_repo/f"
fixture_git -C "$flea_repo" add f
fixture_git -C "$flea_repo" commit -qm init
fixture_git -C "$anchor_repo" add f
fixture_git -C "$anchor_repo" commit -qm init
fixture_git -C "$anchor_repo" tag v9.9.9
flea_sha=$(git -C "$flea_repo" rev-parse --short HEAD)
anchor_sha=$(git -C "$anchor_repo" rev-parse --short HEAD)
anchor_man="$scratch/anchor.manifest.md"
printf '%s\n' "flea-tui|tui|flea" "flea-anchor|gui|qs" \
  | FLEA_REPO="$flea_repo" ANCHOR_REPO="$anchor_repo" "$tool" "$anchor_man" "$fixture" >/dev/null 2>&1
check "a manifest run with anchor arms writes" 0 $?
holds "flea-tui names its own repo sha, untagged" "^flea-tui .*source $flea_sha untagged (clean)" "$anchor_man"
holds "flea-anchor names the anchor repo sha and its tag" "^flea-anchor .*source $anchor_sha v9.9.9 (clean)" "$anchor_man"
noanchor_man="$scratch/noanchor.manifest.md"
printf '%s\n' "flea-anchor|gui|qs" \
  | FLEA_REPO="$flea_repo" env -u ANCHOR_REPO "$tool" "$noanchor_man" "$fixture" >/dev/null 2>&1
check "a manifest run with no anchor repo writes" 0 $?
holds "and the anchor arm reads absent without one" "^flea-anchor .*source absent absent (absent)" "$noanchor_man"
badrepo_man="$scratch/badrepo.manifest.md"
mkdir -p "$scratch/not-a-repo"
printf '%s\n' "flea|gui|qs" \
  | GIT_CEILING_DIRECTORIES="$scratch" FLEA_REPO="$scratch/not-a-repo" "$tool" "$badrepo_man" "$fixture" >/dev/null 2>&1
check "a manifest run with an unreadable repo writes" 0 $?
holds "and the arm reads unreadable rather than clean" "^flea .*source unreadable unreadable (unreadable)" "$badrepo_man"

# ---------------------------------------------------------------- tools/flea-bench-report
# The report renders the work column that carried the wrong claim. A cache count that saw nothing
# must reach the page as "not measured": a 0 there was published as a capability claim about strata
# for the whole v0.1.0 release while it drew six of the eight formats offered.
report_out="$scratch/report.md"
"$repo/tools/flea-bench-report" "$csv" > "$report_out" 2>"$scratch/report.err"
check "the report runs against a closed manifest" 0 $?
lacks "a sentinel never reaches the page as a number" "unmeasurable thumbnails" "$report_out"
lacks "and an unseen entrant is never called a zero" "| 0 thumbnails |" "$report_out"
holds "it reads not measured instead" "strata.*not measured" "$report_out"
holds "and the refusal names the instrument, not the entrant" "nothing reached the thumbnail cache" "$report_out"
lacks "the old claim about the entrant's ability is gone" "drew no thumbnails" "$report_out"
holds "the report heads the GUI section group" "## GUI group" "$report_out"
holds "and the TUI section too" "## TUI group" "$report_out"
lacks "the old GUI bracket heading is gone" "## GUI bracket" "$report_out"
lacks "and the old TUI bracket heading too" "## TUI bracket" "$report_out"
# The chart's eyebrow names the section the same way; the SVG is written before any PNG step, so it is read either way.
mkdir -p "$scratch/chart"
if ! "$repo/tools/flea-bench-chart" "$scratch/chart" >/dev/null 2>"$scratch/chart.err" && [ ! -s "$scratch/chart/bench-scale.svg" ]; then
  echo "FAIL tools/flea-bench-chart wrote no bench-scale.svg into $scratch/chart:"; sed 's/^/  /' "$scratch/chart.err"; fail=1
fi
holds "the chart eyebrow names the GUI group" "FIELD BENCH, GUI GROUP" "$scratch/chart/bench-scale.svg"
lacks "and never the old GUI bracket" "GUI BRACKET" "$scratch/chart/bench-scale.svg"
# The chart finds GUI cells by header name, so a moved column keeps the numbers.
reorder_orig="$scratch/reorder-orig.md"
"$repo/tools/flea-bench-report" "$repo/docs/bench/scale-rc-2026.csv" > "$reorder_orig" 2>/dev/null
check "a reorderable GUI report runs" 0 $?
reordered="$scratch/reordered.md"
python3 - "$reorder_orig" "$reordered" <<'EOF'
import sys
src, dst = sys.argv[1], sys.argv[2]
lines = open(src, encoding="utf-8").read().splitlines()
out = []
for line in lines:
    if line.startswith("|"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 3:
            cells[1], cells[2] = cells[2], cells[1]
        line = "| " + " | ".join(cells) + " |"
    out.append(line)
open(dst, "w", encoding="utf-8").write("\n".join(out) + "\n")
EOF
check "a reordered GUI table is still writable" 0 $?
python3 - "$repo/tools/flea-bench-chart" "$reorder_orig" "$reordered" <<'EOF'
import sys
chart, orig, new = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(chart, encoding="utf-8").read().splitlines()
src = [l for l in src if l.strip() != "main()"]
ns = {}
exec(compile("\n".join(src), chart, "exec"), ns)
parse = ns["parse_gui"]
def sig(md):
    rows, un = parse(md)
    r = sorted((x["name"], x["first"], x["settled"], x["work"], x["mem"], x["cpu"], x["runs"]) for x in rows)
    u = sorted((x["name"], x["settled"], x["work"]) for x in un)
    return (r, u)
try:
    a = sig(open(orig, encoding="utf-8").read())
    b = sig(open(new, encoding="utf-8").read())
except SystemExit as e:
    print("chart refused a GUI table: %s" % e.code)
    sys.exit(1)
sys.exit(0 if a == b and a[0] else 1)
EOF
check "the chart reads a reordered GUI table the same way" 0 $?
# A verdict holding a comma shifts every column right of it, and the failure then surfaces against
# whichever column the shifted row lands in rather than against the verdict that caused it.
comma_csv="$scratch/comma.csv"
sed 's/cache so its work was not measured/cache, so its work was not measured/' "$csv" > "$comma_csv"
cp "$man" "${comma_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$comma_csv" >/dev/null 2>"$scratch/comma.err"
check "a comma inside a verdict is refused" 1 $?
holds "and the refusal names the field that holds it" "so a field holds a comma" "$scratch/comma.err"
# A dirty row's comma sits in ranked, so the raw check names it only while ranked comes before write_clean.
ranked_at=$(printf '%s\n' "$bench_header" | tr ',' '\n' | grep -n '^ranked$' | cut -d: -f1)
clean_at=$(printf '%s\n' "$bench_header" | tr ',' '\n' | grep -n '^write_clean$' | cut -d: -f1)
[ "${ranked_at:-0}" -ge 1 ] && [ "${ranked_at:-0}" -lt "${clean_at:-0}" ]
check "the bench header still orders ranked before write_clean" 0 $?
dirtycomma_csv="$scratch/dirtycomma.csv"
cp "$csv" "$dirtycomma_csv"
CELL=([id]=flea [run]=2 [thumbs_n]=36 [thumbs_by_format]="jpg=2;png=0;txt=0;none=0;unknown=0" [ranked]="unranked: writes in flight, dirty" [preview_ms]=- [write_clean]=0)
emit_row >> "$dirtycomma_csv"
cp "$man" "${dirtycomma_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$dirtycomma_csv" >/dev/null 2>"$scratch/dirtycomma.err"
check "a comma in a dirty row the guard would drop is refused" 1 $?
holds "and the dirty refusal names the field that holds it" "so a field holds a comma" "$scratch/dirtycomma.err"
holds "and the dirty refusal is the raw check naming the row" "flea's row has" "$scratch/dirtycomma.err"

# The two CSVs this repo ships. Their values are not asserted here, only that the report can read
# them: rows have been spliced into these by hand and a lost column is the failure that produces.
for shipped in scale-rc-2026 media-rc-2044; do
  "$repo/tools/flea-bench-report" "$repo/docs/bench/$shipped.csv" >/dev/null 2>&1
  check "the shipped $shipped.csv still parses" 0 $?
done

# The launch line the harness derives from its own table, and the terminal it names once. A renamed
# or reordered field makes the first empty, and an empty launch line in the manifest is the exact
# trap hard rule 7 names first. The second was derived from the class field and quietly resolved to
# a window class rather than a program, which the manifest would have printed as "absent".
bench="$repo/tools/flea-field-bench"
rows=$(sed -n '/^  declare -ga ENTRANTS=(/,/^  )/p' "$bench" | grep '^    "' | tr -d '"')
# -f8-, matching the bench: the command field may hold a pipe and cut -f8 would stop at it.
flea_cmd=$(printf '%s\n' "$rows" | grep '^    flea|' | cut -d'|' -f8-)
tui_term=$(grep -m1 '^TUI_TERM=' "$bench" | cut -d= -f2)
# The header and the row are written in two different places, so a column added to one and not the
# other produces a CSV that parses and is wrong from that column rightward.
header_fields=$(sed -n 's/^echo "\(id,run,[^"]*\)".*/\1/p' "$bench" | tr ',' '\n' | wc -l)
row_fields=$(grep '^  printf .%s,%s' "$bench" | head -1 | grep -o '%s' | wc -l)
check "the CSV header and the row have the same column count" "$header_fields" "$row_fields"

# tools/flea-bench-capability reads the same entrant table. A row extraction that comes back empty
# would report every entrant as capable of nothing, which reads as a result.
cap_gui=$(sed -n '/^  declare -ga ENTRANTS=(/,/^  )/p' "$bench" | grep '^    "' | tr -d '"' | grep -c '|gui|')
if [ "$cap_gui" -ge 5 ]; then
  echo "ok   the capability pass can see the GUI entrants: $cap_gui of them"
else
  echo "FAIL the capability pass derived $cap_gui GUI entrants from the bench table"; fail=1
fi

case $flea_cmd in
  *"--gui"*) echo "ok   the flea launch line is derived and names --gui" ;;
  *) echo "FAIL the flea launch line derived to '$flea_cmd'"; fail=1 ;;
esac
if [ -n "$tui_term" ] && command -v "$tui_term" >/dev/null 2>&1; then
  echo "ok   the TUI terminal is derived and installed: $tui_term"
else
  echo "FAIL the TUI terminal derived to '$tui_term', which is not an installed program"; fail=1
fi

# The harness's control-group helpers, pulled out whole: only a field run executed them before, and it lost thunar and yazi.
eval "$(sed -n '/^pids_for() {/,/^}/p; /^cpu_ticks() {/,/^}/p; /^find_helper() {/,/^}/p; /^settle_ticks() {/,/^}/p; /^entrant_cg() {/,/^}/p; /^cg_empty() {/,/^}/p; /^sweep_cg() {/,/^}/p; /^release_run_cg() {/,/^}/p' "$bench")"
eval "$(grep -m1 '^HELPER_SEARCH_POLLS=' "$bench")"
declare -A WATCHED=()
own_cg="/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)"
not_a_leaf="$scratch/not-a-leaf"
# Named copies of sleep, so pgrep -x finds this suite's stand-ins and nothing else on the box.
cp "$(command -v sleep)" "$scratch/fbhelper" && cp "$(command -v sleep)" "$scratch/fbtui" || exit 1
# Longer than sweep_cg can wait, so an unswept stand-in is still alive when the check reads it.
stand_in_s=120
"$scratch/fbhelper" "$stand_in_s" & helper=$!
# Until the forked child has exec'd, its comm is still bash and pgrep -x cannot see it.
exec_polls=50 poll_s=0.1
for _ in $(seq "$exec_polls"); do [ "$(cat "/proc/$helper/comm" 2>/dev/null)" = fbhelper ] && break; sleep "$poll_s"; done
check "the helper stand-in has exec'd before settle_ticks looks for it" fbhelper "$(cat "/proc/$helper/comm" 2>/dev/null)"

# tumblerd's shape: a helper outside the leaf, found on the first poll, while the entrant is alive.
RUN_CG=$not_a_leaf HELPER_COMM=fbhelper HELPER_TOKEN="" HELPER_PID="" HELPER_POLLS_LEFT=0 HELPER_SIDE=""
settle_ticks $$
check "a helper outside the leaf leaves settle_ticks true, as tumblerd beside thunar" 0 $?
check "and that helper is recorded as outside" outside "$HELPER_SIDE"
kill "$helper"; wait "$helper" 2>/dev/null

RUN_CG=$own_cg ENTRANT_SCOPE=""
entrant_cg $$ gui
check "an entrant in the run's own group is in its group" 0 $?
# systemd-run execs the stand-in in a scope named the way kitty 0.48 names its own, after this shell.
systemd-run --user --scope -q --unit="kitty-$$-7" "$scratch/fbtui" "$stand_in_s" & tui=$!
placed=no
placement_polls=50
for _ in $(seq "$placement_polls"); do
  case "$(cut -d: -f3 "/proc/$tui/cgroup" 2>/dev/null)" in */kitty-$$-7.scope) placed=yes; break ;; esac
  sleep "$poll_s"
done
check "systemd placed the stand-in TUI in kitty-$$-7.scope" yes "$placed"
scope_cg="/sys/fs/cgroup$(cut -d: -f3 "/proc/$tui/cgroup" 2>/dev/null)"
# Only a group proved to be this scope is ever swept: an empty read would make it the root group.
case $scope_cg in
  */kitty-$$-7.scope)
    entrant_cg "$tui" gui
    check "a GUI entrant is never taken from a kitty scope" 1 $?
    RUN_CG=$not_a_leaf ENTRANT_SCOPE=""
    entrant_cg "$tui" tui
    check "a kitty scope whose kitty is outside the leaf does not count" 1 $?
    check "and is never recorded for the sweep, which would end an operator's own kitty" "" "$ENTRANT_SCOPE"
    RUN_CG=$own_cg
    entrant_cg "$tui" tui
    check "a TUI in the scope of a kitty inside the leaf counts" 0 $?
    check "and that scope is recorded for the sweep" "$scope_cg" "$ENTRANT_SCOPE"
    # release_run_cg kills whatever scope entrant_cg recorded, so it runs only on the one this arm proved.
    if [ "$ENTRANT_SCOPE" = "$scope_cg" ]; then
      # A plain directory stands in for the leaf, so the kitty scope is the only group holding a process.
      BENCH_CG=$scratch RUN_CG="$scratch/run-bench"
      mkdir "$RUN_CG"
      release_run_cg 2> "$scratch/sweep.err"
      # Sample input: /proc/<pid>/stat "4242 (fbtui) S 4241 ...", state third; Z or no file is ended.
      state=$(cut -d' ' -f3 "/proc/$tui/stat" 2>/dev/null)
      case $state in ''|Z) state=ended ;; esac
      check "release_run_cg ends what is left in the kitty scope" ended "$state"
      check "and forgets the scope once it is swept" "" "$ENTRANT_SCOPE"
      holds "and names it as a leftover" "LEFTOVER: pid $tui (fbtui)" "$scratch/sweep.err"
    else
      echo "FAIL entrant_cg recorded '$ENTRANT_SCOPE' rather than the proved $scope_cg, so release_run_cg was not run"; fail=1
    fi
    kill -9 "$tui" 2>/dev/null; wait "$tui" 2>/dev/null
    ;;
  *) echo "FAIL the stand-in's group read as '$scope_cg', so the scope checks did not run"; fail=1; kill "$tui" 2>/dev/null ;;
esac

# nautilus's indexer outlives it and crawled $HOME under every later entrant's cold launch, so the kill list ends it after nautilus.
FLEA_UI=/nonexistent ANCHOR_UI=/nonexistent FIXTURE="$fixture" TUI_TERM=kitty TUI_CLASS=flea-bench-kitty
eval "$(sed -n '/^declare -a KILL_TARGETS=(/,/^)/p; /^kill_comms_for() {/,/^}/p' "$bench")"
kill_rows=" ${KILL_TARGETS[*]} "
case $kill_rows in *" localsearch-3| "*) indexer=listed ;; *) indexer=missing ;; esac
check "the kill list ends nautilus's indexer, localsearch-3" listed "$indexer"
case $kill_rows in *" localsearch-ext| "*) extractor=listed ;; *) extractor=missing ;; esac
check "and its extractor, by the 15-character comm the kernel keeps" listed "$extractor"
check "ONLY=nautilus scopes the indexer and its extractor in" "nautilus localsearch-3 localsearch-ext" "$(kill_comms_for nautilus | tr '\n' ' ' | sed 's/ $//')"

# ---------------------------------------------------------------- fieldnet: NAS and USB fixtures
# None of this launches an entrant; see AGENTS.md "Testing" for the gvfs settle clock.
bash -n "$bench"
check "the bench parses" 0 $?
check "ONLY=flea-tui resolves to the flea kill row" "flea" "$(kill_comms_for flea-tui | tr '\n' ' ' | sed 's/ $//')"
case $rows in
  *"flea-tui|tui|"*) echo "ok   the flea-tui entrant rides the TUI group" ;;
  *) echo "FAIL the flea-tui entrant is missing from the bench table"; fail=1 ;;
esac
eval "$(sed -n '/^settle_gap_for() {/,/^}/p' "$bench")"
check "the settle gap defaults to 500 on a local fixture" 500 "$(settle_gap_for /home/flea-sandbox/flea-bench-btrfs)"
check "and to 2000 on a gvfs fixture" 2000 "$(settle_gap_for /run/user/1000/gvfs/smb-share:server=nas,share=data)"

# cpu_sys_s is the run tree plus each mount daemon's own delta during it.
eval "$(sed -n '/^mount_ticks_start() {/,/^}/p; /^mount_ticks_delta() {/,/^}/p; /^sys_seconds() {/,/^}/p' "$bench")"
declare -A FAKE_TICKS=([50001]=100 [50002]=50)
cpu_ticks() {
  local pid=$1
  [ -n "${FAKE_TICKS[$pid]:-}" ] || return 1
  CPU_TICKS=${FAKE_TICKS[$pid]}
  CPU_TREE_TICKS=$(( CPU_TICKS * 3 ))
}
MOUNT_PIDS=(50001 50002)
mount_ticks_start
check "each daemon baseline records its own ticks" "100" "${MOUNT_START[50001]}"
check "and the second daemon too" "50" "${MOUNT_START[50002]}"
FAKE_TICKS=([50001]=140 [50002]=80)
mount_ticks_delta; rc=$?
check "start and settle sum each daemon's own delta" "0" "$rc"
check "forty ticks on one daemon and thirty on the other" "70" "$MOUNT_DTICKS"
check "sys_seconds adds the daemon delta" "1.92" "$(sys_seconds 1.42 50 100)"
check "sys_seconds equals tree with no daemons" "1.42" "$(sys_seconds 1.42 0 100)"
FAKE_TICKS=([50001]=100 [50002]=50)
mount_ticks_start
unset 'FAKE_TICKS[50002]'
mount_ticks_delta; rc=$?
check "a daemon that exits mid-run fails the row" "1" "$rc"
check "and names the lost daemon" "50002" "$MOUNT_LOST"
FAKE_TICKS=([50001]=100 [50002]=50)
mount_ticks_start
FAKE_TICKS=([50001]=90 [50002]=50)
mount_ticks_delta; rc=$?
check "a daemon whose ticks go backwards fails the row" "1" "$rc"
check "and names the reused pid" "50001" "$MOUNT_LOST"
FAKE_TICKS=([50001]=100)
mount_ticks_start
FAKE_TICKS=([50001]=100 [50002]=500)
mount_ticks_delta; rc=$?
check "a daemon unreadable at launch fails the row" "1" "$rc"
check "and names the daemon that had no baseline" "50002" "$MOUNT_LOST"
MOUNT_PIDS=()
mount_ticks_start
mount_ticks_delta; rc=$?
check "no mount daemons deltas to zero" "0" "$MOUNT_DTICKS"
check "and an empty daemon set never fails" "0" "$rc"
# Header and row are pinned whole, so a column moved or dropped in either one fails here.
# Sample input, one run_one line: echo "id,run,mapped_ms,settled_ms,..." (the header, quoted).
header_line=$(sed -n 's/^echo "\(id,run,[^"]*\)".*/\1/p' "$bench")
check "the CSV header names every column in order" "id,run,mapped_ms,settled_ms,pss_kb,cpu_s,child_pss_kb,thumbs_n,pss_anon_kb,uss_kb,drm_kb,thumbs_by_dir,cpu_tree_s,thumbs_by_format,ranked,preview_ms,cpu_reaped_s,cpu_sys_s,dirty_wb_kb,pgpgout_pre_kb,pgpgout_launch_kb,write_clean,rerun,first_rows_ms,cpu_main_s,cold_pss_kb,cold_drm_kb,cold_child_pss_kb,cold_sample_after_map_ms,launch_pos" "$header_line"
row_line=$(grep '^  printf .%s,%s' "$bench" | head -1)
row_specs=$(printf '%s\n' "$row_line" | grep -o '%s' | wc -l)
check "run_one's row carries one %s per header column" "$(printf '%s\n' "$header_line" | tr ',' '\n' | wc -l)" "$row_specs"
# Sample input, one run_one line: printf '%s,%s...\\n...' "$id" "$RUN_INDEX" ... (the row, quoted).
row_args=$(printf '%s\n' "$row_line" | sed 's/^.*\\n. //')
check "run_one's row prints the header's columns in its order" '"$id" "$RUN_INDEX" "$mapped" "$settled" "$best_pss" "$cpu_s" "$child_pss" "$thumbs_n" "$best_anon" "$best_uss" "$DRM_KB" "$thumbs_dirs" "$cpu_tree_s" "$thumbs_fmt" "$ranked" "$preview_ms" "$cpu_reaped_s" "$cpu_sys_s" "$WG_DIRTY_WB_KB" "$WG_PGPGOUT_PRE_KB" "$WG_PGPGOUT_LAUNCH_KB" "$WG_CLEAN" "$RERUN_N" "$first_rows" "$cpu_main_s" "$cold_pss" "$cold_drm" "$cold_child_pss" "$cold_sample_ms" "$LAUNCH_POS"' "$row_args"

# ---------------------------------------------------------------- report Version column
# Every versioned table names the version beside the app, read off the manifest line only.
ver_csv="$scratch/ver.csv"
{
  printf '%s\n' "$bench_header"
  CELL=([id]=flea [run]=1 [mapped_ms]=754 [settled_ms]=1227 [pss_kb]=105779 [cpu_tree_s]=0.75 [thumbs_n]=36 [thumbs_by_format]="jpg=2;png=0;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=nautilus [run]=1 [mapped_ms]=907 [settled_ms]=2311 [pss_kb]=341791 [cpu_tree_s]=19.26 [thumbs_n]=790 [thumbs_by_format]="jpg=2;png=1;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=strata [run]=1 [mapped_ms]=608 [settled_ms]=3201 [pss_kb]=356233 [cpu_tree_s]=22.92 [thumbs_n]=41 [thumbs_by_format]="jpg=2;png=1;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=yazi [run]=1 [mapped_ms]=112 [settled_ms]=340 [pss_kb]=41200 [cpu_tree_s]=0.44 [thumbs_n]=n/a [thumbs_by_format]=n/a [ranked]="n/a: tui in kitty" [preview_ms]=745)
  emit_row
  CELL=([id]=thunar [run]=1 [mapped_ms]=640 [settled_ms]=1500 [pss_kb]=90000 [cpu_tree_s]=1.10 [thumbs_n]=52 [thumbs_by_format]="jpg=2;png=0;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=nemo [run]=1 [mapped_ms]=700 [settled_ms]=1600 [pss_kb]=95000 [cpu_tree_s]=1.20 [thumbs_n]=48 [thumbs_by_format]="jpg=2;png=0;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=flea-tui [run]=1 [mapped_ms]=120 [settled_ms]=350 [pss_kb]=42000 [cpu_tree_s]=0.50 [thumbs_n]=n/a [thumbs_by_format]=n/a [ranked]="n/a: tui in kitty" [preview_ms]=750)
  emit_row
  CELL=([id]=flea-anchor [run]=1 [mapped_ms]=660 [settled_ms]=1700 [pss_kb]=98000 [cpu_tree_s]=1.30 [thumbs_n]=55 [thumbs_by_format]="jpg=2;png=1;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=dolphin [run]=1 [mapped_ms]=720 [settled_ms]=1800 [pss_kb]=99000 [cpu_tree_s]=1.40 [thumbs_n]=60 [thumbs_by_format]="jpg=2;png=1;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
  CELL=([id]=polo [run]=1 [mapped_ms]=740 [settled_ms]=1900 [pss_kb]=100000 [cpu_tree_s]=1.50 [thumbs_n]=62 [thumbs_by_format]="jpg=2;png=0;txt=0;none=0;unknown=0" [ranked]=yes [preview_ms]=-)
  emit_row
} > "$ver_csv"
ver_man="${ver_csv%.csv}.manifest.md"
sed -e 's/^flea[[:space:]][[:space:]]*gui[[:space:]].*/flea       gui  source 1bd18d9 (clean), release binary built 2026-09-23T00:14:24-04:00, 2706072 bytes/' \
    -e '/^flea[[:space:]][[:space:]]*gui[[:space:]]/a nautilus   gui  nautilus 50.3.1-1' \
    -e '/^dolphin[[:space:]][[:space:]]*gui[[:space:]]/d' -e '/^definitely-not-a-file-manager[[:space:]][[:space:]]*gui[[:space:]]/d' \
    -e 's/^strata[[:space:]][[:space:]]*gui[[:space:]].*/strata     gui  source v0.19.0 committed 2026-09-18, built locally from this checkout/' \
    -e 's/^yazi[[:space:]][[:space:]]*tui[[:space:]].*/yazi       tui  yazi 26.9.1-2/' -e '/^xplr[[:space:]][[:space:]]*tui[[:space:]]/d' \
    "$man" > "$ver_man"
printf '%-10s %-4s %s\n' "thunar" "gui" "NOT INSTALLED" >> "$ver_man"
printf '%-10s %-4s unpackaged at %s, built %s\n' "nemo" "gui" "/usr/bin/nemo" "2026-09-23T00:14:24-04:00" >> "$ver_man"
printf '%-10s %-4s source %s %s (%s), release binary built %s, %s bytes\n' "flea-tui" "tui" "9f2c4a1" "untagged" "DIRTY" "2026-09-23T00:14:24-04:00" "2706072" >> "$ver_man"
printf '%-10s %-4s source %s %s (%s), release binary built %s, %s bytes\n' "flea-anchor" "gui" "absent" "absent" "absent" "absent" "0" >> "$ver_man"
printf '%-10s %-4s source %s %s (%s), release binary built %s, %s bytes\n' "dolphin" "gui" "unreadable" "unreadable" "unreadable" "unreadable" "0" >> "$ver_man"
printf '%-10s %-4s %s\n' "polo" "gui" "polo 2.0-1" >> "$ver_man"
ver_out="$scratch/ver.md"
"$repo/tools/flea-bench-report" "$ver_csv" > "$ver_out" 2>"$scratch/ver.err"
check "the versioned report runs" 0 $?
# Each table is cut at its own header, since the progress table repeats the same cells.
gui_tbl="$scratch/ver-gui.md"
tui_tbl="$scratch/ver-tui.md"
prog_tbl="$scratch/ver-prog.md"
sed -n '/^| entrant | version | first window | settled listing | work done | memory/,/^$/p' "$ver_out" > "$gui_tbl"
sed -n '/^| entrant | version | first window | settled listing | time to first preview/,/^$/p' "$ver_out" > "$tui_tbl"
sed -n '/^| entrant | version | thumbnails | by format/,/^$/p' "$ver_out" > "$prog_tbl"
holds "the GUI table heads its version column" '| entrant | version | first window |' "$gui_tbl"
holds "a packaged entrant's row carries its package version" '`nautilus` | 50.3.1-1 |' "$gui_tbl"
holds "flea's row carries its short source ref" '`flea` | 1bd18d9 |' "$gui_tbl"
holds "strata's row carries its tag" '`strata` | v0.19.0 |' "$gui_tbl"
holds "an absent source reads absent" '`flea-anchor` | absent |' "$gui_tbl"
holds "an unreadable source reads unreadable" '`dolphin` | unreadable |' "$gui_tbl"
holds "an uninstalled package reads NOT INSTALLED" '`thunar` | NOT INSTALLED |' "$gui_tbl"
holds "an unpackaged binary reads unpackaged" '`nemo` | unpackaged |' "$gui_tbl"
holds "a second package version reads whole" '`polo` | 2.0-1 |' "$gui_tbl"
holds "the TUI table heads its version column" '| entrant | version | first window | settled listing | time to first preview |' "$tui_tbl"
holds "the TUI row carries its package version" '`yazi` | 26.9.1-2 |' "$tui_tbl"
holds "a dirty source reads with its mark" '`flea-tui` | 9f2c4a1 (dirty) |' "$tui_tbl"
holds "the progress table heads its version column" '| entrant | version | thumbnails | by format, run 1 |' "$prog_tbl"
holds "and flea's progress row carries it" '`flea` | 1bd18d9 | 36 |' "$prog_tbl"
# A deleted entrant line stops the report at the kind guard, never as a blank cell.
nover_csv="$scratch/nover.csv"
cp "$ver_csv" "$nover_csv"
grep -v '^strata[[:space:]][[:space:]]*gui[[:space:]]' "$ver_man" > "${nover_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$nover_csv" >"$scratch/nover.md" 2>"$scratch/nover.err"
check "a manifest missing one entrant line is refused" 1 $?
holds "and the refusal names the entrant" "the manifest gives no gui or tui kind for entrant 'strata'" "$scratch/nover.err"
# A present but unparseable line dies in short_version with its own message, never at the kind guard.
badver_csv="$scratch/badver.csv"
cp "$ver_csv" "$badver_csv"
sed 's/^strata[[:space:]].*/strata     gui  frobnicate/' "$ver_man" > "${badver_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$badver_csv" >"$scratch/badver.md" 2>"$scratch/badver.err"
check "an unparseable entrant line is refused" 1 $?
holds "and the refusal names the version line" "the manifest version line for entrant 'strata' is unparseable" "$scratch/badver.err"
# A package line with more than one version word is refused with the version line message.
polover_csv="$scratch/polover.csv"
cp "$ver_csv" "$polover_csv"
sed 's/^polo[[:space:]].*/polo       gui  polo 2.0 rc1/' "$ver_man" > "${polover_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$polover_csv" >"$scratch/polover.md" 2>"$scratch/polover.err"
check "a three-word package line is refused" 1 $?
holds "and names the entrant's version line" "the manifest version line for entrant 'polo' is unparseable" "$scratch/polover.err"
# A source line without parentheses is refused rather than read as clean.
noparen_csv="$scratch/noparen.csv"
cp "$ver_csv" "$noparen_csv"
sed 's/^polo[[:space:]].*/polo       gui  source abc1234 untagged clean, release binary built 2026-09-23T00:14:24-04:00, 2706072 bytes/' "$ver_man" > "${noparen_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$noparen_csv" >"$scratch/noparen.md" 2>"$scratch/noparen.err"
check "a source line without parentheses is refused" 1 $?
holds "and names the entrant's version line" "the manifest version line for entrant 'polo' is unparseable" "$scratch/noparen.err"
# A dirty word outside the writer's set is refused rather than printed.
baddirty_csv="$scratch/baddirty.csv"
cp "$ver_csv" "$baddirty_csv"
sed 's/^polo[[:space:]].*/polo       gui  source abc1234 untagged (STALE), release binary built 2026-09-23T00:14:24-04:00, 2706072 bytes/' "$ver_man" > "${baddirty_csv%.csv}.manifest.md"
"$repo/tools/flea-bench-report" "$baddirty_csv" >"$scratch/baddirty.md" 2>"$scratch/baddirty.err"
check "an unknown dirty word is refused" 1 $?
holds "and names the entrant's version line" "the manifest version line for entrant 'polo' is unparseable" "$scratch/baddirty.err"

# The EXIT trap on a clean run: every teardown step already ran once, so each helper stays 0 with nothing parked.
cleanup_home="$scratch/cleanup-home"
mkdir -p "$cleanup_home/.cache/flea" "$cleanup_home/.cache/thumbnails"
cleanup_out=$((
  HOME="$cleanup_home"
  fail() { echo "refused: $*"; exit 3; }
  eval "$(sed -n '/^token_pids() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^sweep_token() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^pids_for() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^kill_entrant() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^clean_bench() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^close_entrant_dialogs() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^cg_empty() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^sweep_cg() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^release_run_cg() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^restore_thumbs() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^restore_prefetch() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^prefetch_keep() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^watch_kill() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^safe_rm_tmpdir() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^stop_warm_job() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^bench_cleanup() {/,/^}/p' "$bench")"
  BENCH_TOKEN="bench-clean-$$-$RANDOM"
  SWEEP_TERM_POLLS=20 SWEEP_KILL_POLLS=20
  KILL_TARGETS=() ENTRANTS=() WATCH_PIDS=()
  ROWS_PID="" ENTRANT_LAUNCHED=yes RUN_CG="" ENTRANT_SCOPE=""
  BENCH_CG="$scratch/cg"
  THUMBS="$HOME/.cache/thumbnails" THUMBS_PARKED="$scratch/thumb-parked" THUMBS_PARKED_OK=no
  PREFETCH="$HOME/.cache/flea/prefetch" PREFETCH_PARKED="$scratch/prefetch-parked" PREFETCH_PARKED_OK=no
  PREFETCH_ARM="" PERF_TMP_MARK=.flea-perf-tmp
  TUI_CONF="$scratch/clean-tui" && mkdir -p "$TUI_CONF"
  BENCH_TMP_ROOT="$scratch"
  FLEA_STATE=$(mktemp -d "$BENCH_TMP_ROOT/flea-bench-state.XXXXXX") || exit 3
  ANCHOR_STATE=$(mktemp -d "$BENCH_TMP_ROOT/flea-bench-state.XXXXXX") || exit 3
  trap 'rm -rf "$FLEA_STATE" "$ANCHOR_STATE"' EXIT
  printf 'owner=%s\n' "$$" > "$TUI_CONF/$PERF_TMP_MARK" || exit 3
  printf 'owner=%s\n' "$$" > "$FLEA_STATE/$PERF_TMP_MARK" || exit 3
  printf 'owner=%s\n' "$$" > "$ANCHOR_STATE/$PERF_TMP_MARK" || exit 3
  restore_thumbs; [ $? -eq 0 ] || exit 3
  restore_thumbs; [ $? -eq 0 ] || exit 3
  restore_prefetch; [ $? -eq 0 ] || exit 3
  restore_prefetch; [ $? -eq 0 ] || exit 3
  release_run_cg; [ $? -eq 0 ] || exit 3
  release_run_cg; [ $? -eq 0 ] || exit 3
  THUMBS_PARKED_OK=yes; restore_thumbs; [ $? -eq 0 ] || exit 3
  restore_thumbs; [ $? -eq 0 ] || exit 3
  PREFETCH_PARKED_OK=yes; restore_prefetch; [ $? -eq 0 ] || exit 3
  restore_prefetch; [ $? -eq 0 ] || exit 3
  true
  ( bench_cleanup )
) 2>&1)
cleanup_rc=$?
[ "$cleanup_rc" -eq 0 ] || { printf '%s\n' "$cleanup_out"; }
check "a repeated teardown stays 0 and the clean-run trap exits 0" 0 "$cleanup_rc"

# safe_rm_tmpdir removes only a directory this run made; symlink, foreign and missing markers refuse.
tmpdir_out=$((
  fail() { echo "refused: $*"; exit 3; }
  eval "$(sed -n '/^safe_rm_tmpdir() {/,/^}/p' "$bench")"
  eval "$(sed -n '/^materialize_tui_socket() {/,/^}/p' "$bench")"
  PERF_TMP_MARK=.flea-perf-tmp
  happy="$scratch/tmp-happy" && mkdir -p "$happy"
  printf 'owner=%s\n' "$$" > "$happy/$PERF_TMP_MARK"
  safe_rm_tmpdir "$happy" "TUI config" || exit 3
  [ -e "$happy" ] && exit 3
  target="$scratch/tmp-target" && mkdir -p "$target" && printf 'data' > "$target/f"
  link="$scratch/tmp-link" && ln -s "$target" "$link"
  safe_rm_tmpdir "$link" "TUI config" 2>/dev/null && exit 3
  [ -f "$target/f" ] || exit 3
  [ -L "$link" ] || exit 3
  foreign="$scratch/tmp-foreign" && mkdir -p "$foreign"
  printf 'owner=%s\n' "99999999" > "$foreign/$PERF_TMP_MARK"
  safe_rm_tmpdir "$foreign" "TUI config" 2>/dev/null && exit 3
  [ -d "$foreign" ] || exit 3
  bare="$scratch/tmp-bare" && mkdir -p "$bare"
  safe_rm_tmpdir "$bare" "TUI config" 2>/dev/null && exit 3
  [ -d "$bare" ] || exit 3
  marklink="$scratch/tmp-marklink" && mkdir -p "$marklink"
  printf 'owner=%s\n' "$$" > "$scratch/real-marker"
  ln -s "$scratch/real-marker" "$marklink/$PERF_TMP_MARK"
  safe_rm_tmpdir "$marklink" "TUI config" 2>/dev/null && exit 3
  [ -d "$marklink" ] || exit 3
  TUI_CONF="$scratch/tmp-tuiconf" && mkdir -p "$TUI_CONF/kitty"
  printf 'listen_on unix:/tmp/flea-bench-kitty\n' > "$TUI_CONF/kitty/kitty.conf"
  materialize_tui_socket || exit 3
  grep -q "unix:$TUI_CONF/flea-bench-kitty" "$TUI_CONF/kitty/kitty.conf" || exit 3
  grep -q "unix:/tmp/flea-bench-kitty" "$TUI_CONF/kitty/kitty.conf" && exit 3
  TUI_CONF="$scratch/tmp-noconf" && mkdir -p "$TUI_CONF"
  materialize_tui_socket || exit 3
) 2>&1)
tmpdir_rc=$?
[ "$tmpdir_rc" -eq 0 ] || { printf '%s\n' "$tmpdir_out"; }
check "a symlinked dir, a foreign or missing marker, and the TUI socket behave" 0 "$tmpdir_rc"

# The selftest log stays on disk so a red check still names its labels.
"$repo/tools/flea-bench-readme" selftest >"$scratch/readme-selftest.log" 2>&1
selftest_rc=$?
if [ "$selftest_rc" -ne 0 ]; then cat "$scratch/readme-selftest.log"; fi
check "the readme section generator selftest passes" 0 $selftest_rc

exit $fail
