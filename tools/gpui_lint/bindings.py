"""Lexical imports, local callable aliases and gpui glob shadowing, without literal-value dataflow."""
from dataclasses import dataclass
from .lex import pairs, path_at, scope_aliases

ITEMS = ("fn", "const", "static", "struct", "enum", "type", "trait", "mod", "union")


@dataclass
class Environment:
    aliases: dict
    glob: bool = False
    bound: frozenset = frozenset()


def use_heads(ts, start, end, matched):
    # Heads of the top-level use statements that glob-import, such as gpui or super.
    heads, i = set(), start
    while i < end:
        if ts[i].value == "use":
            stop = next((j for j in range(i, end) if ts[j].value == ";"), end)
            tail = [t.value for t in ts[i + 1:stop]]
            if "*" in tail:
                heads.add(tail[0])
            i = stop
        elif ts[i].value in ("(", "[", "{"):
            i = matched[i]
        i += 1
    return heads


def declared(ts, start, end, matched):
    # Items a block declares at its own level are visible in the whole block; nested blocks declare their own.
    names, i = set(), start
    while i < end:
        value = ts[i].value
        if value in ITEMS and i + 1 < end and ts[i + 1].kind == "ident":
            names.add(ts[i + 1].value)
        elif value == "macro_rules" and i + 2 < end and ts[i + 1].value == "!" and ts[i + 2].kind == "ident":
            names.add(ts[i + 2].value)
        if value in ("(", "[", "{"):
            i = matched[i]
        i += 1
    return names


def let_name(ts, i, stop):
    # Only a plain `let x` or `let mut x` binds a name; tuple, struct and other patterns are not modelled.
    j = i + 1
    if j < stop and ts[j].value == "mut":
        j += 1
    if j < stop and ts[j].kind == "ident" and j + 1 < len(ts) and ts[j + 1].value in ("=", ":", ";"):
        return ts[j].value
    return None


def container_body(ts, start, i):
    # Methods of an impl or trait body are reached through a path or a receiver, never by their bare name.
    j = i - 1
    while j >= start and ts[j].value not in (";", "{", "}"):
        j -= 1
    header = [t.value for t in ts[j + 1:i]]
    return "fn" not in header and ("impl" in header or "trait" in header)


def environments(ts, aliases=None):
    """Imports are block items; lets take effect after their statement, within that block."""
    # Parameters, for, match, if-let and closure bindings are not modelled, so they never shadow a glob.
    matched = pairs(ts)
    result = [None] * len(ts)

    def statement_end(start, end):
        i = start
        while i < end:
            if ts[i].value == ";":
                break
            if ts[i].value in ("{", "(", "["):
                i = matched[i]
            i += 1
        return i

    def binding(start, end):
        i = start + 1
        if i < end and ts[i].value == "mut":
            i += 1
        if i + 1 >= end or ts[i].kind != "ident" or ts[i + 1].value not in ("=", ":"):
            return None
        equal = next((j for j in range(i + 1, end) if ts[j].value == "="), end)
        return (ts[i].value, ts[equal + 1:end]) if equal < end else None

    def walk(start, end, parent, module=False, container=False):
        imports = scope_aliases(ts[start:end], parent.aliases, module)
        heads = use_heads(ts, start, end, matched)
        own = set() if container else declared(ts, start, end, matched)
        if module:
            glob = "gpui" in heads or ("super" in heads and parent.glob)
            bound = own | (parent.bound if "super" in heads else set())
        else:
            glob = parent.glob or "gpui" in heads
            bound = parent.bound | own
        environment = Environment(imports, glob, frozenset(bound))
        pending, shadow = {}, {}
        i = start
        while i < end:
            result[i] = environment
            value = ts[i].value
            if value == "let" and (i == start or ts[i - 1].value not in ("if", "while")):
                stop = statement_end(i, end)
                name = let_name(ts, i, stop)
                if name and stop < end:
                    item = binding(i, stop)
                    pending[stop] = (name, item[1] if item else [])
                    shadow[stop] = name
            elif ts[i].kind == "ident" and i + 1 < end and ts[i + 1].value == "=":
                if i == start or ts[i - 1].value in (";", "}"):
                    stop = statement_end(i, end)
                    if stop < end:
                        pending[stop] = (value, ts[i + 2:stop])
            if value in ("{", "(", "["):
                close = matched[i]
                child_module = i >= 2 and ts[i - 2].value == "mod"
                child_container = value == "{" and container_body(ts, start, i)
                walk(i + 1, close, environment, child_module, child_container)
                result[close] = environment
                i = close
            if i in pending or i in shadow:
                imports = dict(environment.aliases)
                if i in pending:
                    name, expression = pending[i]
                    while expression and expression[0].value == "(":
                        if pairs(expression)[0] != len(expression) - 1:
                            break
                        expression = expression[1:-1]
                    path, last = path_at(expression, 0, imports) if expression else ((), 0)
                    if path and last == len(expression):
                        imports[name] = path
                    else:
                        imports.pop(name, None)
                bound = (environment.bound | {shadow[i]}) if i in shadow else environment.bound
                environment = Environment(imports, environment.glob, frozenset(bound))
            i += 1

    walk(0, len(ts), Environment(dict(aliases or {})))
    return result
