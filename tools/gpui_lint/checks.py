"""Source rules, independent of the Cargo source provenance check."""
import re
from .lex import pairs, path_at
from .rust import calls, resolve
from .bindings import environments

COLOR_HEX = re.compile(r"0x(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})")
COLOR_TYPES = {"Hsla", "Hsla32", "Rgba", "Rgb", "Color", "Srgb", "Srgba"}
COLOR_CALLS = {"rgb", "rgba", "hsl", "hsla", "hex", "from_hex", "from_rgb", "from_rgba", "from_u32"}
COLOR_SETTERS = {"bg", "background", "border_color", "text_color", "color", "fill", "stroke"}
COLOR_NAMES = set("""aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet
brown burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan
darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred
darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue
dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray
green greenyellow grey honeydew hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon
lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon
lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen magenta
maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen
mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab
orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum
powderblue purple rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna
silver skyblue slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato transparent
turquoise violet wheat white whitesmoke yellow yellowgreen""".split())


# Sample input: crates/flea-gui/tests/g1_03/transport.rs belongs to the g1_03 test crate; every src/ file to the package's own.
# Returns the target and whether another package may call into it: src/main.rs and src/bin/ belong to binaries.
def compilation_target(path):
    parts = path.split("/")
    base = parts.index("crates") + 2 if "crates" in parts else 0
    rest = parts[base:]
    if rest[:1] == ["src"]:
        return "/".join(parts[:base] + ["src"]), rest != ["src", "main.rs"] and rest[:2] != ["src", "bin"]
    elif rest[:1] in (["tests"], ["examples"], ["benches"]) and len(rest) > 1:
        key = rest[0] + "/" + rest[1].removesuffix(".rs")
    else:
        key = "/".join(rest[:1])
    return "/".join(parts[:base] + [key]), False


def blocking(symbol, args, method, helpers):
    parts = symbol.split("::")
    if symbol in helpers or parts[:2] == ["std", "fs"]:
        return True
    if parts[:2] == ["std", "io"] and any(p in parts for p in ("Read", "BufRead", "Write")):
        return parts[-1] in ("read", "read_exact", "read_to_end", "read_to_string", "read_line", "write", "write_all", "flush")
    if parts[:2] == ["reqwest", "blocking"] or parts[:1] in (["ureq"], ["attohttpc"]):
        return parts[-1] in ("get", "post", "put", "delete", "send", "call", "request")
    if parts[:2] in (["std", "net"], ["std", "process"]):
        return parts[-1] in ("connect", "bind", "accept", "send", "send_to", "recv", "recv_from",
                             "read", "write", "spawn", "output", "status", "wait", "wait_with_output")
    if symbol in ("std::thread::sleep", "std::thread::park", "std::thread::park_timeout"):
        return True
    if symbol in ("std::thread::JoinHandle::join", "std::sync::mpsc::Receiver::recv",
                  "std::sync::mpsc::Receiver::recv_timeout"):
        return True
    if parts[-1] in ("block_on", "spawn_blocking"):
        return True
    # gpui's Entity::read(cx) borrows entity state; an I/O read takes a buffer, never the app context.
    if method and parts[-1] == "read" and [t.value for t in args] == ["cx"]:
        return False
    if method and parts[-1] in ("wait", "wait_with_output", "output", "status", "exec", "read", "read_exact", "read_line",
                               "read_to_end", "read_to_string", "write_all", "accept", "connect",
                               "recv_timeout", "send_blocking", "recv_blocking"):
        return True
    return method and parts[-1] in ("join", "recv") and not args


def digits(literal):
    # Rust underscores are digit separators, so 0x1e_29_3b is the colour 0x1e293b.
    return literal.replace("_", "")


def immediate(ts, matched, start, stop):
    # Tokens of one argument list outside nested brackets: a nested call's numbers belong to that call.
    result, i = [], start
    while i < stop:
        if ts[i].value in ("(", "[", "{"):
            i = matched[i] + 1
            continue
        result.append(ts[i])
        i += 1
    return result


def allowed_hex_tokens(unit, entries):
    """Only a named module-level constant can exempt its exact non-colour hex value."""
    ts = unit["tokens"]
    matched = pairs(ts)
    allowed, i = set(), 0
    while i < len(ts):
        if ts[i].value == "const" and i + 1 < len(ts):
            symbol = unit["module"] + "::" + ts[i + 1].value
            end = next((j for j in range(i + 2, len(ts)) if ts[j].value == ";"), len(ts))
            for entry in entries:
                if entry["symbol"] == symbol:
                    allowed.update(j for j in range(i + 2, end) if digits(ts[j].value) == entry["literal"])
        if ts[i].value in ("{", "(", "["):
            i = matched[i]
        i += 1
    return allowed


def source_checks(units, policy):
    errors, known = [], {}
    for unit in units:
        for function in unit["functions"]:
            # A test-only twin never shadows the product function of the same name.
            if not function.test:
                known[function.symbol] = function
    roots = set(policy["ui_callbacks"])
    for name in roots - known.keys():
        errors.append(f"callback registration names no function: {name}")
    roots.update(s for s in known if s.split("::")[-1] in ("paint", "prepaint"))
    helpers = set(policy["blocking_helpers"])
    methods = {name for name, function in known.items()
               if any(t.value == "self" and function.signature[j + 1].value != "::"
                      for j, t in enumerate(function.signature[:-1]))}
    exceptions = {(e["caller"], e["symbol"]): e["reason"] for e in policy["blocking_exceptions"]}
    pending, seen = list(roots), set()
    while pending:
        name = pending.pop()
        if name in seen or name not in known:
            continue
        seen.add(name)
        function = known[name]
        for symbol, line, args, method in calls(function, known):
            target = resolve(symbol, function, (methods if method else set(known)) | helpers)
            # Another target is reached only through a package's library; a test, example, bench, build script or binary is not.
            if target in known:
                callee, library = compilation_target(known[target].path)
                if not library and callee != compilation_target(function.path)[0]:
                    target = symbol
            if (blocking(target, args, method, helpers) and (name, target) not in exceptions
                    and (name, symbol) not in exceptions):
                errors.append(f"{function.path}:{line}: UI blocking call {target} in {name}")
            if target in known:
                pending.append(target)

    exempt = {entry["module"] for entry in policy["color_modules"]}
    surfaces = {entry["path"] for entry in policy["hot_surfaces"]}

    for unit in units:
        if unit["module"] in exempt:
            if unit["elements"] or any(f.symbol in roots for f in unit["functions"]):
                errors.append(f"{unit['path']}: drawing/callback modules cannot be colour parsing exemptions")
            continue
        ts = unit["tokens"]
        matched = pairs(ts)
        scopes = environments(ts)
        allowed = allowed_hex_tokens(unit, policy.get("non_color_hex_constants", []))
        signatures = {id(token) for function in unit["functions"] for token in function.signature}
        image_shadowed = any(t.value in ("mod", "struct", "enum", "type", "trait", "union")
                             and ts[j + 1].value == "image" for j, t in enumerate(ts[:-1]))
        for i, token in enumerate(ts):
            if unit["path"] in surfaces and token.kind == "number" and COLOR_HEX.fullmatch(digits(token.value)) and i not in allowed:
                errors.append(f"{unit['path']}:{token.line}: literal colour-sized hex {token.value}; use theme roles or a named non-colour constant exception")
            if token.kind != "ident" or id(token) in signatures or (i and ts[i - 1].value == "::"):
                continue
            environment = scopes[i]
            aliases = environment.aliases
            path, end = path_at(ts, i, aliases)
            name = path[-1]
            if end == len(ts):
                continue
            following = ts[end].value
            # A bare call under a gpui glob is gpui's unless the scope declares that name; a method or fn declaration is not a call.
            if environment.glob and following == "(" and len(path) == 1 and path[0] not in environment.items and path[0] not in environment.lets and not (i and ts[i - 1].value in (".", "fn")):
                path = ("gpui", path[0])
            image_pixel = path[:2] in (("image", "Rgb"), ("image", "Rgba")) and not image_shadowed
            constructor = not image_pixel and following in ("(", "{") and (name in COLOR_CALLS or any(p in COLOR_TYPES for p in path))
            gpui_symbol = path[0] == "gpui" or path[0].startswith("gpui_") or any(p in COLOR_TYPES for p in path)
            # A named colour is a call, a constant or a function value; a variant of a type that is not a colour type is not one.
            named = gpui_symbol and name.lower() in COLOR_NAMES and not any(p[0].isupper() and p not in COLOR_TYPES for p in path[:-1])
            args = ts[end + 1:matched[end]] if following in ("(", "{") else []
            literal = any(t.kind in ("number", "string") for t in args)
            direct = immediate(ts, matched, end + 1, matched[end]) if following in ("(", "{") else []
            text_color = following == "(" and name in COLOR_SETTERS - {"fill", "stroke"} and (
                any(t.kind == "number" for t in direct) or any(t.kind == "string" and (
                t.value[1:-1].lower() in COLOR_NAMES or t.value[1:-1].startswith("#")) for t in direct)
            )
            if (constructor and literal) or named or text_color:
                errors.append(f"{unit['path']}:{token.line}: literal colour { '::'.join(path) }; use theme roles")

    by_path = {unit["path"]: unit for unit in units}
    registry = policy["hot_surfaces"]
    if {r["name"] for r in registry} != {"list", "columns", "grid", "long_menus"}:
        errors.append("hot surfaces must register list, columns, grid and long_menus")
    active = 0
    for entry in registry:
        unit = by_path.get(entry["path"])
        if entry.get("pending"):
            if unit:
                errors.append(f"{entry['path']}: pending surface exists; register its Element and row_height")
            continue
        active += 1
        if not unit or entry["element"] not in unit["elements"]:
            errors.append(f"{entry['path']}: registered hot surface has no custom Element {entry['element']}")
            continue
        methods = {f.symbol.rsplit("::", 1)[-1] for f in unit["functions"]
                   if not f.test and f.symbol.rsplit("::", 2)[-2] == entry["element"]}
        if not {"request_layout", "prepaint", "paint"} <= methods:
            errors.append(f"{entry['path']}: custom Element must implement layout, prepaint and paint")
        # The constant may live in a sibling module the Element file names, recorded as row_height_path.
        height_unit = by_path.get(entry.get("row_height_path", entry["path"]))
        values = [t.value for t in height_unit["tokens"] if t.kind in ("ident", "punct")] if height_unit else []
        if not any(values[i:i + 2] == ["const", entry["row_height"]] for i in range(len(values) - 1)):
            errors.append(f"{entry.get('row_height_path', entry['path'])}: missing fixed row height {entry['row_height']}")
        elif height_unit is not unit and not any(t.kind == "ident" and t.value == entry["row_height"] for t in unit["tokens"]):
            errors.append(f"{entry['path']}: Element file does not use its fixed row height {entry['row_height']}")
        pending_hot = [f.symbol for f in unit["functions"]]
        seen_hot = set()
        while pending_hot:
            name = pending_hot.pop()
            if name in seen_hot or name not in known:
                continue
            seen_hot.add(name)
            function = known[name]
            for symbol, line, _, method in calls(function, known):
                if symbol.split("::")[-1] in ("div", "child", "children", "uniform_list"):
                    errors.append(f"{function.path}:{line}: hot surface builds child tree with {symbol}")
                target = resolve(symbol, function, known)
                if target in known:
                    pending_hot.append(target)
    if not units or not active:
        errors.append("empty native source or hot-surface scan")
    return sorted(set(errors))
