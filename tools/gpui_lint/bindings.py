"""Lexical imports and local callable aliases, without literal-value dataflow."""
from dataclasses import dataclass
from .lex import pairs, path_at, scope_aliases


@dataclass
class Environment:
    aliases: dict


def environments(ts, aliases=None):
    """Imports are block items; lets take effect after their initializer, within that block."""
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

    def walk(start, end, parent, module=False):
        imports = scope_aliases(ts[start:end], parent.aliases, module)
        environment = Environment(imports)
        pending = {}
        i = start
        while i < end:
            result[i] = environment
            value = ts[i].value
            if value == "let" and (i == start or ts[i - 1].value not in ("if", "while")):
                stop = statement_end(i, end)
                item = binding(i, stop)
                if item and stop < end:
                    pending[stop] = item
            elif ts[i].kind == "ident" and i + 1 < end and ts[i + 1].value == "=":
                if i == start or ts[i - 1].value in (";", "}"):
                    stop = statement_end(i, end)
                    if stop < end:
                        pending[stop] = (value, ts[i + 2:stop])
            if value in ("{", "(", "["):
                close = matched[i]
                child_module = i >= 2 and ts[i - 2].value == "mod"
                walk(i + 1, close, environment, child_module)
                result[close] = environment
                i = close
            if i in pending:
                name, expression = pending[i]
                imports = dict(environment.aliases)
                while expression and expression[0].value == "(":
                    if pairs(expression)[0] != len(expression) - 1:
                        break
                    expression = expression[1:-1]
                path, last = path_at(expression, 0, imports) if expression else ((), 0)
                if path and last == len(expression):
                    imports[name] = path
                else:
                    imports.pop(name, None)
                environment = Environment(imports)
            i += 1

    walk(0, len(ts), Environment(dict(aliases or {})))
    return result
