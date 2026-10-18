"""Shell parser-child inventory, including command wrappers and substitutions."""
import re
from pathlib import Path

# Mask quoted data and here-documents while retaining command substitutions as executable code.
def code_mask(text):
    mask = list(text)
    docs = []
    def hide(a, b):
        for j in range(a, b):
            mask[j] = 'Q'
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
                        if end < 0: end = len(text)
                        line = text[i:end]
                        i = min(end + 1, len(text))
                        if (line.lstrip('\t') if tabs else line) == delimiter: break
                    hide(start, i)
                    if i and text[i-1] == '\n': mask[i-1] = '\n'
                docs.clear()
                continue
            if c == '#' and (i == 0 or text[i-1].isspace() or text[i-1] in ';|&('):
                end = text.find('\n', i)
                if end < 0: end = len(text)
                hide(i, end); i = end; continue
            if text.startswith('<<', i) and not text.startswith('<<<', i):
                match = re.match(r'<<(-?)\s*([\'\"]?)([A-Za-z_][A-Za-z_0-9]*)\2', text[i:])
                if match: docs.append((match[3], bool(match[1])))
            if text.startswith('${', i) or text.startswith('$((', i):
                end_token = '}' if text.startswith('${', i) else '))'
                end = text.find(end_token, i+2)
                if end < 0: end = len(text)-len(end_token)
                hide(i, end+len(end_token)); i = end+len(end_token); continue
            if c == "'":
                end = text.find("'", i+1)
                if end < 0: end = len(text)-1
                hide(i, end+1); i = end+1; continue
            if c == '"':
                hide(i, i+1); i += 1
                while i < len(text) and text[i] != '"':
                    if text[i] == '\\': hide(i, min(i+2, len(text))); i += 2
                    elif text.startswith('$(', i) and not text.startswith('$((', i): i = code(i+2, True)
                    elif text[i] == '`':
                        end = text.find('`', i+1)
                        if end < 0: end = len(text)-1
                        i = end+1
                    else: hide(i, i+1); i += 1
                if i < len(text): hide(i, i+1); i += 1
                continue
            if text.startswith('\\\n', i):
                mask[i:i+2] = [' ', ' ']; i += 2; continue
            if c == '\\': hide(i, min(i+2, len(text))); i += 2; continue
            if closing and c == ')' and depth == 0: return i+1
            if c == '(': depth += 1
            if c == ')': depth -= 1
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


def scoped(prefix):
    return (re.search(r"(?:^|\s)LC_ALL=C\.UTF-8(?:\s|$)", prefix) is not None
            and (re.search(r"(?:^|\s)LANGUAGE=(?:\s|$)", prefix) is not None
                 or re.search(r"\benv\s+-u\s+LANGUAGE\b", prefix) is not None))


def exported(source):
    return "export LC_ALL=C.UTF-8\n" in source and "unset LANGUAGE\n" in source


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
        return "-encoders" in suffix.split("\n", 1)[0]
    if name == "magick":
        return "-format" in suffix.split("\n", 1)[0]
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


def inventory(repo):
    for path, source in shell_files(repo):
        functions = set(re.findall(r"^\s*([A-Za-z_][A-Za-z_0-9]*)\s*\(\)", source, re.M))
        for start, name, prefix in commands(source):
            suffix = source[start + len(name):].lstrip()
            if name in functions or not parsed(name, suffix):
                continue
            yield path, source.count("\n", 0, start) + 1, name, exported(source) or scoped(prefix), prefix
