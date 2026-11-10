"""Declared struct field types, so a method called on a field resolves to its type's method."""
from .lex import pairs

# Sample input: `&'a mut Option<File>` names Option; `crate::pane::Pane` names Pane; `(u8, u8)` names nothing.
def type_name(ts):
    depth, name = 0, None
    for token in ts:
        if token.value == "<":
            depth += 1
        elif token.value == ">":
            depth -= 1
        elif depth == 0 and token.kind == "ident" and token.value not in ("mut", "dyn", "impl"):
            name = token.value
        elif depth == 0 and token.value in ("(", "["):
            return None
    return name


# Sample input: tokens of "pub struct AirDrop { pub(crate) shared: Shared, devices: Vec<Peer> }".
def struct_fields(units):
    """Each struct's named fields with their type names, and each field name's type when every struct agrees."""
    structs, by_name = {}, {}
    for unit in units:
        ts = unit["tokens"]
        matched = pairs(ts)
        for i, token in enumerate(ts[:-2]):
            if token.value != "struct" or ts[i + 1].kind != "ident":
                continue
            opening = i + 2
            while opening < len(ts) and ts[opening].value not in ("{", ";", "("):
                opening += 1
            if opening == len(ts) or ts[opening].value != "{":
                continue
            fields = structs.setdefault(ts[i + 1].value, {})
            j = opening + 1
            while j < matched[opening]:
                start = j
                # A field runs to the comma outside its own brackets and angle brackets.
                depth = 0
                while j < matched[opening] and not (ts[j].value == "," and depth == 0):
                    if ts[j].value in ("(", "[", "{"):
                        j = matched[j]
                    elif ts[j].value == "<":
                        depth += 1
                    elif ts[j].value == ">":
                        depth -= 1
                    j += 1
                colon = next((k for k in range(start, j) if ts[k].value == ":"
                              and ts[k - 1].kind == "ident" and (k + 1 == j or ts[k + 1].value != ":")
                              and ts[k - 1].value != ":"), None)
                if colon is not None:
                    field, kind = ts[colon - 1].value, type_name(ts[colon + 1:j])
                    # Two structs of one name that disagree on a field leave it unknown.
                    fields[field] = kind if fields.get(field, kind) == kind else None
                    by_name.setdefault(field, set()).add(kind)
                j += 1
    return structs, {field: kinds.pop() for field, kinds in by_name.items() if len(kinds) == 1}


# Sample input: tokens of `self.shared.claim(` with i at `claim`: the chain `self`, `shared` before it.
def chain_before(ts, i):
    chain, j = [], i - 1
    while j >= 1 and ts[j].value == "." and ts[j - 1].kind == "ident":
        chain.append(ts[j - 1].value)
        j -= 2
    if j >= 0 and ts[j].value in (".", "::", ")", "]", ">", "?"):
        return []
    return chain[::-1]


# Sample input: tokens of `let air = &mut shell.menu.providers.airdrop;` bind air to that chain.
def bound_chain(ts, i, name):
    for j in range(i - 1, 0, -1):
        if ts[j].value == name and ts[j - 1].value in ("let", "mut") and j + 1 < len(ts) and ts[j + 1].value == "=":
            k = j + 2
            while k < len(ts) and ts[k].value in ("&", "mut"):
                k += 1
            end = k
            while end + 2 < len(ts) and ts[end].kind == "ident" and ts[end + 1].value == ".":
                end += 2
            if end < len(ts) and ts[end].kind == "ident" and end + 1 < len(ts) and ts[end + 1].value == ";":
                return [t.value for t in ts[k:end + 1:2]]
            return None
    return None


def receiver_type(ts, i, function, fields):
    """The type of a field chain a method is called on: `self.shared` in an AirDrop method is Shared."""
    structs, by_name = fields
    chain = chain_before(ts, i)
    if not chain:
        return None
    root, rest = chain[0], chain[1:]
    kind = None
    if root == "self":
        kind = function.symbol.split("::")[-2] if function.symbol.count("::") >= 2 else None
    else:
        bound = bound_chain(ts, i, root)
        if bound:
            root, rest = bound[0], bound[1:] + rest
            if root == "self":
                kind = function.symbol.split("::")[-2]
    if not rest:
        return None
    # A step whose type is unknown leaves the next field to its name alone.
    for field in rest:
        known = structs.get(kind, {}) if kind else {}
        kind = known[field] if field in known else by_name.get(field)
    return kind
