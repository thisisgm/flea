"""Shell parser-child inventory, including command wrappers and substitutions."""
import argparse
import re
import runpy
import traceback
from functools import partial
from functools import lru_cache
from pathlib import Path


try:
    SCOPE = runpy.run_path(str(Path(__file__).with_name("locale-shell-scope.py")))
    top_level = SCOPE["top_level"]
except Exception:
    traceback.print_exc()
    raise SystemExit(3)


# Sample input: x="${FOO:-$(grep -m1 bar file)}"
def code_mask(text, comment_spans=None):
    mask = list(text)
    docs = []

    def hide(a, b):
        mask[a:b] = 'Q' * (b - a)

    # Sample input: "${FOO:-$(grep -m1 bar file)}"
    def quote(i):
        delimiter = text[i]
        hide(i, i + 1)
        i += 1
        while i < len(text) and text[i] != delimiter:
            if delimiter == '"' and text.startswith('${', i):
                i = parameter(i)
            elif delimiter == '"' and text.startswith('$((', i):
                i = arithmetic(i)
            elif delimiter == '"' and text.startswith('$(', i):
                i = code(i + 2, True)
            elif delimiter == '"' and text[i] == '\\':
                hide(i, min(i + 2, len(text))); i += 2
            elif delimiter == '"' and text[i] == "`":
                i = code(i + 1, "`")
            else:
                hide(i, i + 1); i += 1
        if i < len(text):
            hide(i, i + 1); i += 1
        return i

    # Sample input: ${FOO:-${BAR:-$(grep -m1 bar file)}}
    def parameter(i):
        hide(i, i + 2)
        i += 2
        while i < len(text):
            if text.startswith('${', i):
                i = parameter(i)
            elif text.startswith('$((', i):
                i = arithmetic(i)
            elif text.startswith('$(', i):
                i = code(i + 2, True)
            elif text[i] in "\"'":
                i = quote(i)
            elif text[i] == '\\':
                hide(i, min(i + 2, len(text))); i += 2
            else:
                end = text[i] == '}'
                hide(i, i + 1); i += 1
                if end:
                    return i
        return i

    # Sample input: $(( $(date +%s) - start ))
    def arithmetic(i):
        hide(i, i + 3)
        i += 3
        depth = 2
        while i < len(text) and depth:
            if text.startswith('$(', i) and not text.startswith('$((', i):
                i = code(i + 2, True)
                continue
            if text[i] == '(':
                depth += 1
            elif text[i] == ')':
                depth -= 1
            hide(i, i + 1); i += 1
        return i

    def code(i, closing=False):
        depth = 0
        while i < len(text):
            c = text[i]
            if c == '\n' and docs:
                i += 1
                for delimiter, tabs in docs[:]:
                    start = i
                    while i < len(text):
                        end = text.find('\n', i)
                        if end < 0:
                            end = len(text)
                        line = text[i:end]
                        i = min(end + 1, len(text))
                        if (line.lstrip('\t') if tabs else line) == delimiter:
                            break
                    hide(start, i)
                    if i and text[i - 1] == '\n':
                        mask[i - 1] = '\n'
                docs.clear()
                continue
            if c == '#' and (i == 0 or text[i - 1].isspace() or text[i - 1] in ';|&('):
                end = text.find('\n', i)
                if end < 0:
                    end = len(text)
                if comment_spans is not None:
                    comment_spans.append((i, end))
                hide(i, end); i = end; continue
            if text.startswith('<<', i) and not text.startswith('<<<', i):
                match = re.match(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z_0-9]*)\2", text[i:])
                if match:
                    docs.append((match[3], bool(match[1])))
            if text.startswith('${', i):
                i = parameter(i); continue
            if text.startswith('$((', i):
                i = arithmetic(i); continue
            if c in "\"'":
                i = quote(i); continue
            if text.startswith('\\\n', i):
                mask[i:i + 2] = [' ', ' ']; i += 2; continue
            if c == '\\':
                hide(i, min(i + 2, len(text))); i += 2; continue
            stop = ')' if closing is True else closing
            if stop and c == stop and depth == 0:
                return i + 1
            if c == '(':
                depth += 1
            if c == ')':
                depth -= 1
            i += 1
        return i

    code(0)
    return ''.join(mask)

BOUNDARY = re.compile(r"(?:^|[\n;|&({<!`]|\b(?:if|then|elif|else|do|while|until)\b)\s*", re.M)
TOKEN = re.compile(r"[^\s;|&()<>]+")
ASSIGNMENT = re.compile(r"[A-Za-z_][A-Za-z_0-9]*=")
WRAPPERS = {"env", "xargs", "time", "nice", "command", "timeout", "exec"}
OPTION_VALUE = {"-u", "--unset", "-a", "-d", "-E", "-e", "-I", "-L", "-l", "-n", "-P", "-s", "-f", "--format", "-o", "--output", "--adjustment", "--max-args", "--max-procs", "--replace", "--arg-file", "--delimiter", "--max-lines", "--max-chars"}
SHELL_LIBRARIES = {"flea-ipc-pid", "flea-sandbox-guard", "flea-pkgrel-field"}
FILTER = {"sort", "uniq", "comm", "join"}
PARSER_NAMES = FILTER | set("awk sed grep rg tr ls ps pgrep find findmnt du df date uname pacman curl file diff python3 perl bc readelf nm strings expr".split())


# Sample input: ! env -u LANGUAGE LC_ALL=C.UTF-8 nice -n 2 grep -m1 bar file
def commands(source):
    masked = code_mask(source)
    seen = set()
    for boundary in BOUNDARY.finditer(masked):
        cursor = boundary.end()
        wrapper = None
        while True:
            token = TOKEN.match(masked, cursor)
            if not token:
                break
            word = token[0]
            cursor = token.end()
            gap = re.match(r"[ \t]*", masked[cursor:])
            cursor += gap.end()
            if ASSIGNMENT.match(word) or word == "!":
                continue
            name = word.rsplit("/", 1)[-1]
            if name in WRAPPERS:
                wrapper = name
                continue
            if wrapper and word.startswith("-"):
                if wrapper == "command" and word in {"-v", "-V"}:
                    break
                if word in OPTION_VALUE:
                    value = TOKEN.match(masked, cursor)
                    if not value:
                        break
                    cursor = value.end()
                    cursor += re.match(r"[ \t]*", masked[cursor:]).end()
                continue
            if wrapper == "timeout":
                wrapper = "command"
                continue
            start = token.start()
            line_start = source.rfind("\n", 0, start) + 1
            arm = re.match(r"\s*[\w*?-]+(?:\|[\w*?-]+)+\)", source[line_start:])
            if arm and start < line_start + arm.end():
                break
            if start not in seen:
                seen.add(start)
                yield start, name, source[boundary.end():start]
            break


# Sample input: LC_ALL=C LANGUAGE= sed x # locale-bytes: count raw filename extensions
def scoped(prefix, line="", program=""):
    language = (re.search(r"(?:^|\s)LANGUAGE=(?:\s|$)", prefix) is not None
                or re.search(r"\benv\s+-u\s+LANGUAGE\b", prefix) is not None)
    if not language:
        return False
    if re.search(r"(?:^|\s)LC_ALL=C\.UTF-8(?:\s|$)", prefix):
        return True
    if program not in {"sed", "awk", "grep"} or not re.search(r"(?:^|\s)LC_ALL=C(?:\s|$)", prefix):
        return False
    comments = []
    code_mask(line, comments)
    return any(re.match(r"# locale-bytes: \S.+", line[a:b]) for a, b in comments)


# Sample input: export LC_ALL=C.UTF-8 followed by unset LANGUAGE before grep reads a file.
@lru_cache(maxsize=64)
def export_scope(source):
    first = min((start for start, name, _ in commands(source)
                 if parsed(name, source[start + len(name):].lstrip())), default=len(source))
    top = top_level(code_mask(source))
    declarations = []
    for match in re.finditer(r"(?m)^[ \t]*(export LC_ALL=C\.UTF-8|unset LANGUAGE)[ \t]*$", top[:first]):
        declarations.append(source[match.start():match.end()])
    return "\n".join(declarations) + ("\n" if declarations else "")


def exported(source):
    return {line.strip() for line in export_scope(source).splitlines()} >= {"export LC_ALL=C.UTF-8", "unset LANGUAGE"}


# Sample input: -crop 10x10 \ followed by a newline and -format '%k' info:.
def command_text(suffix):
    return re.sub(r"\\\n[ \t]*", " ", suffix).split("\n", 1)[0]


# Sample input: a comment with a form feed, a newline, then LC_ALL=C LANGUAGE= sed x # locale-bytes: raw names
def physical_line(source, start):
    return source.split("\n")[source.count("\n", 0, start)]


# Sample input: LC_ALL=C LANGUAGE= grep -c . paths # locale-bytes: count raw path records
def scope_for(source, start, name, prefix):
    explicit = re.search(r"(?:^|\s)(?:LC_ALL|LANGUAGE)=", prefix) is not None
    line = physical_line(source, start)
    return scoped(prefix, line, name) if explicit else exported(source)


# Sample input: magick "$f" -format "%k" info:
def parsed(name, suffix):
    if name in PARSER_NAMES:
        return True
    if name == "git":
        return re.match(r"(?:-C\s+\S+\s+)?(?:rev-parse|status|diff|log|show|ls-files|describe|remote|config)\b", suffix) is not None
    if name == "qs":
        return re.match(r"(?:ipc|list|-V|--version)\b", suffix) is not None
    if name == "gio":
        return re.match(r"(?:info|list|mime|mount\s+-l)\b", suffix) is not None
    if name == "omarchy-drive":
        return True
    if name == "omarchy-shell":
        return suffix.startswith("notifications isDnd")
    if name == "omarchy":
        return re.match(r"theme\s+(?:current|dir|list)\b", suffix) is not None
    if name == "hyprctl":
        return re.match(r"(?:-j\s+)?(?:clients|monitors|activewindow|cursorpos|binds|configerrors|layers|instances|devices|getoption)\b", suffix) is not None
    if name == "hk":
        return suffix.startswith("--version")
    if name == "ffmpeg":
        return "-encoders" in command_text(suffix)
    if name == "magick":
        return "-format" in command_text(suffix)
    if name == "patch":
        return True
    if name == "systemctl":
        return re.match(r"(?:--\S+\s+)*(?:show|show-environment|is-active|list-units)\b", suffix) is not None
    return name in {"ffprobe", "btrfs", "inotifywait", "ip", "gdbus", "wl-paste"}


def shell_files(repo):
    for path in sorted((repo / "tools").rglob("*")):
        if not path.is_file():
            continue
        source = path.read_text(errors="replace")
        if source.startswith(("#!/bin/bash", "#!/usr/bin/env bash", "#!/bin/sh")) or path.name in SHELL_LIBRARIES:
            yield path, source


def inventory(repo, files=None):
    for path, source in shell_files(repo) if files is None else files:
        try:
            export_scope(source)
        except ValueError as error:
            yield path, 1, "shell nesting", False, str(error)
            continue
        functions = set(re.findall(r"^\s*([A-Za-z_][A-Za-z_0-9]*)\s*\(\)", source, re.M))
        for start, name, prefix in commands(source):
            suffix = source[start + len(name):].lstrip()
            if name in functions or not parsed(name, suffix):
                continue
            line = source.count("\n", 0, start) + 1
            scope = scope_for(source, start, name, prefix)
            yield path, line, name, scope, prefix


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--regress-parameters', action='store_true')
    parser.add_argument('--regress-continuations', action='store_true')
    parser.add_argument('--regress-exports', action='store_true')
    parser.add_argument('--regress-case-scope', action='store_true')
    parser.add_argument('--regress-block-boundaries', action='store_true')
    parser.add_argument('--regress-block-balance', action='store_true')
    parser.add_argument('--regress-physical-lines', action='store_true')
    parser.add_argument('--regress-byte-marker', nargs='?', const='all',
                        choices=('all',) + SCOPE['BYTE_TOOLS'])
    args = parser.parse_args()
    global code_mask, command_text, exported, top_level, physical_line
    if args.regress_parameters:
        real_mask = code_mask
        code_mask = lambda text, *a, **k: real_mask(re.sub(
            r'\$\{[^}]*\}', lambda m: 'Q' * len(m[0]), text), *a, **k)
    if args.regress_continuations:
        command_text = lambda suffix: suffix.split('\n', 1)[0]
    if args.regress_exports:
        exported = lambda source: 'export LC_ALL=C.UTF-8\n' in source and 'unset LANGUAGE\n' in source
    top_level = partial(top_level, case_patterns=not args.regress_case_scope,
                        operator_boundaries=not args.regress_block_boundaries,
                        strict=not args.regress_block_balance)
    if args.regress_physical_lines:
        physical_line = lambda source, start: source.splitlines()[source.count('\n', 0, start)]
    errors = []
    checks = 0

    def check(label, passed):
        nonlocal checks
        checks += 1
        if not passed:
            errors.append('FAIL ' + label)

    SCOPE["self_checks"](globals(), check)
    repo = Path(__file__).resolve().parents[1]
    if args.regress_byte_marker:
        targets = SCOPE['BYTE_TOOLS'] if args.regress_byte_marker == 'all' else (args.regress_byte_marker,)
        detected = []
        for target in targets:
            try:
                failures = SCOPE['unmarked_rows'](globals(), repo, target)
            except ValueError as error:
                errors.append('FAIL ' + str(error))
                detected.append(False)
                continue
            detected.append(bool(failures))
            if not failures:
                errors.append(f'FAIL byte-marker control {target}: no unpinned site detected')
            for path, line, name, _, _ in failures:
                print(f'FAIL {path.relative_to(repo)}:{line} {name} without its byte marker')
            print(f'byte-marker control {target}: {len(failures)} unpinned sites')
        if errors:
            print('\n'.join(errors))
        if not (detected and all(detected) and not errors):
            return 3
        print('DETECTED --regress-byte-marker ' + args.regress_byte_marker)
        return 1
    rows = list(inventory(repo))
    for path, line, name, scope_ok, prefix in rows:
        check(f'{path.relative_to(repo)}:{line} {name} scope', scope_ok)
    if errors:
        print('\n'.join(errors))
        print(f'FAIL {len(errors)} of {checks} shell inventory checks')
        SCOPE['emit_detection']('shell', args, errors)
        return 1
    print(f'PASS {checks} shell inventory checks; {len(rows)} parsed sites; 0 unpinned')
    return 0


if __name__ == '__main__':
    try:
        result = main()
    except Exception:
        traceback.print_exc()
        result = 3
    raise SystemExit(result)
