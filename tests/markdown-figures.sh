#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Exercise formula and diagram rendering, malformed and hostile inputs, size limits, EOF, engine refusals and the FigureService lifecycle.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
export FLEA_UI="${FLEA_UI:-$PWD/ui}"
# The service checks below are about tickets and the helper's life, so they run from source; the bytecode block turns the cache back on.
export FLEA_FIGURE_CACHE=off

fleabin="$PWD/target/debug/flea"
[ -x "$fleabin" ] || {
    echo "markdown-figures.sh: no debug binary at $fleabin, run cargo build first"
    exit 1
}
python3 tests/figure-helper-start.py "$fleabin" || exit 1
python3 tests/figure-harness.py || exit 1
env -u FLEA_FIGURE_CACHE python3 tests/figure-bytecode.py "$fleabin" || exit 1
# FLEA_QJS names the engine: an absolute executable path wins, then the Arch system binary, then the dev tree copy.
resolve_qjs() {
    if [ -n "${FLEA_QJS:-}" ] && [ "${FLEA_QJS#/}" != "${FLEA_QJS}" ] && [ -x "${FLEA_QJS}" ]; then
        printf '%s\n' "${FLEA_QJS}"
    elif [ -x /usr/bin/qjs ]; then
        printf '%s\n' /usr/bin/qjs
    elif [ -x "$PWD/.superpowers/tools/qjs" ]; then
        printf '%s\n' "$PWD/.superpowers/tools/qjs"
    else
        return 1
    fi
}
qjs=$(resolve_qjs) || {
    echo "markdown-figures.sh: no qjs (FLEA_QJS=${FLEA_QJS:-unset}, /usr/bin/qjs, $PWD/.superpowers/tools/qjs)"
    exit 1
}

test_root="$FIXTURE_ROOT/flea-markdown-figures-$$"
sandbox_make "$test_root"
cleanup() {
    sandbox_remove "$test_root"
}
trap cleanup EXIT

# Only a detected denial of user namespaces permits the direct-engine test fallback.

# Bound the jailed startup probe so a broken helper cannot hold the suite open.
PROBE_BOUND_SECONDS=10
# The probe and the first drive run with no home, so the launcher builds no cache and the whole corpus runs from source.
nocache=(env -u HOME -u XDG_CACHE_HOME)
probe_out=$(printf '%s\n' '{"id":1,"kind":"math","source":"x^2","display":false,"theme":{"bg":"#101315","fg":"#c0caf5"}}' | FLEA_QJS="$qjs" timeout "$PROBE_BOUND_SECONDS" "${nocache[@]}" "$fleabin" --figure-helper 2> "$test_root/probe.stderr")
probe_rc=$?
# Sample input: {"id":1,"svg":"<svg/>"}, exactly one successful answer to the probe.
if [ "$probe_rc" -eq 0 ] && [ ! -s "$test_root/probe.stderr" ] && printf '%s' "$probe_out" | python3 -c '
import json
import sys
try:
    reply = json.load(sys.stdin)
    valid = reply.get("id") == 1 and reply.get("svg", "").startswith("<svg") and "error" not in reply
except (ValueError, AttributeError):
    valid = False
sys.exit(not valid)
'; then
    engine=("${nocache[@]}" "$fleabin" --figure-helper)
    jailed=1
    echo "markdown-figures.sh: driving the jailed helper"
elif [ "$probe_rc" -ne 0 ] && grep -q '^bwrap: No permissions to create new namespace' "$test_root/probe.stderr"; then
    engine=("$qjs" "$PWD/ui/vendor/figure-helper.mjs")
    jailed=0
    echo "markdown-figures.sh: SKIP no user namespaces in this container, driving qjs direct"
    cat "$test_root/probe.stderr"
else
    echo "markdown-figures.sh: FAIL jailed probe exited $probe_rc without a valid answer"
    cat "$test_root/probe.stderr"
    exit 1
fi

# One python driver builds every request, so the shell never quotes a formula.
cat > "$test_root/drive.py" <<'EOF'
import json, os, subprocess, sys
import re
from collections import Counter
*command, outdir = sys.argv[1:]
# xmlns is a namespace, never a fetch, so it is stripped before the check.
def clean(svg):
    return re.sub(r'xmlns(?::\w+)?="[^"]*"', "", svg)
theme = {"bg": "#101315", "fg": "#c0caf5", "accent": "#7aa2f7", "font": "monospace", "bodyPx": 14}
maths = ["\\frac{a}{b}", "\\int_0^1 x^2\\,dx", "\\sum_{n=1}^{\\infty}\\frac{1}{n^2}",
    "\\begin{matrix}a&b\\\\c&d\\end{matrix}", "\\begin{aligned}x&=1\\\\y&=2\\end{aligned}",
    "x^2", "\\sqrt{2}", "\\alpha+\\beta=\\gamma",
    "\\lim_{x\\to0}\\frac{\\sin x}{x}", "\\binom{n}{k}=\\frac{n!}{k!(n-k)!}"]
diags = ["flowchart TD\n    A --> B", "sequenceDiagram\n    A->>B: hi",
    "stateDiagram-v2\n    A --> B", "classDiagram\n    A <|-- B",
    "erDiagram\n    A ||--|| B : has", "xychart-beta\n    x-axis [a, b]\n    bar [1, 2]"]
# Sample input: {"id":1,"kind":"math","source":"\\frac{a}{b}","display":true,"theme":{...}}.
reqs = []
for i, m in enumerate(maths):
    reqs.append({"id": 10 + i, "kind": "math", "source": m, "display": True, "theme": theme})
for i, d in enumerate(diags):
    reqs.append({"id": 20 + i, "kind": "mermaid", "source": d, "display": True, "theme": theme})
# The malformed pair, then a good request proving the loop survived both.
reqs.append({"id": 30, "kind": "math", "source": "\\frac{unclosed", "display": True, "theme": theme})
reqs.append({"id": 31, "kind": "mermaid", "source": "not a diagram {{{", "display": True, "theme": theme})
reqs.append({"id": 32, "kind": "math", "source": "x^2", "display": False, "theme": theme})
# The hostile trio: two formulas that must fail, one diagram unwrapped to no link.
reqs.append({"id": 40, "kind": "math", "source": "\\href{http://127.0.0.1:18037/x}{click}", "display": True, "theme": theme})
reqs.append({"id": 41, "kind": "math", "source": "\\url{http://127.0.0.1:18037/y}", "display": True, "theme": theme})
reqs.append({"id": 42, "kind": "mermaid", "source": "flowchart TD\n    A --> B\n    click A href \"http://127.0.0.1:18037/evil\"", "display": True, "theme": theme})
big = "flowchart TD\n" + "A-->B\n" * 8000
reqs.append({"id": 43, "kind": "mermaid", "source": big, "display": True, "theme": theme})
body = "".join(json.dumps(r) + "\n" for r in reqs[:3])
# A line that is not JSON at all: it answers id 0 and the loop survives it.
body += "this is not json\n"
body += "".join(json.dumps(r) + "\n" for r in reqs[3:])
p = subprocess.run(command, input=body, capture_output=True, text=True, timeout=300)
# Sample input: {"id":0,"bundle":"math","from":"bytecode"}, the load report a helper prints under FLEA_FIGURE_REPORT=1; it is not an answer.
raw = p.stdout.splitlines()
parsed = [json.loads(l) for l in raw]
reports = [(a["bundle"], a["from"]) for a in parsed if "bundle" in a]
lines = [a for a in parsed if "bundle" not in a]
# The tag names this run's answers, so a later run over the bytecode can be compared byte for byte.
open(outdir + "/answers-" + os.environ.get("FIG_TAG", "source") + ".txt", "w").write("".join(l + "\n" for l, a in zip(raw, parsed) if "bundle" not in a))
reply_counts = Counter(a["id"] for a in lines)
by_id = {a["id"]: a for a in lines}
fails = []
def check(cond, why):
    print(("PASS " if cond else "FAIL ") + why)
    if not cond:
        fails.append(why)
# The malformed input is one request too, answered once under id 0.
request_counts = Counter(r["id"] for r in reqs)
request_counts[0] += 1
check(reply_counts == request_counts, "every request answered once under its own id")
check(len(lines) == len(reqs) + 1, "reply count equals request count including the malformed line")
expect = os.environ.get("FIG_EXPECT_FROM")
if expect:
    check(reports == [("math", expect), ("mermaid", expect)], "the helper loaded math and mermaid from %s, reported %s" % (expect, reports))
check(p.returncode == 0, "EOF ends the helper with exit 0")
check(p.stderr == "", "a clean run writes nothing on stderr")
forbidden = ["http:", "https:", "@import", "<script", "<image", "foreignObject", "127.0.0.1"]
for i in range(10):
    a = by_id.get(10 + i, {})
    svg = a.get("svg", "")
    check(svg.startswith("<svg"), "formula %d renders an svg" % i)
    check(all(w not in clean(svg) for w in forbidden), "formula %d passes checkSafe" % i)
for i in range(6):
    a = by_id.get(20 + i, {})
    svg = a.get("svg", "")
    check(svg.startswith("<svg"), "diagram %d renders an svg" % i)
    check(all(w not in clean(svg) for w in forbidden), "diagram %d passes checkSafe" % i)
check("error" in by_id.get(30, {}), "the malformed formula answers error")
check("error" in by_id.get(31, {}), "the malformed diagram answers error")
check("error" in by_id.get(0, {}), "a line that is not JSON answers error under id 0")
check(by_id.get(32, {}).get("svg", "").startswith("<svg"), "the loop survives the malformed pair")
check("error" in by_id.get(40, {}), "the hostile href never reaches an svg")
check("error" in by_id.get(41, {}), "the hostile url never reaches an svg")
click = by_id.get(42, {}).get("svg", "")
check(click.startswith("<svg") and "127.0.0.1" not in click, "the hostile click unwraps to no link")
check(by_id.get(43, {}).get("error", "") == "diagram over 32 KiB", "the oversize source is refused")
if not fails:
    open(outdir + "/node-theme.json", "w").write(json.dumps(theme))
    open(outdir + "/node-expected.json", "w").write(json.dumps({"frac": by_id[10]["svg"], "flow": by_id[20]["svg"]}))
print("DONE failures=%d" % len(fails))
sys.exit(1 if fails else 0)
EOF

# The figure tests run under quickjs-ng, the product's engine and the only one the CI image has; node runs them again where it exists.
js_runners=("$qjs")
command -v node >/dev/null && js_runners+=(node)
for runner in "${js_runners[@]}"; do
    for figure_test in figure-cache figure-worker mermaid-syntax mermaid-layout mermaid-fit mermaid-types; do
        echo "markdown-figures.sh: $figure_test under $(basename "$runner")"
        "$runner" "tests/$figure_test.mjs" || exit 1
    done
    # Bytecode is quickjs-ng's own, so node has nothing to run here.
    if [ "$runner" = "$qjs" ]; then
        echo "markdown-figures.sh: figure-bytecode under $(basename "$runner")"
        "$runner" tests/figure-bytecode.mjs || exit 1
    fi
    # A checkout path with a space, a percent sign or a literal %20 must not break module resolution or be decoded, so one test runs from a scratch copy at each.
    for spaced_name in "a tree" "50% tree" "a%20b tree"; do
        spaced="$test_root/$spaced_name"
        mkdir -p "$spaced/ui/js" "$spaced/ui/vendor" "$spaced/tests" || exit 1
        cp ui/js/FigureWorker.mjs "$spaced/ui/js/" && cp ui/vendor/mermaid.mjs "$spaced/ui/vendor/" || exit 1
        cp tests/js-runtime.mjs tests/mermaid-corpus.mjs tests/mermaid-svg.mjs tests/mermaid-layout.mjs "$spaced/tests/" || exit 1
        echo "markdown-figures.sh: mermaid-layout from the path '$spaced_name' under $(basename "$runner")"
        "$runner" "$spaced/tests/mermaid-layout.mjs" || exit 1
    done
done

# Under the launcher, the helper says where each bundle came from; a direct qjs run has no launcher and reports nothing.
report_env=(FLEA_FIGURE_REPORT=1)
[ "$jailed" -eq 1 ] && source_expect=(FIG_EXPECT_FROM=source) || source_expect=()
if ! env FLEA_QJS="$qjs" "${report_env[@]}" "${source_expect[@]}" python3 "$test_root/drive.py" "${engine[@]}" "$test_root"; then
    echo "markdown-figures.sh: the helper run failed"
    exit 1
fi
# The same corpus through the bytecode path: the real compile jail builds the cache, then the helper answers the same bytes from it; with no jail the SKIP above already said so.
if [ "$jailed" -eq 1 ]; then
    mkdir -p "$test_root/home" || exit 1
    cached=(env -u FLEA_FIGURE_CACHE HOME="$test_root/home" XDG_CACHE_HOME="$test_root/cache" FLEA_QJS="$qjs")
    # The launcher makes the cache root before it starts a build, so the foreground build is given one too.
    mkdir -p "$test_root/cache/flea/figures" || exit 1
    "${cached[@]}" "$fleabin" --figure-compile || {
        echo "markdown-figures.sh: FAIL the compile jail built no cache"
        exit 1
    }
    ls "$test_root"/cache/flea/figures/*/manifest > /dev/null 2>&1 || {
        echo "markdown-figures.sh: FAIL no verified bytecode directory after the compile"
        exit 1
    }
    echo "PASS the compile jail built a bytecode directory"
    if ! env "${report_env[@]}" FIG_EXPECT_FROM=bytecode FIG_TAG=bytecode python3 "$test_root/drive.py" "${cached[@]}" "$fleabin" --figure-helper "$test_root"; then
        echo "markdown-figures.sh: the bytecode helper run failed"
        exit 1
    fi
    # The bytecode leg's own assertion must have teeth: a helper that ran from source, which answers the same bytes, is refused by it.
    if env FLEA_QJS="$qjs" "${report_env[@]}" FIG_EXPECT_FROM=bytecode FIG_TAG=control python3 "$test_root/drive.py" "${engine[@]}" "$test_root" > "$test_root/control.log" 2>&1; then
        echo "markdown-figures.sh: FAIL a helper that ran from source passed the bytecode leg's assertion"
        exit 1
    fi
    grep -q 'FAIL the helper loaded math and mermaid from bytecode' "$test_root/control.log" || {
        echo "markdown-figures.sh: FAIL the control run failed for another reason than the load report"
        exit 1
    }
    echo "PASS the bytecode leg refuses a helper that answered from source"
    cmp -s "$test_root/answers-source.txt" "$test_root/answers-bytecode.txt" || {
        echo "markdown-figures.sh: FAIL the bytecode path answered other bytes than the source path"
        exit 1
    }
    echo "PASS the bytecode path answers the source path's bytes for the corpus"
fi

# A missing engine refuses exactly, with both sandbox tools present on a controlled PATH.
mkdir -p "$test_root/sandbox-tools" "$test_root/no-sandbox" || exit 1
for tool in bwrap prlimit; do
    tool_path=$(command -v "$tool") || {
        echo "markdown-figures.sh: FAIL missing required tool $tool"
        exit 1
    }
    ln -sf "$tool_path" "$test_root/sandbox-tools/$tool" || exit 1
done
PATH="$test_root/sandbox-tools" FLEA_QJS="$test_root/no-such-qjs" "$fleabin" --figure-helper < /dev/null > "$test_root/missing-qjs.stdout" 2> "$test_root/missing-qjs.stderr"
missing_rc=$?
[ "$missing_rc" -eq 127 ] || {
    echo "markdown-figures.sh: FAIL missing qjs exited $missing_rc, want 127"
    exit 1
}
missing_expected="flea: the figure helper needs quickjs-ng at $test_root/no-such-qjs, and it is missing"
printf '%s\n' "$missing_expected" > "$test_root/missing-qjs.expected"
if [ -s "$test_root/missing-qjs.stdout" ] || ! cmp -s "$test_root/missing-qjs.expected" "$test_root/missing-qjs.stderr"; then
    echo "markdown-figures.sh: FAIL missing qjs did not print the exact refusal on stderr"
    exit 1
fi
echo "PASS $missing_expected (exit 127)"

# The missing sandbox has its own refusal branch, with a present engine.
PATH="$test_root/no-sandbox" FLEA_QJS="$qjs" "$fleabin" --figure-helper < /dev/null > "$test_root/missing-sandbox.stdout" 2> "$test_root/missing-sandbox.stderr"
sandbox_rc=$?
[ "$sandbox_rc" -eq 127 ] || {
    echo "markdown-figures.sh: FAIL missing sandbox exited $sandbox_rc, want 127"
    exit 1
}
sandbox_expected="flea: the figure helper needs bwrap and prlimit, and one of them is missing"
printf '%s\n' "$sandbox_expected" > "$test_root/missing-sandbox.expected"
if [ -s "$test_root/missing-sandbox.stdout" ] || ! cmp -s "$test_root/missing-sandbox.expected" "$test_root/missing-sandbox.stderr"; then
    echo "markdown-figures.sh: FAIL missing sandbox did not print the exact refusal on stderr"
    exit 1
fi
echo "PASS $sandbox_expected (exit 127)"

# Byte identity against node, the engine the bundles were built for. Loud skip when absent.
if command -v node >/dev/null; then
# Build identity imports as file URLs so checkout punctuation cannot affect substitution or module resolution.
identity_root=$(python3 -c 'import pathlib
import sys
print(pathlib.Path(sys.argv[1]).as_uri())' "$PWD") || exit 1
{
printf 'import { renderFigure } from "%s/ui/js/FigureWorker.mjs";\n' "$identity_root"
printf 'import { texToSvg } from "%s/ui/vendor/math.mjs";\n' "$identity_root"
printf 'import { mermaidToSvg } from "%s/ui/vendor/mermaid.mjs";\n' "$identity_root"
cat <<'EOF'
import { readFileSync, writeFileSync } from "node:fs";
const theme = JSON.parse(readFileSync(process.argv[2], "utf8"));
const frac = renderFigure("math", "\\frac{a}{b}", true, theme, { texToSvg });
let flow = renderFigure("mermaid", "flowchart TD\n    A --> B", true, theme, { mermaidToSvg });
if (flow && flow.then)
    flow = await flow;
writeFileSync(process.argv[3], JSON.stringify({ frac, flow }));
EOF
} > "$test_root/identity.mjs"
node "$test_root/identity.mjs" "$test_root/node-theme.json" "$test_root/node-actual.json" || {
    echo "markdown-figures.sh: FAIL node could not render"
    exit 1
}
python3 - "$test_root/node-expected.json" "$test_root/node-actual.json" <<'EOF' || {
import json, sys
want = json.load(open(sys.argv[1]))
got = json.load(open(sys.argv[2]))
assert want == got, "qjs and node disagree on rendered bytes"
print("PASS qjs renders the bundles byte-identical to node")
EOF
    echo "markdown-figures.sh: FAIL qjs and node disagree on rendered bytes"
    exit 1
}

else
    echo "markdown-figures.sh: SKIP node is absent, so the byte-identity check did not run"
fi

# FigureService drives the same selected engine for cache, idle exit, timeout restart, the 127 latch and the fence; a missing qs refuses the suite.
if ! command -v qs >/dev/null; then
    echo "markdown-figures.sh: FAIL missing required tool qs"
    exit 1
fi
mkdir -p "$test_root/qsconfig" || exit 1
ln -s "$PWD/ui" "$test_root/qsconfig/flea" || exit 1
headless_commons "$test_root" "$test_root/qsconfig/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/qsconfig/Ui" || exit 1
cp tests/markdown-figures.qml "$test_root/qsconfig/shell.qml" || exit 1
cp -R tests/figure-memory "$test_root/qsconfig/figure-memory" || exit 1
mkdir -p "$test_root/stubbin" || exit 1
printf 'answer\n' > "$test_root/phase" || exit 1
# With FLEA_BIN unset the service's stub hangs, refuses or execs the selected engine according to the phase file, and its store mode ends at once so the persistent cache latches off.
printf -v engine_exec '%q ' "${engine[@]}"
cat > "$test_root/stubbin/flea" <<EOF
#!/bin/bash
[ "\$1" = "--figure-store" ] && exit 127
if [ "\$1" = "--figure-helper" ]; then
    phase=\$(cat "$test_root/phase" 2>/dev/null)
    case "\$phase" in
        hang)
            mkfifo "$test_root/hang.pipe" || exit 1
            exec {hang_fd}<>"$test_root/hang.pipe"
            printf '%s\n' "\$\$" > "$test_root/hang.pid"
            printf 'FIGHANG pid=%s\n' "\$\$"
            read -r unused <&\$hang_fd
            ;;
        exit42) exit 42 ;;
        refused)
            echo "flea: stub has no engine" >&2
            exit 127
            ;;
        *) exec $engine_exec ;;
    esac
fi
echo "stub flea: unexpected argv \$*" >&2
exit 2
EOF
chmod +x "$test_root/stubbin/flea" || exit 1
output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_BIN \
    HOME="$test_root" XDG_STATE_HOME="$test_root" XDG_CACHE_HOME="$test_root" \
    XDG_RUNTIME_DIR="$test_root" FLEA_QJS="$qjs" \
    FLEA_FIG_PHASE_FILE="$test_root/phase" PATH="$test_root/stubbin:$PATH" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout 150 qs -p "$test_root/qsconfig" 2>&1 ) 2>/dev/null )
qs_status=$?
if [ -f "$test_root/hang.pid" ]; then
    pgrep -F "$test_root/hang.pid" > /dev/null
    fixture_status=$?
    if [ "$fixture_status" -ne 1 ]; then
        echo "markdown-figures.sh: FAIL hanging fixture pid check exited $fixture_status, want 1 after reaping"
        exit 1
    fi
    printf 'PASS hanging fixture pid=%s reaped (pgrep exit=1)\n' "$(cat "$test_root/hang.pid")"
else
    echo "markdown-figures.sh: FAIL no hanging fixture pid receipt"
    printf 'markdown-figures.sh: qs exited %s\n' "$qs_status"
    printf '%s\n' "$output" | grep -aE 'MARKDOWN_FIGURES FAIL|TypeError|ReferenceError|RangeError|ERROR' | head -10
    exit 1
fi
pass_count=$(printf '%s\n' "$output" | grep -c 'MARKDOWN_FIGURES PASS')
fail_count=$(printf '%s\n' "$output" | grep -c 'MARKDOWN_FIGURES FAIL')
done_count=$(printf '%s\n' "$output" | grep -c 'MARKDOWN_FIGURES DONE')
verdict=0
if [ "$qs_status" -ne 143 ]; then
    printf 'FAIL qs exited %s, want the owned self-kill 143 after DONE\n' "$qs_status"
    verdict=1
fi
if [ "$done_count" -ne 1 ]; then
    printf 'FAIL completion receipts %s, want exactly 1 DONE beside the PASS lines\n' "$done_count"
    verdict=1
fi
if [ "$fail_count" -ne 0 ]; then
    printf 'FAIL %s FigureService check(s) failed\n' "$fail_count"
    verdict=1
fi
if [ "$pass_count" -lt 10 ]; then
    printf 'FAIL only %s PASS lines, want at least 10 (answers, cache, idle, timeout, latch, fence)\n' "$pass_count"
    verdict=1
fi
platform_warning='This plugin does not support setting window masks'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN' | grep -vF "$platform_warning")
if [ -n "$warnings" ]; then
    printf 'FAIL the FigureService harness logged a warning\n'
    printf '%s\n' "$warnings" | head -10
    verdict=1
fi
if [ "$verdict" -ne 0 ]; then
    printf '%s\n' "$output" | grep -a 'MARKDOWN_FIGURES FAIL' | head -30
    printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|RangeError|ERROR' | head -6
    exit 1
fi
# The GUI memory claim: judge Rss growth from each stamped read, with PSS, Anonymous and the helper peak printed as evidence.
FIG_RSS_BUDGET_KB=10240
fig_pss=$(printf '%s\n' "$output" | grep -a 'MARKDOWN_FIGURES FIGPSS')
fig_peak=$(printf '%s\n' "$output" | grep -a 'MARKDOWN_FIGURES FIGHELPER')
printf '%s\n' "$fig_pss" "$fig_peak"
# Sample input: MARKDOWN_FIGURES FIGPSS phase=before pss_kb=45120 read_seq=3 anonymous_kb=32200 rss_kb=60100.
fig_val() {
    printf '%s\n' "$fig_pss" | sed -n "s/.*phase=$1 .*${2:-pss_kb}=\([0-9][0-9]*\).*/\1/p"
}
# Sample input: MARKDOWN_FIGURES FIGPSS phase=formulas pss_kb=45120 read_seq=4 anonymous_kb=32200 rss_kb=60100.
fig_stamp() {
    printf '%s\n' "$fig_pss" | sed -n "s/.*phase=$1 pss_kb=[0-9][0-9]* read_seq=\([0-9][0-9]*\).*/\1/p"
}
previous_stamp=0
for phase in before formulas diagrams idle; do
    [ -n "$(fig_val "$phase")" ] || {
    echo "markdown-figures.sh: FAIL no FIGPSS $phase line"
    verdict=1
}
    [ -n "$(fig_val "$phase" anonymous_kb)" ] || {
    echo "markdown-figures.sh: FAIL no FIGPSS $phase Anonymous value"
    verdict=1
}
    [ -n "$(fig_val "$phase" rss_kb)" ] || {
    echo "markdown-figures.sh: FAIL no FIGPSS $phase Rss value"
    verdict=1
}
    stamp=$(fig_stamp "$phase")
    if [ -z "$stamp" ]; then
        echo "markdown-figures.sh: FAIL no FIGPSS $phase read stamp"
        verdict=1
    elif [ "$stamp" -le "$previous_stamp" ]; then
        echo "markdown-figures.sh: FAIL FIGPSS $phase read stamp is stale"
        verdict=1
    else
        previous_stamp=$stamp
    fi
done
printf '%s\n' "$fig_peak" | grep -q 'rss_peak_kb=[0-9]' || {
    echo "markdown-figures.sh: FAIL no FIGHELPER peak line"
    verdict=1
}
if [ -n "$(fig_val before rss_kb)" ] && [ -n "$(fig_val formulas rss_kb)" ]; then
    [ "$(fig_val formulas rss_kb)" -le "$(( $(fig_val before rss_kb) + FIG_RSS_BUDGET_KB ))" ] || {
    echo "markdown-figures.sh: FAIL formulas Rss exceeds before by more than $FIG_RSS_BUDGET_KB kB"
    verdict=1
}
fi
if [ -n "$(fig_val before rss_kb)" ] && [ -n "$(fig_val diagrams rss_kb)" ]; then
    [ "$(fig_val diagrams rss_kb)" -le "$(( $(fig_val before rss_kb) + FIG_RSS_BUDGET_KB ))" ] || {
    echo "markdown-figures.sh: FAIL diagrams Rss exceeds before by more than $FIG_RSS_BUDGET_KB kB"
    verdict=1
}
fi
if [ "$verdict" -ne 0 ]; then
    printf '%s\n' "$output" | grep -a 'MARKDOWN_FIGURES FAIL' | head -30
    exit 1
fi
printf 'MARKDOWN_FIGURES %s check(s), %s failed\n' "$pass_count" "$fail_count"
printf '%s\n' "$output" | grep -o 'MARKDOWN_FIGURES DONE.*'
echo "MARKDOWN_FIGURES DONE failures=0"
