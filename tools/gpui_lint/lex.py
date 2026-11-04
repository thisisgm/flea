"""Rust tokens with source positions; comments and literals are opaque."""
from dataclasses import dataclass
import re


@dataclass(frozen=True)
class Token:
    value: str
    kind: str
    line: int


# Sample input: Rust source "let c = 0x1e_29_3b; // note", lexed into tokens with line numbers.
def tokens(source):
    result = []
    i, line = 0, 1
    while i < len(source):
        start, at = i, line
        ch = source[i]
        if ch.isspace():
            i += 1
            line += ch == "\n"
            continue
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end < 0 else end
            continue
        if source.startswith("/*", i):
            depth = 1
            i += 2
            while depth and i < len(source):
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise ValueError(f"line {at}: unterminated block comment")
            line += source[start:i].count("\n")
            continue
        raw = re.match(r'(?:br|cr|r)(#*)"', source[i:])
        if raw:
            close = '"' + raw[1]
            end = source.find(close, i + raw.end())
            if end < 0:
                raise ValueError(f"line {at}: unterminated raw string")
            i += raw.end()
            value = source[i:end]
            i = end + len(close)
            result.append(Token('"' + value + '"', "string", at))
        elif ch == '"' or source.startswith(('b"', 'c"'), i):
            if ch != '"':
                i += 1
            i += 1
            begin = i
            while i < len(source) and source[i] != '"':
                i += 2 if source[i] == "\\" else 1
            if i >= len(source):
                raise ValueError(f"line {at}: unterminated string")
            result.append(Token('"' + source[begin:i] + '"', "string", at))
            i += 1
        else:
            # A lifetime has no closing quote; escaped and Unicode chars are single opaque tokens.
            char = re.match(r"(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\n])|[^'\\\n])'", source[i:])
            ident = re.match(r"(?:r#)?[^\W\d]\w*|_\w*", source[i:], re.UNICODE)
            number = re.match(r"(?:0[xob][0-9a-fA-F_]+|\d[\w]*(?:\.\d[\w]*)?(?:[eE][+-]?\d+)?)", source[i:])
            if char:
                result.append(Token(char[0], "char", at))
                i += char.end()
            elif ident:
                result.append(Token(ident[0].removeprefix("r#"), "ident", at))
                i += ident.end()
            elif number:
                result.append(Token(number[0], "number", at))
                i += number.end()
            else:
                value = next((p for p in ("::", "->", "=>", "..=", "..") if source.startswith(p, i)), ch)
                result.append(Token(value, "punct", at))
                i += len(value)
        line += source[start:i].count("\n")
    return result


def pairs(ts):
    stack, matched = [], {}
    for i, token in enumerate(ts):
        if token.kind != "punct":
            continue
        if token.value in ("(", "[", "{"):
            stack.append(i)
        elif token.value in (")", "]", "}"):
            if not stack or ts[stack[-1]].value != {")": "(", "]": "[", "}": "{"}[token.value]:
                raise ValueError(f"line {token.line}: unmatched {token.value}")
            start = stack.pop()
            matched[start] = i
            matched[i] = start
    if stack:
        raise ValueError(f"line {ts[stack[-1]].line}: unclosed delimiter")
    return matched


# Sample input: tokens of "use std::{fs::{self, File as F}, io::*};", which aliases F to std::fs::File.
def use_aliases(ts, parent=None):
    """Expand Rust use trees, including renames, nested groups and self imports."""
    aliases = {}

    def tree(part, prefix=()):
        values = [t.value for t in part]
        if "{" in values:
            opening = values.index("{")
            base = prefix + tuple(v for v in values[:opening] if v != "::")
            depth, begin = 0, opening + 1
            for j in range(begin, len(values)):
                if values[j] == "{":
                    depth += 1
                elif values[j] == "}":
                    if depth == 0:
                        tree(part[begin:j], base)
                        break
                    depth -= 1
                elif values[j] == "," and depth == 0:
                    tree(part[begin:j], base)
                    begin = j + 1
            return
        if not values:
            return
        rename = values.index("as") if "as" in values else len(values)
        path = prefix + tuple(v for v in values[:rename] if v != "::")
        if path and path[-1] == "self":
            path = path[:-1]
        if not path:
            return
        name = values[rename + 1] if rename < len(values) else path[-1]
        if name == "*":
            if path[:-1] == ("super",):
                for item, target in (parent or {}).items():
                    aliases.setdefault(item, target)
                return
            exports = {
                ("std", "fs"): "File OpenOptions read read_to_string read_dir metadata symlink_metadata canonicalize write copy rename remove_file remove_dir remove_dir_all create_dir create_dir_all set_permissions hard_link read_link",
                ("std", "thread"): "Builder JoinHandle sleep spawn park park_timeout",
                ("std", "process"): "Command Child exit id",
                ("std", "net"): "TcpStream TcpListener UdpSocket",
            }
            for item in exports.get(path[:-1], "").split():
                aliases[item] = path[:-1] + (item,)
            return
        aliases[name] = path

    matched = pairs(ts)
    i = 0
    while i < len(ts):
        token = ts[i]
        if token.kind == "ident" and token.value == "use":
            end = next((j for j in range(i + 1, len(ts)) if ts[j].value == ";"), len(ts))
            tree(ts[i + 1:end])
            i = end
        if token.value == "extern" and i + 5 < len(ts) and ts[i + 1].value == "crate" and ts[i + 3].value == "as":
            aliases[ts[i + 4].value] = (ts[i + 2].value,)
        if i < len(ts) and ts[i].value in ("{", "(", "["):
            i = matched[i]
        i += 1
    return aliases


# Sample input: tokens of "Vec::<u8>::new()" from index i, with the head resolved through aliases.
def path_at(ts, i, aliases):
    if ts[i].value == "<":
        receiver, end = path_at(ts, i + 1, aliases)
        if end < len(ts) and ts[end].value == "<":
            depth = 1
            end += 1
            while end < len(ts) and depth:
                depth += (ts[end].value == "<") - (ts[end].value == ">")
                end += 1
        if receiver and end + 2 < len(ts) and ts[end].value == ">" and ts[end + 1].value == "::":
            tail, last = path_at(ts, end + 2, {})
            return receiver + tail, last
        return (), i
    if ts[i].kind != "ident":
        return (), i
    values = [ts[i].value]
    end = i + 1
    while end + 1 < len(ts) and ts[end].value == "::":
        if ts[end + 1].kind == "ident":
            values.append(ts[end + 1].value)
            end += 2
        elif ts[end + 1].value == "<":
            depth, cursor = 1, end + 2
            while cursor < len(ts) and depth:
                depth += (ts[cursor].value == "<") - (ts[cursor].value == ">")
                cursor += 1
            if depth:
                break
            end = cursor
        else:
            break
    head = aliases.get(values[0], (values[0],))
    return tuple(head) + tuple(values[1:]), end


def scope_aliases(ts, parent, module=False):
    aliases = {} if module else dict(parent)
    for name, path in use_aliases(ts, parent).items():
        if path[:1] == ("super",) and len(path) > 1:
            aliases[name] = parent.get(path[1], path[:2]) + path[2:]
        else:
            aliases[name] = aliases.get(path[0], (path[0],)) + path[1:]
    return aliases
