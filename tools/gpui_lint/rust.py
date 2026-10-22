"""Rust item scopes and calls used by the native architecture checks."""
from dataclasses import dataclass
from .lex import pairs, path_at, scope_aliases
from .bindings import environments


@dataclass
class Function:
    symbol: str
    module: str
    path: str
    line: int
    body: list
    aliases: dict
    signature: list


def functions(ts, module, path):
    matched = pairs(ts)
    found, elements = [], set()

    def walk(start, end, scope, inherited=None, child_module=False):
        aliases = scope_aliases(ts[start:end], inherited or {}, child_module)
        i = start
        while i < end:
            value = ts[i].value
            if ts[i].kind == "ident" and value in ("impl", "mod", "fn"):
                opening = i + 1
                while opening < end and ts[opening].value not in ("{", ";"):
                    if ts[opening].value in ("(", "["):
                        opening = matched[opening]
                    opening += 1
                if opening == end or ts[opening].value != "{":
                    i = opening + 1
                    continue
                closing = matched[opening]
                header = [t.value for t in ts[i + 1:opening]]
                if value == "fn":
                    found.append(Function("::".join((*scope, header[0])), module, path,
                                          ts[i].line, ts[opening + 1:closing], aliases,
                                          ts[i + 1:opening]))
                elif value == "mod":
                    if header[0] != "tests":
                        walk(opening + 1, closing, (*scope, header[0]), aliases, True)
                else:
                    if "for" in header:
                        name = header[header.index("for") + 1]
                        if "Element" in header[:header.index("for")]:
                            elements.add(name)
                    else:
                        # Skip impl generics before its target type.
                        depth, name = 0, None
                        for token in header:
                            if token == "<":
                                depth += 1
                            elif token == ">":
                                depth -= 1
                            elif depth == 0:
                                name = token
                                break
                    if name:
                        walk(opening + 1, closing, (*scope, name), aliases)
                i = closing + 1
            elif value in ("{", "(", "["):
                i = matched[i] + 1
            else:
                i += 1

    walk(0, len(ts), tuple(module.split("::")))
    return found, elements


def generic_end(ts, index):
    if index + 1 >= len(ts) or ts[index].value != "::" or ts[index + 1].value != "<":
        return index
    depth = 0
    for j in range(index + 1, len(ts)):
        if ts[j].value == "<":
            depth += 1
        elif ts[j].value == ">":
            depth -= 1
            if depth == 0:
                return j + 1
    return index


def receiver_path(ts, last, matched, aliases):
    if last < 0 or ts[last].value != ")":
        return ()
    opening = matched[last]
    begin = opening - 1
    while begin > 1 and ts[begin - 1].value == "::":
        begin -= 2
    path, end = path_at(ts, begin, aliases)
    if end != opening:
        return ()
    if begin > 1 and ts[begin - 1].value == ".":
        parent = receiver_path(ts, begin - 2, matched, aliases)
        if parent == ("std", "thread", "Builder", "new") and path in (("name",), ("stack_size",)):
            return parent
        if parent == ("background_executor",) and path == ("clone",):
            return parent
    return path


def calls(function, known=None):
    """Only closures passed to a proven background executor/thread are excluded."""
    ts = function.body
    matched = pairs(ts)
    result = []
    skipped = set()
    statement = 0
    aliases = dict(function.aliases)
    scopes = environments(ts, aliases)
    process_variables = set()

    def returns_command(path):
        if path == ("std", "process", "Command", "new"):
            return True
        if not known:
            return False
        target = known.get(resolve("::".join(path), function, known))
        if target is None:
            return False
        signature = target.signature
        arrow = next((j for j, t in enumerate(signature) if t.value == "->"), len(signature))
        return any(path_at(signature, j, target.aliases)[0] == ("std", "process", "Command")
                   for j in range(arrow + 1, len(signature)) if signature[j].kind == "ident")
    signature = function.signature
    for j, token in enumerate(signature):
        if token.value == ":" and j and j + 1 < len(signature):
            k = j + 1
            while k < len(signature) and signature[k].value in ("&", "mut"):
                k += 1
            if k < len(signature) and path_at(signature, k, aliases)[0] == ("std", "process", "Command"):
                process_variables.add(signature[j - 1].value)
    for i, token in enumerate(ts):
        if i in skipped:
            continue
        if token.kind == "punct" and token.value == ";":
            statement = i + 1
        if token.value != "<" and (token.kind != "ident" or (i and ts[i - 1].value == "::")):
            continue
        aliases = scopes[i].aliases
        path, end = path_at(ts, i, aliases)
        if not path:
            continue
        # A local function alias remains the same call, even without use syntax.
        before_values = [t.value for t in ts[statement:i]]
        variable = None
        if "let" in before_values:
            offset = len(before_values) - before_values[::-1].index("let")
            binding = before_values[offset:]
            if binding[:1] == ["mut"]:
                binding = binding[1:]
            if len(binding) >= 2 and binding[1] in ("=", ":") and binding[-1] == "=":
                variable = binding[0]
        end = generic_end(ts, end)
        if end >= len(ts) or ts[end].value != "(":
            continue
        close = matched[end]
        method = i > 0 and ts[i - 1].value == "."
        if returns_command(path) and variable:
            process_variables.add(variable)
        if method and path[-1] == "spawn":
            process_builder = any(returns_command(path_at(ts, k, aliases)[0])
                                  for k in range(statement, i) if ts[k].kind == "ident")
            if (i > 1 and ts[i - 2].value in process_variables) or process_builder:
                path = ("std", "process", "Command", "spawn")
        receiver = receiver_path(ts, i - 2, matched, aliases) if method else ()
        builder = receiver == ("std", "thread", "Builder", "new")
        background = path == ("std", "thread", "spawn") or (
            method and path[-1] == "spawn" and (receiver == ("background_executor",) or builder))
        if background:
            # A directly supplied closure body, including an expression, runs after the thread boundary.
            opening = end + 1
            while opening < close and ts[opening].value in ("async", "move"):
                opening += 1
            direct = False
            if opening < close and ts[opening].value == "|":
                other = next((j for j in range(opening + 1, close) if ts[j].value == "|"), close)
                opening = other + 1
                direct = other < close
            elif opening < close and ts[opening].value == "{":
                direct = ts[end + 1].value == "async"
            if direct:
                skipped.update(range(opening, close))
        awaited = close + 2 < len(ts) and ts[close + 1].value == "." and ts[close + 2].value == "await"
        # Async channel receives are not blocking mpsc receives; std::fs calls remain forbidden.
        result.append(("::".join(path), token.line, ts[end + 1:close], method and not (path[-1] == "recv" and awaited)))
    return result


def resolve(symbol, function, known):
    parts = symbol.split("::")
    if parts[0] == "crate":
        parts[0] = function.module.split("::")[0]
    elif parts[0] == "Self":
        return "::".join(function.symbol.split("::")[:-1] + parts[1:])
    elif parts[0] == "self":
        parts = function.module.split("::") + parts[1:]
    elif parts[0] == "super":
        parts = function.module.split("::")[:-1] + parts[1:]
    name = "::".join(parts)
    if name in known:
        return name
    for prefix in (function.symbol.rsplit("::", 1)[0], function.module):
        if prefix + "::" + name in known:
            return prefix + "::" + name
    matches = [s for s in known if s.endswith("::" + name)]
    return matches[0] if len(matches) == 1 else name
