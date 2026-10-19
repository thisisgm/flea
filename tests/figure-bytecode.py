# The helper launcher's bytecode cache end to end, through jail tools that log what they are asked to run: figure-bytecode.py FLEA_BIN
import ctypes
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

MARKER = ".flea-test-sandbox"
PREFIX = "flea-figure-bytecode-"
# A helper or build that never ends fails the check instead of holding the suite.
BOUND_SECONDS = 120
# prctl(2) PR_SET_CHILD_SUBREAPER: orphaned background builds become this process's children, so it can wait for them without polling.
PR_SET_CHILD_SUBREAPER = 36
# A key and its directory are 128 bits in hex; a bytecode blob's bytes are corrupted or flipped at these offsets.
KEY_HEX_CHARS = 32
BLOB_FLIP_OFFSET = 100
# The first bytecode byte is the engine's format version, so any other value is refused at the read.
BLOB_VERSION_OFFSET = 0
THEME = {"bg": "#101315", "fg": "#c0caf5", "accent": "#7aa2f7", "font": "monospace", "bodyPx": 14}
MATHS = ["\\frac{a}{b}", "\\int_0^1 x^2\\,dx", "\\sum_{n=1}^{\\infty}\\frac{1}{n^2}", "\\begin{matrix}a&b\\\\c&d\\end{matrix}", "x^2", "\\frac{unclosed"]
DIAGRAMS = ["flowchart TD\n    A --> B", "sequenceDiagram\n    A->>B: hi", "stateDiagram-v2\n    A --> B", "classDiagram\n    A <|-- B",
            "erDiagram\n    A ||--|| B : has", "not a diagram {{{"]

repo = pathlib.Path(__file__).resolve().parents[1]
flea = pathlib.Path(sys.argv[1]).resolve()
failures = 0
checks = 0


def check(passed, label):
    global failures, checks
    checks += 1
    failures += 0 if passed else 1
    print(("PASS " if passed else "FAIL ") + label)


def engine():
    for candidate in (os.environ.get("FLEA_QJS"), "/usr/bin/qjs", str(repo / ".superpowers/tools/qjs")):
        if candidate and os.path.isabs(candidate) and os.access(candidate, os.X_OK):
            return candidate
    sys.exit("figure-bytecode.py: no qjs (FLEA_QJS, /usr/bin/qjs)")


root = pathlib.Path(tempfile.mkdtemp(prefix=PREFIX)).resolve()
(root / MARKER).write_text("")
cache = root / "cache"
log = root / "jail.log"
ui = root / "ui"
tools = root / "tools"


def cleanup():
    if root.is_absolute() and root.name.startswith(PREFIX) and (root / MARKER).is_file():
        shutil.rmtree(root)


def build_tree():
    for name in ("boot", "vendor", "js"):
        (ui / name).mkdir(parents=True)
    (ui / "boot/shell.qml").write_text("")
    for module in (repo / "ui/vendor").glob("*.mjs"):
        shutil.copy(module, ui / "vendor" / module.name)
    shutil.copy(repo / "ui/js/FigureWorker.mjs", ui / "js/FigureWorker.mjs")
    tools.mkdir()
    # The stub jail logs its arguments tab-separated, then runs the command unjailed: the argv is the subject, the real jail is the suites'.
    (tools / "prlimit").write_text('#!/bin/sh\nwhile case "$1" in --*) true ;; *) false ;; esac; do shift; done\nexec "$@"\n')
    (tools / "bwrap").write_text('''#!/bin/sh
(IFS="$(printf '\\t')"; printf '%s\\t%s\\n' "$$" "$*" >> "$JAIL_LOG")
while [ $# -gt 0 ]; do
    case "$1" in
        --setenv|--ro-bind|--ro-bind-try|--bind|--symlink) shift 3 ;;
        --proc|--dev|--tmpfs|--json-status-fd) shift 2 ;;
        --*) shift ;;
        *) break ;;
    esac
done
# JAIL_CORRUPT stands for a compile whose output the engine refuses to read: the first byte of math.bc is overwritten after the compile.
case "$*" in
    *figure-compile.mjs*)
        if [ -n "${JAIL_CORRUPT:-}" ]; then
            "$@" || exit $?
            for scratch; do :; done
            printf '\\377' | dd of="$scratch/math.bc" bs=1 seek=@OFFSET@ conv=notrunc 2>/dev/null
            exit 0
        fi ;;
esac
exec "$@"
'''.replace("@OFFSET@", str(BLOB_VERSION_OFFSET)))
    for tool in tools.iterdir():
        tool.chmod(0o755)


def environment(with_cache, hook=None, extra=None):
    env = {"PATH": str(tools) + ":" + os.environ["PATH"], "FLEA_QJS": engine(), "FLEA_UI": str(ui), "JAIL_LOG": str(log),
           "HOME": str(root / "home"), "LC_ALL": "C.UTF-8"}
    if hook:
        env["FLEA_FIGURE_CACHE"] = hook
    env.update(extra or {})
    if with_cache:
        env["XDG_CACHE_HOME"] = str(cache)
    else:
        env.pop("HOME")
    return env


def requests():
    body = []
    for i, source in enumerate(MATHS):
        body.append({"id": 10 + i, "kind": "math", "source": source, "display": True, "theme": THEME})
    for i, source in enumerate(DIAGRAMS):
        body.append({"id": 20 + i, "kind": "mermaid", "source": source, "display": True, "theme": THEME})
    return "".join(json.dumps(r) + "\n" for r in body)


def jail_lines():
    return log.read_text().splitlines() if log.exists() else []


def reap():
    # Blocks until every orphaned background build has ended; the count is how many there were, 0 when none started.
    count = 0
    while True:
        try:
            _, status = os.wait()
        except ChildProcessError:
            return count, True
        count += 1
        if os.waitstatus_to_exitcode(status) != 0:
            return count, False


def helper(with_cache=True, flags=(), body=None, hook=None, extra=None):
    # The launcher execs into the stub jail, so the line carrying the helper's own pid is the render jail and every other line is the background build's.
    before = len(jail_lines())
    child = subprocess.Popen([str(flea), "--figure-helper", *flags], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=environment(with_cache, hook, extra))
    out, err = child.communicate(requests() if body is None else body, timeout=BOUND_SECONDS)
    started, clean = reap()
    lines = [line.split("\t") for line in jail_lines()[before:]]
    render = [line[1:] for line in lines if line[0] == str(child.pid)]
    built = [line[1:] for line in lines if line[0] != str(child.pid)]
    done = subprocess.CompletedProcess(child.args, child.returncode, out, err)
    return done, render, started, clean, built


def keys():
    return sorted(p.name for p in (cache / "flea/figures").iterdir() if len(p.name) == KEY_HEX_CHARS) if (cache / "flea/figures").is_dir() else []


def bytecode_dir_of(argv):
    flags = [t[len("--bytecode="):] for t in argv if t.startswith("--bytecode=")]
    return flags[0] if flags else None


def main():
    prctl = ctypes.CDLL(None, use_errno=True).prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0)
    if prctl != 0:
        check(False, f"prctl(PR_SET_CHILD_SUBREAPER) failed with errno {ctypes.get_errno()}, so no background build can be waited for")
        print(f"figure-bytecode: {checks} check(s), {failures} failed")
        return
    build_tree()
    source, renders, started, _, _ = helper(with_cache=False)
    check(source.returncode == 0 and source.stderr == "" and len(source.stdout.splitlines()) == len(MATHS) + len(DIAGRAMS), "the source path answers every request")
    check(started == 0 and len(renders) == 1 and bytecode_dir_of(renders[0]) is None, "with no cache dir the helper runs from source and builds nothing")
    check(not any(t == "--bind" for t in renders[0]), "the render jail has no writable bind")

    # A compile whose math.bc the engine refuses to read: the manifest matches it and the source fallback answers the same bytes, so only the helper's load report can tell.
    refused, renders, started, clean, _ = helper(extra={"JAIL_CORRUPT": "1"})
    check(refused.stdout == source.stdout and started == 1, "a build over an unreadable blob still starts and the helper answers from source")
    check(keys() == [] and not clean, "a blob the engine refuses to load leaves no verified directory and fails the build")
    check(len(list((cache / "flea/figures").glob("*.failed"))) == 1, "and the failure is marked, so it is not retried at once")
    shutil.rmtree(cache / "flea")

    cold, renders, started, clean, built = helper()
    check(cold.stdout == source.stdout and cold.stderr == "", "the first run answers the source path's bytes")
    check(bytecode_dir_of(renders[0]) is None and started == 1 and clean, "the first run starts from source and one background build finishes cleanly")
    key = keys()
    check(len(key) == 1 and (cache / "flea/figures" / key[0] / "manifest").is_file(), "the build installs one verified directory")
    if not key:
        check(False, "no verified directory was installed, so every later check that reads one is counted failed")
        print(f"figure-bytecode: {checks} check(s), {failures} failed")
        return
    check(len(built) > 0, "the build ran its compile and smoke runs through the jail, and the log saw them")
    compile_call = built[0] if built else []
    writable = [compile_call[i + 1] for i, t in enumerate(compile_call) if t == "--bind"]
    check(len(writable) == 1 and writable[0].startswith(str(cache / "flea/figures")), "the compile jail's only writable path is inside the cache dir")
    check(all(not any(t == "--bind" for t in call) for call in [renders[0]] + built[1:]), "no render jail, probe included, has a writable bind")

    warm, renders, started, _, _ = helper()
    keydir = str(cache / "flea/figures" / key[0])
    check(warm.stdout == source.stdout and warm.stderr == "", "the bytecode path answers the source path's bytes for the corpus")
    bound = any(renders[0][i:i + 3] == ["--ro-bind", keydir, keydir] for i in range(len(renders[0]) - 2))
    check(bytecode_dir_of(renders[0]) == keydir and bound, "the render jail binds the verified directory read-only and names it last")
    check(started == 0, "a verified cache starts no build")

    # A flipped byte, a short file, a foreign manifest and a linked blob each fall back to source and are rebuilt whole.
    blob = cache / "flea/figures" / key[0] / "math.bc"
    original = blob.read_bytes()
    damages = {
        "a flipped byte": lambda: blob.write_bytes(original[:BLOB_FLIP_OFFSET] + bytes([original[BLOB_FLIP_OFFSET] ^ 1]) + original[BLOB_FLIP_OFFSET + 1:]),
        "a truncated file": lambda: blob.write_bytes(original[:-1]),
        "a foreign manifest": lambda: (blob.parent / "manifest").write_text("flea-figures 1\nkey " + "0" * KEY_HEX_CHARS + "\n"),
        "a linked blob": lambda: (blob.unlink(), blob.symlink_to(root / "elsewhere.bc")),
    }
    for label, damage in damages.items():
        (root / "elsewhere.bc").write_bytes(original)
        damage()
        again, renders, started, clean, _ = helper()
        check(again.stdout == source.stdout and bytecode_dir_of(renders[0]) is None, label + " is refused and the helper answers from source")
        check(started == 1 and clean and keys() == key, label + " is replaced by one background build")
        after, renders, started, _, _ = helper()
        check(bytecode_dir_of(renders[0]) == keydir and started == 0 and after.stdout == source.stdout, label + " leaves a cache the next run uses")

    # A changed source is a new key: source this once, one build, the old directory gone, the new one used.
    math = ui / "vendor/math.mjs"
    math.write_text("// changed\n" + math.read_text())
    stale, renders, started, clean, _ = helper()
    check(stale.stdout == source.stdout and bytecode_dir_of(renders[0]) is None and started == 1 and clean, "a changed source misses the old key and rebuilds in the background")
    fresh = keys()
    check(len(fresh) == 1 and fresh != key, "the old key's directory is swept and the new key's stays")
    again, renders, started, _, _ = helper()
    check(bytecode_dir_of(renders[0]) == str(cache / "flea/figures" / fresh[0]) and started == 0 and again.stdout == source.stdout, "the next run uses the new key's bytecode")

    # A warm start loads the named bundles and says so before any request; its answers are the cold answers, with the one extra line.
    warm, renders, started, _, _ = helper(flags=["--warm=mermaid,math"])
    lines = warm.stdout.splitlines()
    check(warm.returncode == 0 and warm.stderr == "" and json.loads(lines[0]) == {"id": 0, "warm": ["mermaid", "math"]}, "a warm start announces the kinds it loaded before any request")
    check("\n".join(lines[1:]) + "\n" == source.stdout and started == 0, "a warm helper answers the cold helper's bytes")
    check("--warm=mermaid,math" in renders[0] and bytecode_dir_of(renders[0]) is not None and not any(t == "--bind" for t in renders[0]), "the warm argument reaches the helper beside the bytecode and widens nothing")
    idle, renders, _, _, _ = helper(flags=["--warm=math"], body="")
    check(idle.returncode == 0 and idle.stdout.splitlines() == ['{"id":0,"warm":["math"]}'], "a warm helper with no request exits at EOF having loaded only maths")
    hostile, renders, _, _, _ = helper(flags=["--warm=../x,Math"], body="")
    check(hostile.returncode == 0 and hostile.stdout == "" and not any(t.startswith("--warm") for t in renders[0]), "an unknown kind in the warm argument never reaches the helper")
    # The hook that turns the cache off leaves the source path with a cache dir in place, and builds nothing.
    shutil.rmtree(cache / "flea")
    off, renders, started, _, _ = helper(hook="off")
    check(off.stdout == source.stdout and bytecode_dir_of(renders[0]) is None and started == 0 and not (cache / "flea").exists(), "FLEA_FIGURE_CACHE=off runs from source and builds nothing")
    # The svg-only hook keeps the SVG cache on and the bytecode off, so a figure test needs no background build.
    svg, renders, started, _, _ = helper(hook="svg")
    check(svg.stdout == source.stdout and bytecode_dir_of(renders[0]) is None and started == 0 and not (cache / "flea").exists(), "FLEA_FIGURE_CACHE=svg runs from source and builds nothing")
    print(f"figure-bytecode: {checks} check(s), {failures} failed")


try:
    main()
finally:
    cleanup()
sys.exit(1 if failures else 0)
