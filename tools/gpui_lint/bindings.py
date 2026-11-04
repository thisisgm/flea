"""Lexical gpui glob scopes: a crate:: glob, or a self::, super:: or child-module glob path through a name that is not an inline child module there and that the file does not declare only as a type, counts as gpui when any module holds one."""
from dataclasses import dataclass, field
from .lex import pairs, path_at, scope_aliases

ITEMS = ("fn", "const", "static", "struct", "enum", "type", "trait", "mod", "union")
TYPES = ("struct", "enum", "trait", "union", "type")
QUALIFIERS = ("#", "!", "pub", "async", "unsafe", "extern", "default", "safe")


@dataclass
class Environment:
    aliases: dict
    glob: bool = False
    items: frozenset = frozenset()
    lets: frozenset = frozenset()


@dataclass
class Module:
    parent: object
    globs: set
    names: set
    children: dict = field(default_factory=dict)
    holds: bool = False


# Sample input: tokens of "use ::gpui::*; use {super::*, std::fs};", whose glob paths are ("gpui",) and ("super",).
def glob_paths(ts, start, end, matched):
    paths, i = set(), start

    # Sample input: tokens of "a::{b::*, c}" between start and end, which adds the glob path ("a", "b").
    def tree(start, end, prefix):
        i = start
        while i < end:
            value = ts[i].value
            if value == "{":
                begin, j, close = i + 1, i + 1, matched[i]
                while j <= close:
                    if j == close or ts[j].value == ",":
                        tree(begin, j, prefix)
                        begin = j + 1
                    elif ts[j].value == "{":
                        j = matched[j]
                    j += 1
                return
            if value == "*" and prefix:
                paths.add(prefix)
            if value in ("*", "as"):
                return
            if value != "::":
                prefix += (value,)
            i += 1

    while i < end:
        if ts[i].value == "use":
            stop = next((j for j in range(i, end) if ts[j].value == ";"), end)
            tree(i + 1, stop, ())
            i = stop
        elif ts[i].value in ("(", "[", "{"):
            i = matched[i]
        i += 1
    return paths


# Sample input: tokens of "fn a() {} extern \"C\" { fn b(); } struct S;", which declare a, b and S at this level.
def declared(ts, start, end, matched):
    # Items a block declares at its own level are visible in the whole block; an extern block declares into it.
    names, i = set(), start
    while i < end:
        value = ts[i].value
        if value in ITEMS and i + 1 < end and ts[i + 1].kind == "ident":
            names.add(ts[i + 1].value)
        elif value == "macro_rules" and i + 2 < end and ts[i + 1].value == "!" and ts[i + 2].kind == "ident":
            names.add(ts[i + 2].value)
        elif value == "{" and "extern" in [t.value for t in ts[max(start, i - 2):i]]:
            names |= declared(ts, i + 1, matched[i], matched)
        if value in ("(", "[", "{"):
            i = matched[i]
        i += 1
    return names


# Sample input: tokens of "let mut white = 1;" from the let at i to the semicolon at stop, which bind white.
def let_name(ts, i, stop):
    # Only a plain `let x` or `let mut x` binds a name; tuple, struct and other patterns are not modelled.
    j = i + 1
    if j < stop and ts[j].value == "mut":
        j += 1
    if j < stop and ts[j].kind == "ident" and j + 1 < len(ts) and ts[j + 1].value in ("=", ":", ";"):
        return ts[j].value
    return None


# Sample input: tokens of "let n = 1; fn go(rows: [u8; 4]) {" with i at the brace, whose statement starts at fn.
def statement_start(ts, start, i, matched):
    j = i - 1
    while j >= start and ts[j].value not in (";", "{", "}"):
        j = matched[j] - 1 if ts[j].value in (")", "]") else j - 1
    return j + 1


# Sample input: tokens of "impl S {" with i at the brace, a body whose methods are not bare names in scope.
def container_body(ts, start, i, matched):
    # Methods of an impl or trait body are reached through a path or a receiver, never by their bare name.
    header = [t.value for t in ts[statement_start(ts, start, i, matched):i]]
    return "fn" not in header and ("impl" in header or "trait" in header)


# Sample input: tokens of "#[inline] pub(crate) fn go(&self) {" with i at the brace, an item body that never sees the enclosing lets.
def item_body(ts, start, i, matched):
    k = statement_start(ts, start, i, matched)
    while k < i and (ts[k].value in QUALIFIERS or ts[k].kind == "string"):
        # An attribute or a pub(...) visibility is skipped whole.
        k = matched[k + 1] + 1 if ts[k + 1].value in ("(", "[") else k + 1
    return k + 1 < i and ts[k].value in ITEMS and ts[k + 1].kind == "ident"


# Sample input: tokens of "use gpui::*; mod inner { use super::*; }", a Module for the file at -1 and one per mod brace.
def module_table(ts, matched):
    table = {-1: Module(None, glob_paths(ts, 0, len(ts), matched), declared(ts, 0, len(ts), matched))}

    # Sample input: tokens of a module body from start to end; a mod nested in a fn body belongs to the module around it.
    def scan(start, end, key, top):
        i = start
        while i < end:
            if ts[i].value == "mod" and i + 2 < end and ts[i + 1].kind == "ident" and ts[i + 2].value in ("{", ";"):
                brace, inline = i + 2, ts[i + 2].value == "{"
                if top:
                    table[key].children[ts[i + 1].value] = brace if inline else None
                if inline:
                    close = matched[brace]
                    table[brace] = Module(key, glob_paths(ts, brace + 1, close, matched), declared(ts, brace + 1, close, matched))
                    scan(brace + 1, close, brace, True)
                    i = close
            elif ts[i].value in ("(", "[", "{"):
                scan(i + 1, matched[i], key, False)
                i = matched[i]
            i += 1

    scan(0, len(ts), -1, True)
    types = {ts[k + 1].value for k in range(len(ts) - 1) if ts[k].value in TYPES and ts[k + 1].kind == "ident"}
    types -= {ts[k + 1].value for k in range(len(ts) - 1) if ts[k].value == "mod"}
    changed = True
    while changed:
        changed = False
        for key, module in table.items():
            for path in module.globs:
                holds, names = imported(table, types, key, path)
                if (holds and not module.holds) or not names <= module.names:
                    module.holds, module.names, changed = module.holds or holds, module.names | names, True
    return table, types


# Sample input: module key -1 and the glob path ("super", "colors"), which carries what that inline module holds.
def imported(table, types, key, path):
    if path[0] == "gpui":
        return True, set()
    if path[0] not in ("crate", "self", "super") and path[0] not in table[key].children:
        return False, set()
    target = None if path[0] == "crate" else key
    for segment in path:
        if target is None or segment == "self":
            continue
        if segment == "super":
            target = table[target].parent
        elif segment in table[target].children:
            target = table[target].children[segment]
        elif segment in types:
            return False, set()
        else:
            target = None
    if target is None:
        return any(module.holds for module in table.values()), set()
    return table[target].holds, table[target].names


# Sample input: tokens of a whole Rust file, or of one function body with its parent aliases.
def environments(ts, aliases=None):
    """Imports and items are block-wide; lets take effect after their statement and never cross an item body."""
    # Parameters, for, match, if-let and closure bindings are not modelled, so they never shadow a glob.
    matched = pairs(ts)
    table, types = module_table(ts, matched)
    result = [None] * len(ts)

    # Sample input: tokens of "let ink = theme.raw; paint(ink);" from the let, which ends at the first semicolon.
    def statement_end(start, end):
        i = start
        while i < end:
            if ts[i].value == ";":
                break
            if ts[i].value in ("{", "(", "["):
                i = matched[i]
            i += 1
        return i

    # Sample input: tokens of "let make = gpui::rgb;" from start to the semicolon at end, giving make and its value.
    def binding(start, end):
        i = start + 1
        if i < end and ts[i].value == "mut":
            i += 1
        if i + 1 >= end or ts[i].kind != "ident" or ts[i + 1].value not in ("=", ":"):
            return None
        equal = next((j for j in range(i + 1, end) if ts[j].value == "="), end)
        return (ts[i].value, ts[equal + 1:end]) if equal < end else None

    # Sample input: tokens of one block from start to end, under the environment of the block around it.
    def walk(start, end, parent, key, module=False, container=False, item=False):
        imports = scope_aliases(ts[start:end], parent.aliases, module)
        if module:
            environment = Environment(imports, table[key].holds, frozenset(table[key].names))
        else:
            carried = [imported(table, types, key, path) for path in glob_paths(ts, start, end, matched)]
            own_glob = any(holds for holds, _ in carried)
            items = set() if container else declared(ts, start, end, matched)
            items |= set().union(*(names for _, names in carried))
            # A block's own gpui glob is found before every name bound outside the block.
            outer = Environment({}) if own_glob else parent
            lets = frozenset() if container or item else outer.lets
            environment = Environment(imports, parent.glob or own_glob, frozenset(outer.items | items), lets)
        pending, shadow = {}, {}
        i = start
        while i < end:
            result[i] = environment
            value = ts[i].value
            if value == "let" and (i == start or ts[i - 1].value not in ("if", "while")):
                stop = statement_end(i, end)
                name = let_name(ts, i, stop)
                if name and stop < end:
                    item_binding = binding(i, stop)
                    pending[stop] = (name, item_binding[1] if item_binding else [])
                    shadow[stop] = name
            elif ts[i].kind == "ident" and i + 1 < end and ts[i + 1].value == "=":
                if i == start or ts[i - 1].value in (";", "}"):
                    stop = statement_end(i, end)
                    if stop < end:
                        pending[stop] = (value, ts[i + 2:stop])
            if value in ("{", "(", "["):
                close = matched[i]
                child_module = value == "{" and i in table
                child_container = value == "{" and container_body(ts, start, i, matched)
                child_item = value == "{" and item_body(ts, start, i, matched)
                walk(i + 1, close, environment, i if child_module else key, child_module, child_container, child_item)
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
                lets = (environment.lets | {shadow[i]}) if i in shadow else environment.lets
                environment = Environment(imports, environment.glob, environment.items, frozenset(lets))
            i += 1

    walk(0, len(ts), Environment(dict(aliases or {})), -1)
    return result
