#!/usr/bin/env python3
"""Replays tools/flea-field-bench's own run_one on stubbed launches and checks which media rows may rank.

Only the process, clock, window and memory readers are stubbed; the watch, cache, format, helper and
CPU code is the bench's own, cut out of the file by name. Each check also runs against a mutant of
the bench with one refusal removed, and that mutant must fail it, so a check that cannot fail is caught.
FIELD_WORK_BENCH=<path> replays another copy of the bench, for a before and after comparison.
"""
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BENCH = Path(os.environ.get("FIELD_WORK_BENCH", REPO / "tools" / "flea-field-bench"))
KEYS_TOOL = REPO / "tools" / "flea-bench-keys"
REAL = ("run_one", "watch_stop", "watch_kill", "watch_classify", "known_formats", "count_thumbs",
        "capture_keys", "settle_ticks", "find_helper", "cgroup_usage_usec", "mount_ticks_start",
        "mount_ticks_delta", "sys_seconds", "write_guard_ranked")
FIXTURE_FILES = ("a.jpg", "b.png", "c.mp4")
EMPTY_WORK = "unranked: no thumbnail work was measured"
UNCLASSIFIED = "unclassified"

STUBS = r'''
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
NOW_MS=1000
now_ms() { NOW_MS=$(( NOW_MS + 20 )); }
sleep() { :; }
clean_bench() { :; }
prefetch_for() { :; }
drop_caches() { :; }
release_run_cg() { :; }
measure_preview() { PREVIEW_MS=-; }
net_rx_bytes() { echo 0; }
drm_kb() { DRM_KB=0; }
pss_kb() { PSS_KB=100; PSS_ANON_KB=50; USS_KB=80; }
descendant_ticks() { DESC_TICKS=0; }
write_guard_pre() { WG_DIRTY_WB_KB=0; WG_PGPGOUT_PRE_KB=0; }
write_guard_launch_end() { WG_PGPGOUT_LAUNCH_KB=0; WG_CLEAN=1; }
entrant_cg() { ENTRANT_CG=$RUN_CG; }
wait_for_pid() { ENTRANT_PID=$ENTRANT_FAKE; }
wait_for_window() {
  NOW_MS=$(( NOW_MS + 100 ))
  printf 'usage_usec 1000000\n' > "$RUN_CG/cpu.stat"
  WINDOW_PID=$ENTRANT_FAKE
  [ "$SC_MAP" = yes ]
}
# The renders and opens a transient entrant's two watches would have seen, from the scenario's own files.
watch_start() {
  local id=$1
  WATCH_RENDERS="$KEYS_DIR/$id-run$RUN_INDEX.renders"
  WATCH_OPENS="$KEYS_DIR/$id-run$RUN_INDEX.opens"
  WATCH_RENDERED="$KEYS_DIR/$id-run$RUN_INDEX.rendered"
  cp -- "$SC_DIR/renders" "$WATCH_RENDERS"
  cp -- "$SC_DIR/opens" "$WATCH_OPENS"
  command sleep 600 & WATCH_PIDS=("$!")
  command sleep 600 & WATCH_PIDS+=("$!")
}
# The cache a cached entrant leaves, written where count_thumbs and capture_keys read it.
drop_thumbs() {
  [ "$DROP_THUMBS" = yes ] || return 0
  case $THUMBS in "$ROOT"/thumbnails) ;; *) fail "thumbnail cache $THUMBS is outside the marked root" ;; esac
  [ -e "$ROOT/.flea-test-sandbox" ] || fail "the root has no sandbox marker"
  rm -rf -- "$THUMBS"
  mkdir -p "$THUMBS/large" "$THUMBS/fail"
  cp -- "$SC_DIR"/cache/* "$THUMBS/large/" 2>/dev/null || :
}
declare -A CALLS=()
cpu_ticks() {
  local pid=$1 n
  n=$(( ${CALLS[$pid]:-0} + 1 )); CALLS[$pid]=$n
  if [ "$pid" = "$ENTRANT_FAKE" ]; then
    [ -n "$SC_DIE_AT" ] && [ "$n" -ge "$SC_DIE_AT" ] && return 1
    if [ -n "$HELPER_A" ] && [ "$n" = "$SC_KILL_A" ]; then kill "$HELPER_A"; wait "$HELPER_A" 2>/dev/null; fi
    CPU_TICKS=$(( n < SC_BUSY ? n : SC_BUSY )); CPU_TREE_TICKS=$CPU_TICKS
    return 0
  fi
  # Helper A burns 30 ticks before it exits and its replacement B burns 20.
  if [ "$pid" = "$HELPER_A" ]; then CPU_TICKS=$(( n < 3 ? n * 10 : 30 )); CPU_TREE_TICKS=$CPU_TICKS; return 0; fi
  if [ "$pid" = "$HELPER_B" ]; then CPU_TICKS=$(( n < 2 ? n * 10 : 20 )); CPU_TREE_TICKS=$CPU_TICKS; return 0; fi
  return 1
}
pids_for() {
  PIDS=()
  if [ -n "$HELPER_A" ] && [ -r "/proc/$HELPER_A/stat" ]; then PIDS=("$HELPER_A"); elif [ -n "$HELPER_B" ]; then PIDS=("$HELPER_B"); fi
}
'''

DRIVER = r'''
set -u
ROOT=$1 SC_DIR=$2
. "$SC_DIR/env"
DROP_THUMBS=${SC_DROP:-yes} FIELD_PROFILE=field RUN_MODE=measure RERUN_N=0 RUN_INDEX=1 LAUNCH_POS=1
BENCH_CG="$ROOT/cg" LAUNCH_TIMEOUT_MS=${SC_TIMEOUT:-100000} SETTLE_GAP_MS=200 POLL_SLEEP=0
KEYS_DIR="$ROOT/keys" KEY_MAP="$ROOT/keys/fixture.map" THUMBS="$ROOT/thumbnails" FIXTURE="$ROOT/fixture"
HARNESS_DIR=$HARNESS_DIR BENCH_TOKEN=t FLEA_PREFETCH_SLOT=s ANCHOR_PREFETCH_SLOT=s
NET_IFACE="" MOUNT_PIDS=() TUI_TERM=kitty TUI_CLASS=c TUI_CONF="$ROOT" TUI_CONF_FS=tmpfs TUI_DUMP="$ROOT/dump" FIRST_ROWS_PROBE=-
HELPER_SEARCH_POLLS=25 ENTRANT_FAKE=999999999 ENTRANT_SCOPE="" THUMBS_PARKED_OK=yes
HELPER_A="" HELPER_B="" KEY_MAP_OK=$SC_KEYMAP
if [ "$SC_HELPER" = yes ]; then command sleep 600 & HELPER_A=$!; command sleep 600 & HELPER_B=$!; fi
run_one "$SC_ID" gui "$SC_ID.class" "$SC_ID" "" "$SC_HELPER_COMM" "" :
rc=$?
# A mutant that skips watch_stop leaves its watches running, and they hold the replay's output open.
watch_kill
for pid in $HELPER_A $HELPER_B; do kill "$pid" 2>/dev/null; done
exit $rc
'''


def functions(text):
    # The bench's own transient defaults, so the replay names the same entrant the bench does.
    out = re.findall(r"^TRANSIENT_(?:ID|PREFIX)=.*\n", text, re.M)
    for name in REAL:
        m = re.search(r"^%s\(\) \{\n.*?^\}\n" % re.escape(name), text, re.S | re.M)
        if m is None:
            if name == "known_formats":
                continue
            raise SystemExit(f"FAIL: no {name} in {BENCH}")
        out.append(m.group(0))
    return "\n".join(out)


def header(text):
    m = re.search(r'^echo "(id,run,[^"]*)"', text, re.M)
    if m is None:
        raise SystemExit(f"FAIL: no CSV header in {BENCH}")
    return m.group(1).split(",")


def fixture_key(root, name):
    return hashlib.md5(f"file://{root}/fixture/{name}".encode()).hexdigest()


class Scenario:
    def __init__(self, label, entrant, renders=0, opens=(), cache=(), foreign=0, drop="yes",
                 helper=False, mapped=True, die_at="", timeout=None, busy=40, keymap=True):
        self.__dict__.update(locals())


def replay(source, scen, base):
    root = Path(tempfile.mkdtemp(prefix="sc.", dir=base))
    (root / ".flea-test-sandbox").write_text("")
    for d in ("cg", "keys", "fixture", "sc/cache"):
        (root / d).mkdir(parents=True)
    for name in FIXTURE_FILES:
        (root / "fixture" / name).write_text("")
    if scen.keymap:
        subprocess.run([str(KEYS_TOOL), "map", str(root / "fixture"), str(root / "keys/fixture.map")], check=True)
    sc = root / "sc"
    (sc / "renders").write_text("".join(f"/tmp/strata-preview-4242-{i}/result.png\n" for i in range(scen.renders)))
    (sc / "opens").write_text("".join(f"{n}\n" for n in scen.opens))
    for name in scen.cache:
        (sc / "cache" / f"{fixture_key(root, name)}.png").write_text("")
    for i in range(scen.foreign):
        (sc / "cache" / f"{hashlib.md5(f'foreign{i}'.encode()).hexdigest()}.png").write_text("")
    env = {"SC_ID": scen.entrant, "SC_DROP": scen.drop, "SC_HELPER": "yes" if scen.helper else "no",
           "SC_HELPER_COMM": "tumblerd" if scen.helper else "", "SC_MAP": "yes" if scen.mapped else "no",
           "SC_DIE_AT": str(scen.die_at), "SC_BUSY": str(scen.busy), "SC_KILL_A": "8",
           "SC_TIMEOUT": str(scen.timeout or 100000), "SC_KEYMAP": "yes" if scen.keymap else "no"}
    (sc / "env").write_text("".join(f"{k}={v}\n" for k, v in env.items()))
    script = root / "driver.sh"
    script.write_text(functions(source) + STUBS + DRIVER)
    try:
        run = subprocess.run(["bash", str(script), str(root), str(sc)], capture_output=True, text=True,
                             stdin=subprocess.DEVNULL, timeout=60, start_new_session=True,
                             env={"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LC_ALL": "C.UTF-8",
                                  "HARNESS_DIR": str(KEYS_TOOL.parent)})
    except subprocess.TimeoutExpired:
        return {"_rc": "timeout", "_err": "the replay ran past 60 s"}
    rows = [line for line in run.stdout.splitlines() if line.startswith(scen.entrant + ",")]
    names = header(source)
    if run.returncode != 0 or not rows:
        return {"_rc": str(run.returncode), "_err": run.stderr.strip()[-300:]}
    cells = rows[-1].split(",")
    row = dict(zip(names, cells))
    row["_fields"] = str(len(cells))
    return row


def scenarios():
    full = ("a.jpg", "b.png", "c.mp4")
    yield Scenario("an empty transient watch on strata-stable", "strata-stable"), \
        {"ranked": EMPTY_WORK, "thumbs_n": "unmeasurable"}
    yield Scenario("an empty transient watch on strata-nightly", "strata-nightly"), \
        {"ranked": EMPTY_WORK, "thumbs_n": "unmeasurable"}
    yield Scenario("an empty transient watch on strata-nightly-channel", "strata-nightly-channel"), \
        {"ranked": EMPTY_WORK, "thumbs_n": "unmeasurable"}
    yield Scenario("one stray fixture open and no render", "strata-nightly", opens=("a.jpg",)), \
        {"ranked": EMPTY_WORK, "thumbs_n": "unmeasurable"}
    yield Scenario("renders the fixture watch never saw opened", "strata-stable", renders=3), \
        {"ranked": "unranked: its watches saw 3 renders for 0 opened fixture files so neither is a count",
         "thumbs_n": "unmeasurable"}
    yield Scenario("complete transient work", "strata-stable", renders=3, opens=full), \
        {"ranked": "yes", "thumbs_n": "3", "thumbs_by_format": "jpg=1;mp4=1;png=1;unknown=0"}
    yield Scenario("transient work on one format of three", "strata-nightly", renders=2, opens=("a.jpg", "a.jpg")), \
        {"ranked": "yes", "thumbs_n": "2", "thumbs_by_format": "jpg=2;mp4=0;png=0;unknown=0"}
    yield Scenario("an empty cache on a cached entrant", "dolphin"), \
        {"ranked": EMPTY_WORK, "thumbs_n": "unmeasurable"}
    yield Scenario("complete cached work", "dolphin", cache=full), \
        {"ranked": "yes", "thumbs_n": "3", "thumbs_by_format": "jpg=1;mp4=1;png=1;unknown=0"}
    yield Scenario("cached work on one format of three", "nautilus", cache=("b.png",)), \
        {"ranked": "yes", "thumbs_n": "1", "thumbs_by_format": "jpg=0;mp4=0;png=1;unknown=0"}
    yield Scenario("a cache holding no fixture file", "thunar", foreign=2), \
        {"ranked": "unranked: none of its thumbnails were of a fixture file", "thumbs_n": "2",
         "thumbs_by_format": "jpg=0;mp4=0;png=0;unknown=2"}
    yield Scenario("the synthetic fixture, no thumbnail drop", "dolphin", drop="no"), \
        {"ranked": "yes", "thumbs_n": "n/a", "thumbs_by_format": "-"}
    yield Scenario("media work by an entrant that died before settling", "strata-stable", renders=3, opens=full,
                   die_at=10), \
        {"ranked": "unranked: it never settled so its media work did not finish", "settled_ms": "-1"}
    yield Scenario("media work that never settled by the deadline", "dolphin", cache=full, timeout=1500,
                   busy=10**9), \
        {"ranked": "unranked: it never settled so its media work did not finish", "settled_ms": "-1"}
    yield Scenario("media work whose window never mapped", "nemo", cache=full, mapped=False), \
        {"ranked": "unranked: its window never mapped so its media work did not finish", "mapped_ms": "-1"}
    yield Scenario("a never-settled scale run keeps its old verdict", "dolphin", drop="no", die_at=10), \
        {"ranked": "yes", "settled_ms": "-1"}
    yield Scenario("an outside helper replaced mid-run keeps both lifetimes", "thunar", cache=full, helper=True,
                   busy=80), \
        {"ranked": "yes", "cpu_tree_s": "1.50", "cpu_s": "1.50", "cpu_main_s": "0.80", "_fields": "30"}
    # A nas or usb run tolerates a key map that did not build; its rows say unclassified, never "no fixture file".
    yield Scenario("cached work with no fixture key map", "dolphin", cache=full, keymap=False), \
        {"ranked": "yes", "thumbs_n": "3", "thumbs_by_format": UNCLASSIFIED}
    yield Scenario("transient work with no fixture key map", "strata-stable", renders=3, opens=full, keymap=False), \
        {"ranked": "yes", "thumbs_n": "3", "thumbs_by_format": UNCLASSIFIED}
    yield Scenario("no helper adds nothing to the leaf", "dolphin", cache=full, busy=80), \
        {"ranked": "yes", "cpu_tree_s": "1.00", "cpu_main_s": "0.80"}


# Each mutant removes one refusal from the bench; the label is a scenario that must catch it.
MUTANTS = [
    ("the zero-work refusal", r'elif \[ "\$thumbs_n" = 0 \]; then', 'elif false; then',
     "an empty transient watch on strata-nightly"),
    ("the transient watch count", r'    if \[ "\$id" = "\$TRANSIENT_ID" \] \|\| \[\[ \$id = strata-nightly\* \]\]; then',
     '    if false; then', "complete transient work"),
    ("the fixture-file refusal", r'\[ "\$\(known_formats "\$thumbs_fmt"\)" = 0 \]', 'false',
     "a cache holding no fixture file"),
    ("the unclassified format without a key map",
     r'\[ "\$\{KEY_MAP_OK:-yes\}" = yes \] \|\| \[ "\$thumbs_fmt" = unmeasurable \] \|\| thumbs_fmt=unclassified', ':',
     "cached work with no fixture key map"),
    ("the per-device client key", r'key="\$device:\$id"', 'key="$id"', "drm"),
    ("the GiB unit", r' GiB\) amount=\$\(\( amount \* 1048576 \)\) ;;', ' GiB) ;;', "drm"),
    ("the unit-less byte reading", r'case \$unit in "" *\) amount=\$\(\( \(amount \+ 1023\) / 1024 \)\) ;; ', 'case $unit in ', "drm"),
    ("the NVIDIA driver reading", r"-lname '/dev/nvidia\[0-9\]\*'", "-lname '/dev/never'", "drm"),
    ("the unlisted NVIDIA refusal", r'\{ DRM_KB=unmeasurable; return 0; \}', '{ return 0; }', "drm"),
    ("the NVIDIA GPU-node match", r"'/dev/nvidia\[0-9\]\*'", "'/dev/nvidia*'", "drm"),
    ("the unfinished-work refusal", r'if \[ "\$settled" -lt 0 \]', 'if false', None),
    ("the helper lifetime", r'HELPER_TICKS=\$\(\( \$\{HELPER_DONE_TICKS:-0\} \+ CPU_TREE_TICKS \)\)',
     'HELPER_TICKS=$CPU_TREE_TICKS', "an outside helper replaced mid-run keeps both lifetimes"),
]


def check(source, base, only=None, quiet=False):
    failures = []
    for scen, want in scenarios():
        if only and scen.label != only:
            continue
        row = replay(source, scen, base)
        got = {k: row.get(k, f"<missing; rc {row.get('_rc')} {row.get('_err', '')}>") for k in want}
        ok = got == want
        if not quiet:
            print(f"{'ok  ' if ok else 'FAIL'} {scen.label}")
            if not ok:
                print(f"  expected: {want}\n  actual:   {got}")
        if not ok:
            failures.append(scen.label)
    return failures


def drm_readings(source, base, quiet=False):
    """drm_kb over fake fdinfo: per-device clients, kernel units (none means bytes), duplicates and zero."""
    m = re.search(r"^drm_kb\(\) \{\n.*?^\}\n", source, re.S | re.M)
    if m is None:
        print("FAIL no drm_kb in the bench")
        return ["drm_kb"]
    root = Path(tempfile.mkdtemp(prefix="drm.", dir=base))
    fds = {
        "1": {"3": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t1\ndrm-resident-system0:\t2 MiB\n",
              "4": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t1\ndrm-resident-system0:\t2 MiB\n",
              "5": "drm-pdev:\t0000:03:00.0\ndrm-client-id:\t1\ndrm-resident-system0:\t512 KiB\n",
              "6": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t2\ndrm-resident-system0:\t0\n",
              "7": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t3\ndrm-resident-system0:\t1536\n",
              "8": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t4\ndrm-resident-system0:\t1 GiB\n",
              "9": "pos:\t0\nflags:\t02\n"},
        "2": {"3": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t9\ndrm-resident-system0:\t0\n"},
        "3": {"3": "pos:\t0\n"},
        # NVIDIA: an idle i915 client beside /dev/nvidia0; the driver names 68 + 2 MiB on two GPUs.
        "4": {"3": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-resident-system0:\t0\n"},
        # On the NVIDIA device but absent from the driver's list: no honest total.
        "5": {"3": "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t6\ndrm-resident-system0:\t0\n"},
    }
    for pid, files in fds.items():
        (root / pid / "fdinfo").mkdir(parents=True)
        (root / pid / "fd").mkdir()
        for fd, text in files.items():
            (root / pid / "fdinfo" / fd).write_text(text)
    for pid in ("4", "5"):
        (root / pid / "fd" / "7").symlink_to("/dev/nvidia0")
    # The control: pid 1 holds /dev/nvidiactl only, which is not a GPU node and must not reach the driver query.
    (root / "1" / "fd" / "8").symlink_to("/dev/nvidiactl")
    smi = root / "nvidia-smi"
    smi.write_text("#!/bin/sh\nprintf '4, 68\\n77, 5\\n4, 2\\n1, 999\\n'\n")
    smi.chmod(0o755)
    script = m.group(0) + 'for pid in 1 2 3 4 5; do drm_kb "$pid"; printf "%s " "$DRM_KB"; done\n'
    run = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=30,
                         env={"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "DRM_PROC_ROOT": str(root),
                              "NVIDIA_SMI": str(smi)})
    want = ["1051138", "0", "unmeasurable", "71680", "unmeasurable"]
    ok = run.returncode == 0 and run.stdout.split() == want
    if not quiet:
        print(f"{'ok  ' if ok else 'FAIL'} drm_kb sums each device's clients once, in kernel units, keeps a zero, and reads NVIDIA from its driver")
        if not ok:
            print(f"  expected {want} got {run.stdout.split()} rc {run.returncode} {run.stderr.strip()[-200:]}")
    return [] if ok else ["drm readings"]


def capability_rows(source):
    """Every Strata row the field roster can name is a live-watch row in the capability pass too."""
    cap = (REPO / "tools" / "flea-bench-capability").read_text()
    # The stable row's id comes from a variable in the roster, so it is named here.
    ids = sorted(set(re.findall(r'"(strata[-a-z]*)\|gui\|', source)) | {"strata-stable"})
    funcs = ""
    for name in ("cap_is_transient", "select_process_targets"):
        m = re.search(r"^%s\(\) \{\n.*?^\}\n" % name, cap, re.S | re.M)
        funcs += m.group(0) if m else ""
    script = funcs + r'''
TRANSIENT_ID=strata
for id in "$@"; do
  CAP_ONLY=$id
  select_process_targets
  if cap_is_transient "$id"; then t=transient; else t=cached; fi
  printf '%s %s %s\n' "$id" "$t" "${PROCESS_TARGETS[*]}"
done
'''
    run = subprocess.run(["bash", "-c", script, "cap", *ids], capture_output=True, text=True, timeout=30)
    got = run.stdout.splitlines()
    # The per-row dispatch and the cache park run only on a display, so both must ask the same helper.
    sites = {"dispatch": r'^  if cap_is_transient "\$id"; then\n    cap_transient "\$id" "\$cmd"$',
             "park": r'^if \[ -z "\$CAP_ONLY" \] \|\| ! cap_is_transient "\$CAP_ONLY"; then\n  park_thumbs$'}
    for site, pattern in sites.items():
        if not re.search(pattern, cap, re.M):
            got.append(f"the {site} site does not ask cap_is_transient")
    want = [f"{i} transient strata|" for i in ids]
    ok = run.returncode == 0 and "strata-nightly-channel" in ids and got == want
    print(f"{'ok  ' if ok else 'FAIL'} the capability pass watches every Strata row of the roster live ({' '.join(ids)})")
    if not ok:
        print(f"  expected {want}\n  got      {got} rc {run.returncode} {run.stderr.strip()[-200:]}")
    return [] if ok else ["capability rows"]


def engine_sample(source):
    """The engine profile's cold sample fills COLD_* only; the post-settle readings the row prints stay as read."""
    m = re.search(r"^engine_memory_sample\(\) \{\n.*?^\}\n", source, re.S | re.M)
    if m is None:
        print("FAIL no engine_memory_sample in the bench")
        return ["engine_memory_sample"]
    script = m.group(0) + r'''
fail() { echo "FAIL: $*" >&2; exit 1; }
drm_kb() { DRM_KB=777; }
pss_kb() { PSS_KB=900; PSS_ANON_KB=901; USS_KB=902; }
HELPER_COMM=""
DRM_KB=5 PSS_KB=10 PSS_ANON_KB=11 USS_KB=12
engine_memory_sample 4242
echo "$DRM_KB $PSS_KB $PSS_ANON_KB $USS_KB $COLD_DRM $COLD_PSS"
'''
    run = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=30)
    ok = run.returncode == 0 and run.stdout.split() == ["5", "10", "11", "12", "777", "900"]
    print(f"{'ok  ' if ok else 'FAIL'} the engine cold sample leaves the post-settle drm and pss readings alone")
    if not ok:
        print(f"  got {run.stdout.strip()!r} rc {run.returncode} {run.stderr.strip()[-200:]}")
    return [] if ok else ["engine sample"]


def companions(base):
    """A bench copied without the sibling tool a mode needs refuses that mode at once, naming the tool."""
    failures = []
    alone = Path(tempfile.mkdtemp(prefix="alone.", dir=base))
    shutil.copy(BENCH, alone / "flea-field-bench")
    gate = alone / "flea-bench-gate"
    # Nothing past the refusal may run: a stub scope launcher and sudo stop any bench that misses it.
    stubs = alone / "stubs"
    stubs.mkdir()
    for name in ("systemd-run", "sudo"):
        (stubs / name).write_text("#!/bin/sh\necho 'stub reached: no refusal before the scope' >&2\nexit 97\n")
        (stubs / name).chmod(0o755)
    path = str(stubs) + ":" + os.environ.get("PATH", "/usr/bin:/bin")
    cases = [("--thumbs without flea-bench-thumbs", ["--thumbs", "x"], {}, "needs flea-bench-thumbs"),
             ("the engine profile without flea-bench-gate", [], {"FIELD_PROFILE": "engine"}, "needs flea-bench-gate")]
    for label, args, extra, wanted in cases:
        env = {"PATH": path, "LC_ALL": "C.UTF-8", "FLEA_REPO": str(alone), **extra}
        run = subprocess.run(["bash", str(alone / "flea-field-bench"), *args], capture_output=True, text=True,
                             stdin=subprocess.DEVNULL, timeout=60, start_new_session=True, env=env)
        ok = run.returncode != 0 and wanted in run.stderr
        print(f"{'ok  ' if ok else 'FAIL'} {label} is refused naming {wanted}")
        if not ok:
            print(f"  rc {run.returncode}: {run.stderr.strip()[-300:]}")
            failures.append(label)
    # A gate without map-event is the old gate, and the engine profile cannot arm its reader with it.
    gate.write_text("#!/usr/bin/env python3\nimport sys\nsys.exit(2)\n")
    gate.chmod(0o755)
    env = {"PATH": path, "LC_ALL": "C.UTF-8", "FLEA_REPO": str(alone), "FIELD_PROFILE": "engine"}
    run = subprocess.run(["bash", str(alone / "flea-field-bench")], capture_output=True, text=True,
                         stdin=subprocess.DEVNULL, timeout=60, start_new_session=True, env=env)
    ok = run.returncode != 0 and "map-event" in run.stderr
    print(f"{'ok  ' if ok else 'FAIL'} the engine profile with a gate lacking map-event is refused")
    if not ok:
        failures.append("gate without map-event")
    return failures


def main():
    source = BENCH.read_text()
    names = header(source)
    base = Path(tempfile.mkdtemp(prefix="bench-field-work."))
    if not base.is_absolute() or len(base.parts) < 3:
        raise SystemExit(f"FAIL: mktemp gave {base}")
    (base / ".flea-test-sandbox").write_text("")
    try:
        fail = 0
        print(f"{'ok  ' if len(names) == 30 else 'FAIL'} the CSV header keeps its 30 columns")
        fail |= len(names) != 30
        fail |= bool(check(source, base))
        fail |= bool(companions(base))
        fail |= bool(engine_sample(source))
        fail |= bool(capability_rows(source))
        fail |= bool(drm_readings(source, base))
        if "FIELD_WORK_BENCH" not in os.environ:
            for label, pattern, repl, catcher in MUTANTS:
                mutant, n = re.subn(pattern, repl, source)
                if n == 0:
                    print(f"FAIL mutant {label}: its pattern is not in the bench")
                    fail = 1
                    continue
                caught = (drm_readings(mutant, base, quiet=True) if catcher == "drm" else
                          check(mutant, base, only=catcher, quiet=True))
                print(f"{'ok  ' if caught else 'FAIL'} a bench without {label} fails {len(caught)} check(s)")
                fail |= not caught
        return 1 if fail else 0
    finally:
        if (base / ".flea-test-sandbox").exists() and base.is_absolute() and len(base.parts) >= 3:
            shutil.rmtree(base)


if __name__ == "__main__":
    sys.exit(main())
