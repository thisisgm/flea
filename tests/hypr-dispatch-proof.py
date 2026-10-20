#!/usr/bin/env python3
"""Run typed helpers and actual harness source statements without a compositor."""
import ast
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

WARNING = "warning: =[C]:-1: hl.focus: window not found"
PARSER_SAMPLE_PREFIX = "# Sample input: "
HELPER_TIMEOUT_SECONDS = 10
FAKE_EXECUTABLE_MODE = 0o755
FAILED_COMMAND_STATUS = 7
PLACEMENT_FIRST_WINDOW_CALL = 2
PLACEMENT_CALL_COUNT = 4
STANDALONE_HARNESS_COUNT = 9
PLACEMENT_FLOAT_INDEX = 1
PLACEMENT_RESIZE_INDEX = 2
TEST_PID = 111
OTHER_PID = 999
PLACEMENT_X = 40
PLACEMENT_Y = 80
PLACEMENT_WIDTH = 1000
PLACEMENT_HEIGHT = 720
SHARED_HELPER_FILE = "tests/lib/hypr-dispatch.sh"
OPERATIONS = ("workspace_focus", "window_focus", "window_float", "window_resize", "window_move", "window_resize_absolute",
              "window_move_absolute", "window_opacity", "cursor_move", "session_exec_script",
              "session_exec_rules", "window_send_shortcut",
              "window_rounding", "output_create", "output_remove", "monitor_mode", "query_monitors", "query_clients",
              "query_cursor", "query_option", "query_version", "query_help", "query_output_help")
ENTRY_POINTS = tuple("hypr_" + operation for operation in OPERATIONS)
RULE_REPLIES = ("error: =[C]:-1: buildRuleFromTable: unknown effect 'bogus'",
                "error: =[C]:-1: buildRuleFromTable: failed to build table for exec rules from argument")
LUA_PREFIX = "hl." + "dsp."
FAKE = r'''#!/usr/bin/env bash
set -u
case "$1" in
    clients) printf '%s\n' "$HYPR_FAKE_CLIENTS" ;;
    activewindow) cat "$HYPR_FAKE_ACTIVE" ;;
    -j)
        printf '%s\n' "$*" >> "$HYPR_FAKE_LOG"
        case "$2" in
            monitors) printf '%s\n' '[{"name":"FLEA-fixture","id":7}]' ;;
            clients) printf '%s\n' '[{"pid":111,"address":"0xabc"}]' ;;
            cursorpos) printf '%s\n' '{"x":1280,"y":720}' ;;
            getoption) printf '%s\n' '{"option":"decoration:rounding","int":8}' ;;
            *) printf 'fake hyprctl refused unexpected query: %s\n' "$*" >&2; exit 2 ;;
        esac
        exit "${HYPR_FAKE_STATUS:-0}"
        ;;
    version) printf '%s\n' "$*" >> "$HYPR_FAKE_LOG"; printf '%s\n' 'Hyprland 0.56.2 (fake)'; exit "${HYPR_FAKE_STATUS:-0}" ;;
    --help) printf '%s\n' "$*" >> "$HYPR_FAKE_LOG"; printf '%s\n' 'usage: hyprctl command'; exit "${HYPR_FAKE_STATUS:-0}" ;;
    output)
        printf '%s\n' "$*" >> "$HYPR_FAKE_LOG"
        if [[ "$2" == --help ]]; then printf '%s\n' 'usage: hyprctl output create|remove'; exit "${HYPR_FAKE_STATUS:-0}"; fi
        printf '%s\n' "${HYPR_FAKE_REPLY-ok}"
        exit "${HYPR_FAKE_STATUS:-0}"
        ;;
    dispatch|eval)
        printf '%s\n' "$2" >> "$HYPR_FAKE_LOG"
        if [[ -n "${HYPR_FAKE_FAIL_ACTION:-}" && "$2" == *"$HYPR_FAKE_FAIL_ACTION"* ]]; then
            printf '%s\n' "$HYPR_FAKE_WARNING"
        else
            if [[ "$2" == *"$HYPR_FAKE_FOCUS_PREFIX"* ]]; then
                printf '{"pid":%s}\n' "$HYPR_FAKE_TEST_PID" > "$HYPR_FAKE_ACTIVE"
            fi
            printf '%s\n' "${HYPR_FAKE_REPLY-ok}"
        fi
        exit "${HYPR_FAKE_STATUS:-0}"
        ;;
    *)
        printf 'fake hyprctl refused unexpected command: %s\n' "$*" >&2
        exit 2
        ;;
esac
'''


# Sample input: "xwdrag_place() {\n    hypr_window_focus 0xabc\n}" with names=["xwdrag_place"].
def functions(text, names):
    pattern = r"(?ms)^(?:" + "|".join(names) + r")\(\) \{\n.*?^\}"
    return "\n".join(match.group() for match in re.finditer(pattern, text))


# Sample input: cd "$(dirname "$0")/.." then repo=$(pwd) followed by . "$repo/tests/lib/hypr-dispatch.sh".
def helper_source_block(text):
    setup = []
    sources = []
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith('cd "$(dirname "$0")/.."') and not sources:
            setup.append(line)
        if stripped.startswith("repo=") and not sources:
            setup.append(line)
        if stripped.startswith((". ", "source ", ".\t", "source\t")) and "hypr-dispatch.sh" in line:
            sources.append(line)
    if len(sources) != 1:
        return None
    return "\n".join(setup + sources)


# Sample input: "hypr_window_focus 39 /repo/tests/lib/hypr-dispatch.sh" from declare -F with extdebug.
def source_origins(output):
    origins = {}
    for line in output.splitlines():
        name, _, path = line.split(maxsplit=2)
        origins[name] = Path(path).resolve()
    return origins


# Sample input: "hypr_window_focus() {" or "function hypr_dispatch() {".
def helper_definitions(text):
    names = (*ENTRY_POINTS, "hypr_dispatch")
    pattern = r"^[ \t]*(?:function[ \t]+)?(" + "|".join(names) + r")[ \t]*\([ \t]*\)[ \t]*\{"
    return re.findall(pattern, text, re.MULTILINE)


# Sample output: hypr_window_move 0xabc 40 80: the compositor answered "window not found" (exit 0)
def refusal(operation, arguments, reply, status):
    return f'hypr_{operation} {" ".join(arguments)}: the compositor answered "{reply}" (exit {status})\n'


def main():
    root = Path(__file__).resolve().parent.parent
    helper = (root / SHARED_HELPER_FILE).resolve(strict=True)
    source = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / "tests/ui.sh"
    text = source.read_text()
    harnesses = [path for path in sorted((root / "tests").rglob("*.sh"))
                 if path != helper and helper_source_block(path.read_text()) is not None]
    harnesses = [source if path == root / "tests/ui.sh" else path for path in harnesses]
    cases = json.loads((root / "tests/fixtures/hypr-dispatch-typed.json").read_text())
    failures = []
    checks = 0
    with tempfile.TemporaryDirectory(prefix="hypr-proof-", dir=os.environ.get("TMPDIR")) as directory:
        scratch = Path(directory)
        fake = scratch / "hyprctl"
        fake.write_text(FAKE)
        fake.chmod(FAKE_EXECUTABLE_MODE)
        environment = dict(os.environ, PATH=str(scratch) + os.pathsep + os.environ["PATH"])
        environment.update(HYPR_FAKE_LOG=str(scratch / "dispatch.log"), HYPR_FAKE_ACTIVE=str(scratch / "active.json"),
                           HYPR_FAKE_WARNING=WARNING, HYPR_FAKE_FOCUS_PREFIX=LUA_PREFIX + "focus",
                           HYPR_FAKE_TEST_PID=str(TEST_PID))
        environment["HYPR_FAKE_CLIENTS"] = json.dumps([{"pid": TEST_PID, "address": "0xabc"}])
        for name in ("HYPRLAND_INSTANCE_SIGNATURE", "WAYLAND_DISPLAY", "DISPLAY"):
            environment.pop(name, None)

        def run(command, code="", harness=source, **overrides):
            (scratch / "dispatch.log").write_text("")
            (scratch / "active.json").write_text(json.dumps({"pid": OTHER_PID}) + "\n")
            script = "set -euo pipefail\n" + code + "\n" + command
            result = subprocess.run(["bash", "-c", script, str(harness)], cwd=harness.parent,
                                    env=dict(environment, **overrides), text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=HELPER_TIMEOUT_SECONDS)
            calls = (scratch / "dispatch.log").read_text().splitlines()
            return result.returncode, result.stdout, calls

        def check(name, holds, output=""):
            nonlocal checks
            checks += 1
            if not holds:
                failures.append(name)
                print(f"FAIL hypr-dispatch-proof {name}: {output.strip()}")

        def source_check(harness):
            block = helper_source_block(harness.read_text())
            if block is None:
                return False, "missing or ambiguous helper source"
            rc, output, calls = run("shopt -s extdebug\ndeclare -F " + " ".join(ENTRY_POINTS), code=block, harness=harness)
            if rc != 0 or calls:
                return False, output
            try:
                origins = source_origins(output)
            except ValueError:
                return False, output
            return origins == dict.fromkeys(ENTRY_POINTS, helper), output

        check("all standalone shell harnesses have a helper source", len(harnesses) == STANDALONE_HARNESS_COUNT)
        for harness in harnesses:
            valid, output = source_check(harness)
            check(harness.name + " executes its own source and defines repository entry points", valid, output)
            block = helper_source_block(harness.read_text())
            if block is None:
                continue
            for label, statement in (
                ("missing", '. "/nonexistent/lib/hypr-dispatch.sh"'),
                ("foreign", ". " + shlex.quote(str(scratch / "foreign/lib/hypr-dispatch.sh"))),
                ("commented", '# . "/nonexistent/lib/hypr-dispatch.sh"'),
                ("source-keyword", "\tsource " + shlex.quote(str(helper))),
                ("source-comment", '. "unrelated.sh" # "/nonexistent/lib/hypr-dispatch.sh"'),
            ):
                foreign = scratch / "foreign/lib/hypr-dispatch.sh"
                foreign.parent.mkdir(parents=True, exist_ok=True)
                foreign.write_text(helper.read_text())
                copy = scratch / (harness.stem + "-" + label + ".sh")
                copy.write_text(harness.read_text().replace(block.splitlines()[-1], statement))
                valid, output = source_check(copy)
                check(harness.name + " source control " + label, valid == (label == "source-keyword"), output)
                if label == "missing":
                    print(f"source control {harness.name}: /nonexistent/lib/hypr-dispatch.sh rejected={not valid}")

        for relative, names in (("tests/hyprdispatch.py", ("RAW_WORD", "HYPRCTL_WORD", "CONTINUED_LUA_PREFIX", "scan")),
                                ("tests/hypr-dispatch-proof.py", ("functions", "helper_source_block", "source_origins", "helper_definitions"))):
            parser_source = (root / relative).read_text()
            source_lines = parser_source.splitlines()
            definitions = {node.name: node for node in ast.walk(ast.parse(parser_source))
                           if isinstance(node, ast.FunctionDef)}
            for node in ast.walk(ast.parse(parser_source)):
                if isinstance(node, ast.Assign):
                    for target in node.targets:
                        if isinstance(target, ast.Name):
                            definitions[target.id] = node
            for name in names:
                definition = definitions.get(name)
                documented = definition is not None and definition.lineno > 1
                if documented:
                    documented = source_lines[definition.lineno - 2].strip().startswith(PARSER_SAMPLE_PREFIX)
                check("sample input directly above " + name, documented, relative)

        shared_code = ". " + shlex.quote(str(helper))
        rc, output, calls = run("compgen -A function hypr_", code=shared_code)
        check("only supported typed entry points exist",
              rc == 0 and set(output.splitlines()) == set(ENTRY_POINTS) and not calls, output)
        # Sample input: window_focus) hypr_window_focus "$@" ;; from the program case.
        program_operations = re.findall(r"^        ([a-z_]+)\) hypr_", helper.read_text(), re.MULTILINE)
        check("only supported program operations exist", tuple(program_operations) == OPERATIONS,
              ", ".join(program_operations))
        check("typed fixtures cover only supported operations",
              {case["operation"] for case in cases} == set(OPERATIONS))
        scanner = ast.parse((root / "tests/hyprdispatch.py").read_text())
        stdin_reads = [node for node in ast.walk(scanner) if isinstance(node, ast.Attribute)
                       and isinstance(node.value, ast.Name) and node.value.id == "sys" and node.attr == "stdin"]
        check("scanner has no stdin mode", not stdin_reads)
        definition_owners = {name: [] for name in (*ENTRY_POINTS, "hypr_dispatch")}
        for path in sorted((root / "tests").rglob("*.sh")):
            harness_text = text if path == root / "tests/ui.sh" else path.read_text()
            for name in helper_definitions(harness_text):
                definition_owners[name].append(path.relative_to(root).as_posix())
        for name, owners in definition_owners.items():
            expected = [] if name == "hypr_dispatch" else [SHARED_HELPER_FILE]
            check(name + " definition ownership", owners == expected, ", ".join(owners))
        rc, output, calls = run("! declare -F hypr_dispatch", code=shared_code)
        check("no free-form dispatch entry point", rc == 0 and not output and not calls, output)
        for case in cases:
            operation = case["operation"]
            arguments = case["args"]
            query = operation.startswith("query_")
            command = shlex.join(["hypr_" + operation, *arguments])
            rc, output, calls = run(command, code=shared_code)
            check(operation + " exact text " + repr(arguments), rc == 0 and output == case.get("reply", "") and calls == [case["text"]], output)
            program = shlex.join(["bash", str(helper), operation, *arguments])
            rc, output, calls = run(program)
            check(operation + " program form", rc == 0 and output == case.get("reply", "ok\n") and calls == [case["text"]], output)
            for form, code, invoke in (("function", shared_code, command), ("program", "", program)):
                if query:
                    # A query has no ok reply to check, so its returned status is the whole failure contract.
                    rc, output, calls = run(invoke, code=code, HYPR_FAKE_STATUS=str(FAILED_COMMAND_STATUS))
                    check(operation + " " + form + " propagates a failed query", rc != 0 and calls == [case["text"]], output)
                    continue
                for reply in (WARNING, "", "ok\nwarning", " ok", "ok ", "okay",
                              *(RULE_REPLIES if operation == "session_exec_rules" else ())):
                    rc, output, calls = run(invoke, code=code, HYPR_FAKE_REPLY=reply)
                    check(operation + " " + form + " rejects reply " + repr(reply),
                          rc == 1 and output == refusal(operation, arguments, reply, 0) and calls == [case["text"]], output)
                rc, output, calls = run(invoke, code=code, HYPR_FAKE_STATUS=str(FAILED_COMMAND_STATUS))
                check(operation + " " + form + " rejects failed command with ok",
                      rc == 1 and output == refusal(operation, arguments, "ok", FAILED_COMMAND_STATUS)
                      and calls == [case["text"]], output)
            invalid = [arguments[:-1], arguments + ["extra"]] if arguments else [["extra"]]
            if operation not in ("cursor_move", "session_exec_script", "session_exec_rules"):
                invalid.extend([bad, *arguments[1:]] for bad in
                               ("flea", "abc", '0xab"c', "0xab}c", "address:0xabc", "", "0Xabc", "0x"))
            if operation == "session_exec_script":
                invalid.extend([bad] for bad in ("", "run.sh", "./run.sh", "/tmp/a b.sh", "/tmp/a'bad.sh",
                               '/tmp/a"bad.sh', "/tmp/a;exit.sh", "/tmp/$(id).sh", "/tmp/`id`.sh", "/tmp/a\nb.sh"))
            if operation == "session_exec_rules":
                invalid.extend([bad, arguments[1], arguments[2]] for bad in
                               ("", "a b", "a;exit", 'a"b', "a'b", "a\\b", "$(id)", "a`id`", "a\nb"))
                invalid.extend([arguments[0], bad, arguments[2]] for bad in
                               ("DP-2", "FLEA-", "FLEA-a b", 'FLEA-"x', "FLEA-a;exit", "", "HDMI-A-1"))
                invalid.extend([arguments[0], arguments[1], bad] for bad in
                               ("", "run.sh", "./run.sh", "/tmp/a b.sh", "/tmp/a'bad.sh", "/tmp/a;exit.sh", "/tmp/$(id).sh"))
            integer_start = 0 if operation in ("workspace_focus", "cursor_move") else 1
            if operation in ("workspace_focus", "window_resize", "window_move", "window_resize_absolute", "window_move_absolute", "cursor_move",
                             "window_rounding"):
                for index in range(integer_start, len(arguments)):
                    for bad in ("1.5", "one", "1;exit", "1}", "+1", ""):
                        invalid.append(arguments[:index] + [bad] + arguments[index + 1:])
                    if operation in ("window_resize", "window_resize_absolute"):
                        invalid.append(arguments[:index] + ["-1"] + arguments[index + 1:])
            if operation == "window_send_shortcut":
                invalid.extend([arguments[0], bad, arguments[2]] for bad in ("ctrl", "SHIFT;exit", 'SUPER"', "Super", "SHIFT,SUPER"))
                invalid.extend([arguments[0], arguments[1], bad] for bad in ("", "Return;exit", 'R"', "a b", "a,b"))
            if operation == "monitor_mode":
                invalid.extend([arguments[0], bad] for bad in ("2560x1440", "2560x1440@", "wide", "2560x1440@60;exit", ""))
            if operation == "window_float":
                invalid.extend([arguments[0], bad] for bad in ("ON", "toggle", 'on"', ""))
            if operation == "window_opacity":
                invalid.extend([arguments[0], bad, arguments[2]] for bad in
                               ("bordercolor", "opacity;exit", 'opacity"', "", "opacity\n"))
                bad_values = ("0", "-0.1", "1.1", "1.0", "nan", "0.5;exit", '0.5"', "")
                if arguments[1].endswith("override"):
                    bad_values += ("0.5", "0.96")
                invalid.extend([arguments[0], arguments[1], bad] for bad in bad_values)
            if operation == "query_option":
                # Option paths reach hyprctl as a word, so a separator, a quote or a substitution must refuse before it runs.
                invalid.extend([bad] for bad in ("decoration:rounding;exit", 'decoration:"rounding"', "decoration:rounding}",
                                                 "decoration:rounding)", "decoration rounding", "decoration:$(id)",
                                                 "decoration:rounding\nexit", "decoration", ":rounding", "decoration:",
                                                 "Decoration:rounding"))
            for bad_arguments in invalid:
                for prefix, code in ((["hypr_" + operation], shared_code), (["bash", str(helper), operation], "")):
                    rc, output, calls = run(shlex.join(prefix + bad_arguments), code=code)
                    check(operation + " refuses " + repr(bad_arguments),
                          rc == 1 and "hypr_" + operation + ": refused argument " in output and not calls, output)

        glass_helpers = shared_code + "\n" + functions((root / "tests/ui-captures-glass.sh").read_text(), ["glasspick_opacity"])
        glass_helpers += r'''
fail() { printf '%s\n' "$*"; exit 1; }
glasspick_address() { printf '0xabc\n'; }
flea_process_owned() {
    [[ "$1" == 111 && "$(wc -l < "$HYPR_FAKE_LOG")" -lt "${HYPR_FAKE_OWNER_LIMIT:-4}" ]]
}
'''
        opacity_call = f"glasspick_opacity {TEST_PID} 0.985 0.96"
        expected_properties = (("opacity", "0.985"), ("opacity_inactive", "0.96"),
                               ("opacity_override", "1"), ("opacity_inactive_override", "1"))
        expected_opacity = [LUA_PREFIX + 'window.set_prop({window = "address:0xabc", prop = "' + prop
                            + '", value = "' + value + '"})' for prop, value in expected_properties]
        rc, output, calls = run(opacity_call, code=glass_helpers)
        check("Glass opacity keeps its exact four fixed writes", rc == 0 and calls == expected_opacity, output)
        for limit in range(len(expected_opacity)):
            rc, output, calls = run(opacity_call, code=glass_helpers, HYPR_FAKE_OWNER_LIMIT=str(limit))
            check("Glass opacity rechecks PID ownership before write " + str(limit + 1),
                  rc == 1 and "lost ownership before opacity" in output and calls == expected_opacity[:limit], output)
        for status, reply in (("0", WARNING), (str(FAILED_COMMAND_STATUS), "ok")):
            rc, output, calls = run(opacity_call, code=glass_helpers, HYPR_FAKE_STATUS=status, HYPR_FAKE_REPLY=reply)
            check("Glass opacity stops on the first refused reply " + repr((status, reply)),
                  rc == 1 and "cannot set opacity on owned fixture" in output and calls == expected_opacity[:1], output)
        for active, inactive in (("1.1", "0.96"), ("0.985", "bad"), ('0.5"', "1")):
            rc, output, calls = run(shlex.join(["glasspick_opacity", str(TEST_PID), active, inactive]), code=glass_helpers)
            check("Glass opacity refuses invalid alpha before any write " + repr((active, inactive)),
                  rc == 1 and "invalid fixture opacity" in output and not calls, output)

        for arguments in ([], ["dispatch", "arbitrary Lua"], ["unknown"]):
            rc, output, calls = run(shlex.join(["bash", str(helper), *arguments]))
            check("program refuses unsupported input " + repr(arguments),
                  rc == 1 and "hypr-dispatch.sh: refused argument " in output and not calls, output)

        block = helper_source_block(text)
        helpers = (block or "false") + "\n" + functions(text, ["fail", "xwdrag_addr", "xwdrag_focus", "xwdrag_assert_focus", "xwdrag_place"])
        helpers += "\nsleep() {\n    :\n}\n"
        rc, output, calls = run(f"xwdrag_focus {TEST_PID}", code=helpers)
        expected_focus = LUA_PREFIX + 'focus({ window = "address:0xabc" })'
        check("addressed focus reaches the wanted PID", rc == 0 and calls == [expected_focus], output)
        placement = f"xwdrag_place {TEST_PID} {PLACEMENT_X} {PLACEMENT_Y} {PLACEMENT_WIDTH} {PLACEMENT_HEIGHT}"
        rc, output, calls = run(placement, code=helpers)
        expected_actions = [LUA_PREFIX + action for action in ("focus", "window.float", "window.resize", "window.move")]
        check("addressed placement keeps focus, float, resize, move order",
              rc == 0 and len(calls) == PLACEMENT_CALL_COUNT and all(call.startswith(action + "(")
              for call, action in zip(calls, expected_actions)), output)
        check("every placement call names the owned window", len(calls) == PLACEMENT_CALL_COUNT
              and all('window = "address:0xabc"' in call for call in calls), "\n".join(calls))
        check("floating is on and resize is exact", len(calls) == PLACEMENT_CALL_COUNT
              and 'action = "on"' in calls[PLACEMENT_FLOAT_INDEX]
              and "exact = true" in calls[PLACEMENT_RESIZE_INDEX], "\n".join(calls))
        for index, action in enumerate(("window.float", "window.resize", "window.move"), start=PLACEMENT_FIRST_WINDOW_CALL):
            rc, output, calls = run(placement, code=helpers, HYPR_FAKE_FAIL_ACTION=action)
            check(action + " stops placement on an exit-zero warning", rc == 1 and WARNING in output and len(calls) == index, output)
        for name, clients in (("missing", "[]"), ("ambiguous", json.dumps([{"pid": TEST_PID, "address": "0xabc"},
                              {"pid": TEST_PID, "address": "0xdef"}])), ("malformed", "not json")):
            rc, output, calls = run(f"xwdrag_addr {TEST_PID}", code=helpers, HYPR_FAKE_CLIENTS=clients)
            check(name + " address lookup fails loud", rc != 0 and "xwdrag: no " in output and not calls, output)
        for name in ("xwdrag_focus", "xwdrag_place"):
            invocation = f"xwdrag_focus {TEST_PID}" if name == "xwdrag_focus" else placement
            rc, output, calls = run(invocation, code=helpers,
                                    HYPR_FAKE_CLIENTS=json.dumps([{"pid": TEST_PID, "address": "flea"}]))
            check(name + " refuses a non-address before calling the compositor",
                  rc == 1 and "hypr_window_focus: refused argument " in output and f"could not focus {TEST_PID}" in output and not calls, output)

    print(f"hypr-dispatch-proof: {checks - len(failures)} passed, {len(failures)} failed")
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())
