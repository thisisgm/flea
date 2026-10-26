# Execute the figure suite's real shell fragments with controlled lower-layer failures.
import json
import os
import pathlib
import re
import selectors
import signal
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

tree = pathlib.Path(__file__).resolve().parents[1]
script = (tree / "tests/markdown-figures.sh").read_text()
checks = 0
failures = 0
# Every isolated shell fragment and fixture engine has a short termination bound.
FRAGMENT_BOUND_SECONDS = 10
HEADER_COMMENT_MAX_CHARS = 140


def check(passed, label):
    global checks, failures
    checks += 1
    failures += not passed
    print(("PASS " if passed else "FAIL ") + label)


def section(start, end):
    return script.split(start, 1)[1].split(end, 1)[0]


with tempfile.TemporaryDirectory(prefix="flea-figure-harness-") as scratch:
    box = pathlib.Path(scratch)
    binary = box / "flea"
    binary.write_text('''#!/bin/sh
if [ "$FIG_MODE" = silent ]; then
    exit 127
fi
if [ "$FIG_MODE" = wrong ]; then
    echo "flea: unrelated refusal" >&2
    exit 127
fi
if [ "$FIG_MODE" = namespace ]; then
    echo "bwrap: No permissions to create new namespace" >&2
    exit 1
fi
if [ "$FIG_MODE" = broken ]; then
    echo "figure renderer exploded" >&2
    exit 1
fi
if [ "$FIG_MODE" = empty ]; then
    exit 0
fi
if [ "$FIG_MODE" = good ]; then
    echo '{"id":1,"svg":"<svg/>"}'
    exit 0
fi
if ! command -v bwrap >/dev/null || ! command -v prlimit >/dev/null; then
    echo "flea: the figure helper needs bwrap and prlimit, and one of them is missing" >&2
    exit 127
fi
echo "flea: the figure helper needs quickjs-ng at $FLEA_QJS, and it is missing" >&2
exit 127
''')
    binary.chmod(0o755)
    qjs = box / "qjs"
    qjs.write_text("#!/bin/sh\nprintf 'selected direct engine\\n'\n")
    qjs.chmod(0o755)
    tools = box / "tools"
    tools.mkdir()
    for name in ("bwrap", "prlimit"):
        (tools / name).write_text("#!/bin/sh\nexit 0\n")
        (tools / name).chmod(0o755)
    for name in ("mkdir", "ln", "cmp", "wc"):
        (tools / name).symlink_to(shutil.which(name))
    (box / "empty-path").mkdir()

    def run(body, mode="refusal", **extra):
        env = dict(os.environ, FIG_MODE=mode, test_root=str(box), fleabin=str(binary), qjs=str(qjs), **extra)
        return subprocess.run(["/bin/bash", "-uc", body], cwd=tree, env=env,
                              capture_output=True, text=True, timeout=FRAGMENT_BOUND_SECONDS)

    ui_setup = section('cd "$(dirname "$0")/.." || exit 1\n', '\nfleabin=')
    exported_ui = '\n/bin/sh -c \'printf "%s\\n" "$FLEA_UI"\''
    result = run(ui_setup + exported_ui, FLEA_UI="")
    check(result.returncode == 0 and result.stdout.strip() == str(tree / "ui"),
          "PSS fix suite exports the candidate UI by default")
    chosen_ui = str(box / "caller-ui")
    result = run(ui_setup + exported_ui, FLEA_UI=chosen_ui)
    check(result.returncode == 0 and result.stdout.strip() == chosen_ui,
          "PSS fix suite preserves the caller's UI override")

    start_binary = box / "start-flea"
    captured_ui = box / "captured-ui"
    start_pid = box / "start.pid"
    # Larger than the startup check's bounded diagnostic excerpt.
    DIAGNOSTIC_FIXTURE_CHARS = 8192
    start_binary.write_text(f'''#!{sys.executable}
import os, pathlib, signal, sys
pathlib.Path({str(captured_ui)!r}).write_text(os.environ.get("FLEA_UI", ""))
print("helper stdout cause", flush=True)
print("helper stderr cause", file=sys.stderr, flush=True)
if os.environ.get("FIG_START_MODE") == "hang":
    pathlib.Path({str(start_pid)!r}).write_text(str(os.getpid()))
    signal.pause()
print("x" * {DIAGNOSTIC_FIXTURE_CHARS} + "UNBOUNDED_STDOUT_TAIL")
print("x" * {DIAGNOSTIC_FIXTURE_CHARS} + "UNBOUNDED_STDERR_TAIL", file=sys.stderr)
''')
    start_binary.chmod(0o755)
    def startup(ui=None, mode="failed"):
        env = dict(os.environ, FIG_START_MODE=mode)
        if ui is None:
            env.pop("FLEA_UI", None)
        else:
            env["FLEA_UI"] = ui
        return subprocess.run([sys.executable, str(tree / "tests/figure-helper-start.py"), str(start_binary)],
                              cwd=tree, env=env, capture_output=True,
                              text=True, timeout=FRAGMENT_BOUND_SECONDS)
    result = startup()
    check(captured_ui.read_text() == str(tree / "ui"),
          "PSS fix helper startup exports the candidate UI by default")
    check(result.returncode != 0 and "helper stdout cause" in result.stdout
          and "helper stderr cause" in result.stdout,
          "PSS fix failed startup prints captured stdout and stderr")
    check("UNBOUNDED_STDOUT_TAIL" not in result.stdout and "UNBOUNDED_STDERR_TAIL" not in result.stdout,
          "PSS fix startup diagnostic excerpts stay bounded")
    startup(chosen_ui)
    check(captured_ui.read_text() == chosen_ui,
          "PSS fix helper startup preserves the caller's UI override")
    result = startup(mode="hang")
    check(result.returncode != 0 and "FAIL helper did not exit within" in result.stdout
          and "helper stdout cause" in result.stdout and "helper stderr cause" in result.stdout,
          "PSS fix startup timeout reports its bound and both captured streams")
    result = subprocess.run(["pgrep", "-F", str(start_pid)], capture_output=True, text=True,
                            timeout=FRAGMENT_BOUND_SECONDS)
    check(result.returncode == 1 and not result.stdout,
          f"r3 startup fixture pid={start_pid.read_text()} reaped (pgrep exit={result.returncode})")

    pss_checks = '# The GUI memory claim:' + section('# The GUI memory claim:', '\nprintf \'MARKDOWN_FIGURES %s')
    def memory_samples(before, formulas, diagrams, stamps=(1, 2, 3, 4), anonymous=None, rss=None):
        phases = (("before", before), ("formulas", formulas), ("diagrams", diagrams), ("idle", before))
        anonymous = anonymous if anonymous is not None else (before, formulas, diagrams, before)
        rss = rss if rss is not None else anonymous
        output = "\n".join(f"MARKDOWN_FIGURES FIGPSS phase={phase} pss_kb={value}"
                           + (f" read_seq={stamp}" if stamp is not None else "")
                           + f" anonymous_kb={anon} rss_kb={resident}"
                           for (phase, value), stamp, anon, resident in zip(phases, stamps, anonymous, rss))
        output += "\nMARKDOWN_FIGURES FIGHELPER rss_peak_kb=41140"
        return run(pss_checks, output=output, verdict="0")
    result = memory_samples(52877, 52877, 52877)
    check(result.returncode == 0, "r3 equal PSS samples with fresh read stamps pass")
    for index, phase in enumerate(("before", "formulas", "diagrams", "idle")):
        stamps = [1, 2, 3, 4]
        stamps[index] = None
        result = memory_samples(52877, 52878, 52879, stamps)
        check(result.returncode != 0 and f"FAIL no FIGPSS {phase} read stamp" in result.stdout,
              f"r3 a missing {phase} read stamp fails")
    for stamps in ((1, 1, 3, 4), (1, 2, 1, 4), (1, 2, 3, 3)):
        result = memory_samples(52877, 52878, 52879, stamps)
        check(result.returncode != 0 and "read stamp is stale" in result.stdout,
              f"r3 cached or backward read stamps {stamps} fail")
    result = memory_samples(52877, 52878, 52879)
    check(result.returncode == 0, "PSS fix fresh render-phase samples within budget pass")
    result = memory_samples(52877, 63118, 52879)
    check(result.returncode != 0 and "FAIL formulas Rss exceeds before" in result.stdout,
          "PSS fix fresh formula samples still enforce the existing budget")
    result = memory_samples(52877, 52878, 63118)
    check(result.returncode != 0 and "FAIL diagrams Rss exceeds before" in result.stdout,
          "PSS fix fresh diagram samples still enforce the existing budget")
    memory_before_kb = 52877
    memory_limit_kb = 10240
    over_limit_kb = memory_before_kb + memory_limit_kb + 1
    result = memory_samples(memory_before_kb, over_limit_kb, over_limit_kb,
                            anonymous=(memory_before_kb,) * 4)
    check(result.returncode == 0, "G1 PSS jumps past 10240 kB while Rss and Anonymous are flat pass")
    for index, phase in ((1, "formulas"), (2, "diagrams")):
        rss = [memory_before_kb] * 4
        rss[index] = over_limit_kb
        result = memory_samples(memory_before_kb, memory_before_kb, memory_before_kb,
                                anonymous=(memory_before_kb,) * 4, rss=rss)
        check(result.returncode != 0 and f"FAIL {phase} Rss exceeds before" in result.stdout,
              f"G1 file-backed {phase} Rss growth past 10240 kB with flat Anonymous fails")
    for index, phase in ((1, "formulas"), (2, "diagrams")):
        anonymous = [memory_before_kb] * 4
        anonymous[index] = over_limit_kb
        result = memory_samples(memory_before_kb, memory_before_kb, memory_before_kb,
                                anonymous=anonymous, rss=anonymous)
        check(result.returncode != 0 and f"FAIL {phase} Rss exceeds before" in result.stdout,
              f"G7 flat PSS with explicit {phase} Rss growth past 10240 kB fails")
        result = memory_samples(memory_before_kb, memory_before_kb, memory_before_kb,
                                anonymous=anonymous, rss=(memory_before_kb,) * 4)
        check(result.returncode == 0,
              f"mx2b F25 {phase} Anonymous growth with flat Rss passes")
    result = memory_samples(memory_before_kb, memory_before_kb, memory_before_kb,
                            anonymous=(memory_before_kb, "", memory_before_kb, memory_before_kb))
    check(result.returncode != 0 and "FAIL no FIGPSS formulas Anonymous value" in result.stdout,
          "G7 a missing Anonymous value fails")
    result = memory_samples(memory_before_kb, memory_before_kb, memory_before_kb,
                            rss=(memory_before_kb, "", memory_before_kb, memory_before_kb))
    check(result.returncode != 0 and "FAIL no FIGPSS formulas Rss value" in result.stdout,
          "G1 a missing Rss value fails")

    refusal = section("# A missing engine", "# Byte identity")
    prerequisites = run('for tool in bwrap prlimit mkdir ln cmp wc; do\n    command -v "$tool" || exit 1\ndone',
                        PATH=str(tools))
    check(prerequisites.returncode == 0 and binary.is_file() and os.access(binary, os.X_OK),
          "mx2b F31 missing-engine controls have every prerequisite")
    for mode in ("silent", "wrong"):
        result = run(refusal, mode, PATH=str(tools))
        check(prerequisites.returncode == 0 and result.returncode == 1
              and "FAIL missing qjs did not print the exact refusal on stderr" in result.stdout,
              "F1 rejects " + mode + " missing-engine refusal")
    result = run(refusal, PATH=str(tools))
    check(result.returncode == 0 and "quickjs-ng" in result.stdout and "bwrap and prlimit" in result.stdout,
          "F2 independently exercises engine and sandbox refusal branches")
    (tools / "prlimit").unlink()
    result = run(refusal, PATH=str(tools))
    check(result.returncode == 1 and "FAIL missing required tool prlimit" in result.stdout,
          "mx2b F32 refusal setup names the missing sandbox tool")
    (tools / "prlimit").write_text("#!/bin/sh\nexit 0\n")
    (tools / "prlimit").chmod(0o755)

    probe = f"PROBE_BOUND_SECONDS={FRAGMENT_BOUND_SECONDS}\nnocache=(env -u HOME -u XDG_CACHE_HOME)\nprobe_out=" + section("probe_out=", "# One python driver")
    prerequisites = run('for tool in timeout python3 grep cat; do\n    command -v "$tool" || exit 1\ndone')
    check(prerequisites.returncode == 0 and os.access(binary, os.X_OK),
          "mx2b F31 jailed-probe controls have every prerequisite")
    for mode in ("broken", "empty"):
        result = run(probe, mode)
        expected_status = 1 if mode == "broken" else 0
        check(prerequisites.returncode == 0 and result.returncode == 1
              and f"FAIL jailed probe exited {expected_status} without a valid answer" in result.stdout
              and (mode != "broken" or "figure renderer exploded" in result.stdout),
              "F3 rejects " + mode + " jail probe")
    result = run(probe, "namespace")
    check(result.returncode == 0 and "user namespaces" in result.stdout,
          "F3 names the detected user-namespace exception")
    check(any("No permissions to create new namespace" in p.read_text() for p in box.glob("*stderr*")),
          "F3 retains jailed probe stderr")
    result = run(probe, "good")
    check(result.returncode == 0 and "driving the jailed helper" in result.stdout,
          "F3 keeps the jail for a successful probe")

    missing_qs = 'if ! command -v qs' + section('if ! command -v qs', '\nmkdir -p "$test_root/qsconfig"')
    result = run(missing_qs, PATH=str(box / "empty-path"))
    check(result.returncode == 1 and "qs" in result.stdout and "FAIL" in result.stdout
          and "DONE" not in result.stdout and len(result.stdout.splitlines()) == 1,
          "mx2b F26 absent qs refuses once without a success receipt")
    receipt_check = 'if [ -f "$test_root/hang.pid" ]; then' + section(
        'if [ -f "$test_root/hang.pid" ]; then', '\npass_count=')
    (box / "hang.pid").unlink(missing_ok=True)
    result = run(receipt_check, qs_status="1",
                 output="MARKDOWN_FIGURES FAIL QML load failed\nERROR Type TestRoot unavailable")
    check(result.returncode == 1 and "FAIL QML load failed" in result.stdout
          and "ERROR Type TestRoot unavailable" in result.stdout and "qs exited 1" in result.stdout,
          "mx2b F27 missing pid receipt reports captured launch errors and qs status")
    driver_call = 'if ! ' + section('\nif ! ', '\n# A missing engine')
    (box / "drive.py").write_text('import os\nprint(os.environ.get("FLEA_QJS", "unset"))\n')
    result = run('jailed=0\nengine=("$fleabin" --figure-helper)\n' + driver_call, FLEA_QJS="")
    check(result.returncode == 0 and result.stdout.strip() == str(qjs),
          "mx2b F33 the jailed driver inherits the resolved development engine")

    if shutil.which("qml6"):
        component = box / "component"
        component.mkdir()
        shutil.copyfile(tree / "ui/MarkdownFigure.qml", component / "MarkdownFigure.qml")
        (component / "js").mkdir()
        shutil.copyfile(tree / "ui/js/FigureDate.js", component / "js/FigureDate.js")
        (component / "qmldir").write_text("singleton Theme 1.0 Theme.qml\nsingleton FigureService 1.0 FigureService.qml\n")
        (component / "Theme.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property var font: ({family: "monospace", body: 14})
    property var spacing: ({gap: 8})
    property var color: ({surface: "#202020", foreground: "#eeeeee"})
}
''')
        (component / "FigureService.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property var requests: []
    property int sequence: 0
    // The drawings the service already holds, by source; the real singleton answers its cache the same way.
    property var held: ({"held^1": "<svg/>"})
    function cached(kind, source, theme, display) { return held[source]; }
    signal done(int ticket, string svg, string error)
    function ask(kind, source, display, theme) {
        requests.push(JSON.parse(JSON.stringify(theme)));
        return ++sequence;
    }
}
''')
        for name in ("markdown-board.js", "markdown-render.js"):
            shutil.copyfile(tree / "tests" / name, component / name)
        shutil.copyfile(tree / "tests/figure-component.qml", component / "probe.qml")
        result = subprocess.run(["qml6", str(component / "probe.qml")],
                                env=dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_FORCE_STDERR_LOGGING="1",
                                         XDG_CACHE_HOME=str(box / "qml-cache")),
                                capture_output=True, text=True, timeout=FRAGMENT_BOUND_SECONDS)
        component_output = result.stdout + result.stderr
        component_ok = result.returncode == 0 and "figure-component: 24 check(s), 0 failed" in component_output
        check(component_ok, "mx2a F31/F32 and mx2b F37/F38 one request per creation and per burst, an equal ask dropped, the exact failed-inline fence"
              + ("" if component_ok else ": " + component_output.strip()))
    else:
        check(False, "mx2a F31/F32 require qml6 for the real component probe")

    if shutil.which("node"):
        worker_uri = (tree / "ui/js/FigureWorker.mjs").as_uri()
        # Sample input: a user style's quoted font families on two labels, the way Pele writes a classDef into a style attribute.
        quoted_svg = '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10" style="font-family:&quot;Inter&quot;, sans-serif"><text class="pele-label" style="font-family:&quot;Inter&quot;, sans-serif;font-weight:bold">hello</text><text style="font-family:'Inter', sans-serif">single quotes</text></svg>'''
        result = subprocess.run(["node", "--input-type=module", "-e",
                                 f'import {{ postMermaid }} from {json.dumps(worker_uri)};\n'
                                 + f'console.log(postMermaid({json.dumps(quoted_svg)}, {{bg:"#000000", fg:"#ffffff", font:"monospace"}}));'],
                                capture_output=True, text=True, timeout=FRAGMENT_BOUND_SECONDS)
        try:
            svg = ET.fromstring(result.stdout)
            text_nodes = [node for node in svg.iter() if node.tag.endswith("text")]
            valid = svg.attrib.get("font-family") == "monospace" and len(text_nodes) == 2
            valid = valid and all("font-family" not in node.attrib and "style" not in node.attrib for node in text_nodes)
            valid = valid and text_nodes[0].attrib.get("font-weight") == "bold"
        except (ET.ParseError, IndexError):
            valid = False
        check(result.returncode == 0 and valid,
              "mx2b F30 quoted font families in a style yield well-formed XML and one font, the root's")
    else:
        print("figure-harness: SKIP node is absent, so the mx2b F30 quoted-family check did not run")

    stub = 'cat > "$test_root/stubbin/flea"' + section('cat > "$test_root/stubbin/flea"', 'chmod +x "$test_root/stubbin/flea"')
    (box / "stubbin").mkdir()
    (box / "phase").write_text("answer\n")
    prefix = 'engine=("$qjs" "helper.mjs")\nprintf -v engine_exec \'%q \' "${engine[@]}"\n'
    result = run(prefix + stub + '\n/bin/bash "$test_root/stubbin/flea" --figure-helper')
    check(result.returncode == 0 and "selected direct engine" in result.stdout,
          "F3 QML stub executes the same selected engine")
    check("FLEA_FIG_REAL" not in script, "F3 removes the unread engine export")
    (box / "phase").write_text("hang\n")
    fixture = subprocess.Popen(["/bin/bash", str(box / "stubbin/flea"), "--figure-helper"],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        with selectors.DefaultSelector() as ready:
            ready.register(fixture.stdout, selectors.EVENT_READ)
            signaled = bool(ready.select(FRAGMENT_BOUND_SECONDS))
        receipt = fixture.stdout.readline().strip() if signaled else ""
        check(receipt == f"FIGHANG pid={fixture.pid}" and fixture.poll() is None,
              "r3 the shell hang fixture blocks in its own process")
        result = subprocess.run(["pgrep", "-F", str(box / "hang.pid")], capture_output=True,
                                text=True, timeout=FRAGMENT_BOUND_SECONDS)
        check(result.returncode == 0 and result.stdout.strip() == str(fixture.pid),
              f"r3 hanging shell fixture pid={fixture.pid} present (pgrep exit={result.returncode})")
    finally:
        if fixture.poll() is None:
            fixture.kill()
        fixture.communicate(timeout=FRAGMENT_BOUND_SECONDS)
    result = subprocess.run(["pgrep", "-F", str(box / "hang.pid")], capture_output=True,
                            text=True, timeout=FRAGMENT_BOUND_SECONDS)
    check(fixture.returncode == -signal.SIGKILL and result.returncode == 1 and not result.stdout,
          f"r3 shell fixture pid={fixture.pid} killed and reaped (pgrep exit={result.returncode})")

    # Sample input: the drive.py heredoc between <<'EOF' and the next standalone EOF.
    driver = section('cat > "$test_root/drive.py" <<\'EOF\'\n', "\nEOF")
    runner = box / "driver.py"
    runner.write_text(driver)
    duplicate = box / "duplicate.py"
    duplicate.write_text('''#!/usr/bin/python3
import json
import sys
for line in sys.stdin:
    try:
        request = json.loads(line)
        ident = request["id"]
    except ValueError:
        ident = 0
    if ident in (0, 30, 31, 40, 41):
        reply = {"id": ident, "error": "malformed"}
    elif ident == 43:
        reply = {"id": ident, "error": "diagram over 32 KiB"}
    else:
        reply = {"id": ident, "svg": "<svg/>"}
    print(json.dumps(reply))
    if ident == 10:
        print(json.dumps(reply))
''')
    duplicate.chmod(0o755)
    result = subprocess.run([sys.executable, str(runner), str(duplicate), "unused", str(box)],
                            capture_output=True, text=True, timeout=FRAGMENT_BOUND_SECONDS)
    check(result.returncode != 0 and "FAIL every request answered once" in result.stdout,
          "F5 rejects a duplicate reply alongside all expected ids")

    byte_phase = section("# Byte identity", "# FigureService")
    identity_check = 'python3 - "$test_root/node-expected.json"' + section(
        'python3 - "$test_root/node-expected.json"', "\nelse\n")
    expected = box / "node-expected.json"
    actual = box / "node-actual.json"
    expected.write_text('{"frac":"<svg/>"}')
    actual.write_text('{"frac":"<svg!>"}')
    result = run(identity_check + '\nprintf "later suite verdict reached\\n"\n')
    check(result.returncode != 0 and "later suite verdict reached" not in result.stdout,
          "mx2a F12 one changed rendered byte fails the suite")
    check("markdown-figures.sh: FAIL qjs and node disagree on rendered bytes" in result.stdout,
          "mx2a F12 byte mismatch names the failed comparison")
    actual.write_text(expected.read_text())
    result = run(identity_check)
    check(result.returncode == 0 and "PASS qjs renders" in result.stdout,
          "mx2a F12 identical rendered bytes pass")
    result = run(byte_phase + '\nprintf "service phase reached\\n"\n', PATH=str(tools))
    check(result.returncode == 0 and "SKIP node is absent" in result.stdout and "service phase reached" in result.stdout,
          "F11 no-node skip continues to the service and PSS phase")

    if shutil.which("node"):
        alias = box / "checkout&#path"
        alias.symlink_to(tree, target_is_directory=True)
        start = 'cat > "$test_root/identity.mjs"' if 'cat > "$test_root/identity.mjs"' in script else "# Build identity imports"
        generator = (start + section(start, 'node "$test_root/identity.mjs"'))
        result = run(generator, PWD=str(alias))
        theme = box / "theme.json"
        theme.write_text(json.dumps({"bg": "#101315", "fg": "#c0caf5", "bodyPx": 14}))
        if result.returncode == 0:
            result = subprocess.run(["node", str(box / "identity.mjs"), str(theme), str(box / "identity.json")],
                                    capture_output=True, text=True, timeout=FRAGMENT_BOUND_SECONDS)
        check(result.returncode == 0, "F14 identity imports work with & and # in the checkout path")

worker = (tree / "ui/js/FigureWorker.mjs").read_text()
check(not re.search(r"(?m)^[ \t]*//[^\n]*\n[ \t]*//", worker), "F7 worker comment paragraphs occupy one line")
check(not re.search(r"(?m)^#[^\n]*\n#", script[script.index("\n") + 1:]), "F7 shell comment paragraphs occupy one line")
check("depth > 12" not in worker and "/ 2;" not in worker and "* 100) / 100" not in worker,
      "F12 resolver and ex conversion policy numbers have names")
qml = (tree / "tests/markdown-figures.qml").read_text()
check("readonly property int hangingRenderLimitMs: 300000" in qml
      and "Flea.FigureService.renderMs = shell.hangingRenderLimitMs" in qml
      and "readonly property int requiredPendingPumpCallbacks: 2" in qml
      and "readonly property int expiredTicketDeadline: 0" in qml
      and "shell.pendingPumpCallbacks === shell.requiredPendingPumpCallbacks" in qml
      and "shell.ticket > 0 && Flea.FigureService.waiting[shell.ticket] !== undefined" in qml
      and "Flea.FigureService.deadlineExpirations === shell.renderDeadlineMark" in qml
      and "shell.pendingPumpCallbacks++;" in qml
      and "Flea.FigureService.waiting[shell.ticket].deadline = shell.expiredTicketDeadline;" in qml
      and "shell.hangingDeadlineArmed && shell.pendingPumpCallbacks === shell.requiredPendingPumpCallbacks" in qml
      and "blockedLoopTicks" not in qml and "pendingTicksMark" not in qml,
      "G2 hanging deadline follows pending-ticket pump callbacks without racing a duration")
# Sample input: "    time.sleep(10)" in a generated Python hang fixture.
check(not re.search(r"(?m)^    time[.]sleep[(]", pathlib.Path(__file__).read_text()) and "hang) sleep" not in script,
      "r3 hang fixtures block without wall-clock sleeps")
reader = (tree / "tests/figure-memory/FigureMemory.qml").read_text()
check('rss: memory.memValue(contents, "Rss")' in reader and '" rss_kb=" + sample.rss' in qml,
      "G1 Rss, PSS and Anonymous share one stamped memory snapshot")
# Sample input: var kids = memory.readText("/proc/1234/task/1234/children").trim().split(/\s+/);
children_line = next(line for line in reader.splitlines() if '"/children"' in line)
check("Sample input:" in reader.split(children_line, 1)[0].splitlines()[-1],
      "r3 the children parser has a sample-input comment directly above it")
check("Sample input" in reader.split("    function memField", 1)[0].splitlines()[-1],
      "r3 the memory-field parser has a sample-input comment")
check("Date.now()" not in qml and "maxGap" not in qml,
      "mx2a F13 / mx2b F4 QML verdicts use events instead of elapsed time")
service_test = (tree / "tests/js/figureservice.js").read_text()
declaration = "function cacheKeyOf(kind, source, t, display) {"
regex_before = service_test.split("    var functions =", 1)[0].splitlines()[-1]
check("Sample input:" in regex_before and declaration in regex_before,
      "mx2a F14 declaration parser quotes a real service declaration")
start_test = (tree / "tests/figure-helper-start.py").read_text()
check("timeout=HELPER_EXIT_BOUND_SECONDS" in start_test
      and "# A helper that never exits fails the check instead of hanging the suite.\nHELPER_EXIT_BOUND_SECONDS = 5" in start_test,
      "mx2a F16 / mx2b F15 helper startup has a named termination bound")
build = (tree / "tools/vendor-js/build.sh").read_text()
check(not re.search(r"(?m)^#[^\n]*\n#", build[build.index("\n") + 1:]),
      "mx2b F24 build.sh comment paragraphs occupy one line")
# Sample input: cmp math-bundle.mjs ../../ui/vendor/math.mjs || {.
targets = re.findall(r"(?m)^cmp \S+ (\S+)", build)
check(len(targets) == 2, "mx2b F22 build compares both tracked bundles")
for target in targets:
    resolved = (tree / "tools/vendor-js" / target).resolve()
    vendor_targets = {tree / "ui/vendor/math.mjs", tree / "ui/vendor/mermaid.mjs"}
    check(resolved.is_file() and resolved in vendor_targets,
          f"mx2b F22 comparison resolves to the shipped bundle: {target}")
    if (tree / ".git").exists():
        relative = str(resolved.relative_to(tree))
        tracked = subprocess.run(["git", "ls-files", "--error-unmatch", relative], cwd=tree,
                                 capture_output=True, text=True, timeout=FRAGMENT_BOUND_SECONDS)
        check(tracked.returncode == 0, f"mx2b F22 comparison target is tracked: {target}")
# The real build.sh runs beside stub npm and npx in a throwaway layout, so its refusal and its comparison both execute.
with tempfile.TemporaryDirectory(prefix="flea-vendor-targets-") as scratch:
    layout = pathlib.Path(scratch)
    build_dir = layout / "tools/vendor-js"
    vendor = layout / "ui/vendor"
    stubs = layout / "stubs"
    for directory in (build_dir, vendor, stubs):
        directory.mkdir(parents=True)
    shutil.copyfile(tree / "tools/vendor-js/build.sh", build_dir / "build.sh")
    (build_dir / "package.json").write_text("{}\n")
    npm_receipt = layout / "npm.ran"
    (stubs / "npm").write_text(f'#!/bin/sh\n: > "{npm_receipt}"\n')
    # Sample input: npx esbuild math-entry.mjs --bundle --minify --outfile=math-bundle.mjs.
    (stubs / "npx").write_text('#!/bin/sh\nfor arg in "$@"; do\n    case "$arg" in\n'
                               '        --outfile=*) printf "same bytes\\n" > "${arg#--outfile=}" ;;\n    esac\ndone\n')
    for stub in ("npm", "npx"):
        (stubs / stub).chmod(0o755)

    def rebuild(**extra):
        npm_receipt.unlink(missing_ok=True)
        return subprocess.run(["/bin/bash", str(build_dir / "build.sh")], capture_output=True, text=True,
                              env=dict(os.environ, PATH=f"{stubs}:{os.environ['PATH']}", **extra),
                              timeout=FRAGMENT_BOUND_SECONDS)

    for name in ("math", "mermaid"):
        (vendor / f"{name}.mjs").write_text("same bytes\n")
    result = rebuild()
    check(result.returncode == 0 and "both bundles reproduce byte for byte" in result.stdout,
          "mx2b F22 a rebuild identical to ui/vendor passes")
    for name in ("math", "mermaid"):
        target = vendor / f"{name}.mjs"
        target.write_text("other bytes\n")
        result = rebuild()
        check(result.returncode == 1 and f"vendor-js: {name}.mjs differs" in result.stdout,
              f"mx2b F22 a changed {name} bundle fails the byte comparison")
        target.unlink()
        result = rebuild()
        check(result.returncode == 1 and f"missing tracked target ../../ui/vendor/{name}.mjs" in result.stdout
              and not npm_receipt.exists(), f"mx2b F22 an absent {name} target refuses by name before npm runs")
        target.write_text("same bytes\n")
testing_doc = (tree / "docs/architecture/testing.md").read_text()
sandbox = (tree / "src/backend/sandbox.rs").read_text()
check("const READONLY_PREFIX_ARGS: usize = 4;" in sandbox
      and "const READONLY_BIND_ARGS: usize = 3;" in sandbox
      and "READONLY_PREFIX_ARGS + ro_binds.len() * READONLY_BIND_ARGS" in sandbox,
      "mx2a F30 read-only sandbox prefix and bind argument counts are named")
service_source = (tree / "ui/FigureService.qml").read_text()
# Sample input: `const REFUSED: i32 = 127;` in src/figurehelper.rs.
def rust_refusal(source):
    found = re.search(r"(?m)^(?:pub )?const REFUSED: i32 = (\d+);", source)
    return int(found[1]) if found else None


# Sample input: `readonly property int refusalExit: 127` in ui/FigureService.qml.
def qml_refusal(source):
    found = re.search(r"(?m)^\s*readonly property int refusalExit: (\d+)$", source)
    return int(found[1]) if found else None


helper_rust = (tree / "src/figurehelper.rs").read_text()
rust_status = rust_refusal(helper_rust)
qml_status = qml_refusal(service_source)
check(rust_status is not None and rust_status == qml_status
      and "REFUSED in src/figurehelper.rs" in service_source
      and "exitCode === root.refusalExit" in service_source,
      f"mx2a F35 and F38 the QML refusal status ({qml_status}) equals the Rust REFUSED ({rust_status})")
sample_rust = "const REFUSED: i32 = 127;\n"
sample_qml = "    readonly property int refusalExit: 127\n"
check(rust_refusal(sample_rust) == qml_refusal(sample_qml) == 127
      and rust_refusal(sample_rust.replace("127", "126")) != qml_refusal(sample_qml)
      and qml_refusal(sample_qml.replace("127", "126")) != rust_refusal(sample_rust),
      "mx2b F36 a changed refusal status on either side no longer matches")
check(rust_refusal("") is None and qml_refusal("") is None,
      "mx2b F36 an absent refusal status parses as nothing, never as a match")
helper_source = (tree / "ui/vendor/figure-helper.mjs").read_text()
check(not re.search(r"(?m)^[ \t]*//[^\n]*\n[ \t]*//", helper_source),
      "F26 helper comments keep each constraint on one line")
figure_source = (tree / "ui/MarkdownFigure.qml").read_text()
header = figure_source.splitlines()[2]
check(header.startswith("// One rendered figure:") and len(header) <= HEADER_COMMENT_MAX_CHARS,
      "mx2a F36 the figure header states its purpose in one short line")
fallback = figure_source.split("readonly property string fallbackBody:", 1)[1].split("    Rectangle", 1)[0]
check("readonly property int fallbackChars: 2000" in figure_source
      and fallback.count("root.fallbackChars") == 3 and "2000" not in fallback,
      "F27 fallback comparison, slice and remaining count share a named limit")
headless_contract = testing_doc.split("The suites that drive the debug binary", 1)[1].split("Its own `headless=`", 1)[0]
check("needs nothing but a shell" not in headless_contract
      and all(word in headless_contract for word in ("no display, session or hardware", "python3", "Qt", "Quickshell", "quickjs-ng", "refuse loudly", "naming")),
      "mx2b F8 headless contract names its tools and missing-tool refusal")
# The comment lines directly above the first esbuild run state its one constraint.
constraint = re.search(r"((?:^#[^\n]*\n)+)npx esbuild", build, re.M)[1].strip().splitlines()
check(len(constraint) == 1 and len(constraint[0]) <= 140
      and all(word in constraint[0] for word in ("quickjs-ng", "ES modules", "neutral", "es2017", "minified", "exact command")),
      "mx2b F7 bundle constraint fits one comment line")
# The bundle entry resolves every colour, a label halo's color-mix too, so the worker keeps no resolver of its own.
entry = (tree / "tools/vendor-js/mermaid-entry.mjs").read_text()
check("const PERCENT = 100;" in entry and "parseFloat(share) / PERCENT" in entry,
      "mx2b F12 color-mix percentage conversion uses its named scale")
check(all(expression not in worker + entry for expression in
          ("i + 3", "i + 4", "m + 9", "m + 10", "parseInt(h, 16)", "(n >> 16) & 255", "(n >> 8) & 255", ", 16)"))
      and "const HEX_RADIX = 16;" in entry and "function resolveValue" not in worker,
      "mx2b F34 CSS token lengths, hex radix, byte shifts and mask have names")
check("shell.t0 < 5000" not in qml and "shell.maxGap < 2000" not in qml,
      "F12 idle-exit and tick-gap bounds have names")
for text, marker in ((worker, "function styleToAttributes"), (worker, "function iconGroups"), (worker, "function rootFont"),
                     (worker, "function lineTexts"), (worker, "function accentMarks"), (worker, "function transformOf"),
                     (worker, "function outlineOf"), (worker, "function arcExtremes"), (worker, "function strokeRoom"),
                     (worker, "export function checkSafe"), (worker, "function decodeReferences"), (worker, "function attributeError"), (worker, "function styleError"),
                     (worker, "function inAttributes"), (entry, "function inAttributes"), (entry, "function todayOf"), (entry, "function faceOf"),
                     (entry, "function mix"), (entry, "function resolve"), (entry, "function mixed")):
    before = text.split(marker, 1)[0].splitlines()[-1]
    check("Sample input:" in before, "F13 parser has sample input: " + marker.strip())
print(f"figure-harness: {checks} check(s), {failures} failed")
sys.exit(bool(failures))
