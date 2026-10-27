#!/usr/bin/env python3
# Source-level checks use the standard library and Qt's compiler and diagnostic IDs.
import argparse
import collections
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tarfile

COUNTED_WORKER = 'ui/CountedWorker.qml'
GATES = ('conflict-marker', 'fused-line', 'paneprops', 'del-printable', 'qml-undeclared-read', 'qml-duplicate-member', 'worker-announce', 'commons-color', 'extern-variadic', 'pane-function-value', 'dead-module-call', 'worker-report', 'easing', 'ellipsis')
SUFFIXES = {'.rs', '.qml', '.js', '.sh'}
# Bound alias propagation so pathological chains cannot keep a source gate running.
ALIAS_FIXPOINT_CAP = 8
# Allow the whole QML tree to finish while bounding a stalled qmllint process.
QMLLINT_TIMEOUT_SECONDS = 120
# Keep a missing-JSON diagnostic readable even when qmllint emits a long error.
STDERR_EXCERPT_LENGTH = 200
# Limit simultaneous Qt compilers and bound each source file's compilation.
QMLCACHEGEN_POOL_SIZE = 4
QMLCACHEGEN_TIMEOUT_SECONDS = 15
QMLCACHEGEN_PATHS = ('/usr/lib/qt6/qmlcachegen', '/usr/lib/qt6/libexec/qmlcachegen')
# One stub import root per module under QMLLINT_STUBS: qs.Commons is always on the import path, each probed module only where the import probe finds the real one missing.
QMLLINT_STUBS = 'tests/staticgates-stubs'
QMLLINT_ALWAYS_STUB = 'qs.Commons'
QMLLINT_PROBED_MODULES = ('Quickshell', 'QtQuick.Shapes', 'QtQuick.Layouts')


def inventory(root, tracked=False):
    args = ['git', '-C', str(root), 'ls-files', '-z']
    if not tracked:
        args += ['--cached', '--others', '--exclude-standard']
    try:
        result = subprocess.run(args, capture_output=True)
    except FileNotFoundError:
        result = None
    if result is not None and result.returncode == 0:
        return sorted(set(p for p in result.stdout.decode().split('\0') if p))
    # The CI lane exports Git's tree to /work/flea and mounts its authoritative tar at /in/tree.tar.
    archive = Path(os.environ.get('FLEA_SOURCE_ARCHIVE', '/in/tree.tar'))
    if not archive.is_file():
        raise ValueError('source inventory requires Git or FLEA_SOURCE_ARCHIVE (CI uses /in/tree.tar)')
    with tarfile.open(archive) as source:
        paths = {member.name for member in source.getmembers() if member.isfile() or member.issym()}
    if any(Path(path).is_absolute() or '..' in Path(path).parts for path in paths):
        raise ValueError('source archive contains a path outside its root')
    return sorted(paths)


# Sample input: var text = "quoted"; /* comment */ run()
def masked(source, suffix, fill='~'):
    # Keep offsets and newlines so diagnostics always point at the original source.
    out = list(source)
    i = 0
    while i < len(source):
        start = i
        raw = re.match(r'r(\#*)"', source[i:]) if suffix == '.rs' else None
        if raw:
            end = source.find('"' + raw[1], i + len(raw[0]))
            i = len(source) if end < 0 else end + 1 + len(raw[1])
        elif source.startswith('//', i) and suffix != '.sh' or source[i] == '#' and suffix == '.sh' and (i == 0 or source[i - 1].isspace()):
            i = source.find('\n', i)
            if i < 0:
                i = len(source)
        elif source.startswith('/*', i) and suffix != '.sh':
            depth = 1
            i += 2
            while i < len(source) and depth:
                if suffix == '.rs' and source.startswith('/*', i):
                    depth += 1
                    i += 2
                elif source.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        elif suffix == '.sh' and source[i] == '\\':
            # Sample input: echo \' ; x    y keeps the quote literal and the following code visible.
            i += 2
        elif source[i] in '"\'`' and not (suffix == '.rs' and source[i] == "'" and not re.match(r"'(?:\\.|[^'\\\n])'", source[i:])):
            quote = source[i]
            # Sample input: $'it\'s' is ANSI-C; \$'a\' keeps its backslash literal.
            literal_backslash = suffix == '.sh' and quote == "'"
            if literal_backslash and i > 0 and source[i - 1] == '$':
                backslashes = 0
                at = i - 2
                while at >= 0 and source[at] == '\\':
                    backslashes += 1
                    at -= 1
                literal_backslash = backslashes % 2 != 0
            i += 1
            while i < len(source):
                if source[i] == '\\' and not literal_backslash:
                    i += 2
                elif source[i] == quote:
                    i += 1
                    break
                else:
                    i += 1
        elif (source[i] == '/' and suffix in ('.js', '.qml')
              and re.search(r'(?:^|[=(,:!\[{};?]|\breturn)\s*$', source[:i])):
            # A regex literal is data, including its character classes and escaped slashes.
            i += 1
            bracket = False
            while i < len(source) and source[i] != '\n':
                if source[i] == '\\':
                    i += 2
                elif source[i] == '/' and not bracket:
                    i += 1
                    while i < len(source) and source[i].isalpha():
                        i += 1
                    break
                else:
                    if source[i] == '[':
                        bracket = True
                    elif source[i] == ']':
                        bracket = False
                    i += 1
        else:
            i += 1
            continue
        for at in range(start, min(i, len(source))):
            if source[at] != '\n':
                out[at] = fill
    text = ''.join(out)
    if suffix == '.sh':
        # Literal heredoc bodies are fixture data or child-script strings, not shell tokens.
        lines = text.splitlines(keepends=True)
        originals = source.splitlines(keepends=True)
        until = None
        arithmetic_depth = 0
        for n, line in enumerate(originals):
            if until is not None:
                lines[n] = re.sub(r'[^\n]', '~', line)
                if line.strip() == until:
                    until = None
            else:
                # Sample input: (( a << 2 )); cat <<'EOF' opens a heredoc only at the second <<.
                for token in re.finditer(r'\(\(|[()]|(?<!<)<<-?(?!<)', lines[n]):
                    if token[0] == '((':
                        arithmetic_depth += len(token[0])
                    elif arithmetic_depth:
                        arithmetic_depth += (token[0] == '(') - (token[0] == ')')
                    elif token[0].startswith('<<'):
                        # Sample input: <<'EOF' reads its delimiter from the original opener position.
                        match = re.match(r'<<-?[ \t]*[\'"]?(\w+)[\'"]?', line[token.start():])
                        if match:
                            until = match[1]
                            break
        text = ''.join(lines)
    return text


def member_code(source, suffix):
    code = list(masked(source, suffix))
    # Quoted object keys and constant bracket members are declarations/accesses, not string payloads.
    for m in re.finditer(r'([\'"])([A-Za-z_$][\w$]*)\1\s*:', source):
        if re.search(r'[,{]\s*$', ''.join(code[:m.start()])):
            code[m.start():m.start() + len(m[2]) + 2] = ' ' + m[2] + ' '
    for m in re.finditer(r'\b([\w.]+)\[\s*([\'"])([A-Za-z_$][\w$]*)\2\s*\]', source):
        if ''.join(code[m.start():m.start() + len(m[1])]) == m[1]:
            replacement = m[1] + '.' + m[3]
            code[m.start():m.end()] = replacement.ljust(m.end() - m.start())
    return ''.join(code)


def location(file, text, at):
    return f'{file}:{text.count(chr(10), 0, at) + 1}'


def conflict_marker(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if not path.is_file() or path.is_symlink():
            continue
        data = path.read_bytes()
        if b'\0' in data:
            continue
        try:
            source = data.decode('utf-8', errors='replace')
        except UnicodeDecodeError:
            continue
        scanned += 1
        markers, separators = [], None
        for n, line in enumerate(source.splitlines(), 1):
            if re.fullmatch(r'(?:<<<<<<< .*|>>>>>>> .*|\|\|\|\|\|\|\| .*)', line):
                markers.append(n)
                if line.startswith('<<<<<<< '):
                    separators = []
                elif line.startswith('>>>>>>> ') and separators is not None:
                    markers.extend(separators)
                    separators = None
            elif line == '=======' and separators is not None:
                separators.append(n)
        errors.extend(f'{file}:{n}: unresolved conflict marker' for n in sorted(markers))
    return scanned, errors


def fused_line(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if path.suffix not in SUFFIXES or not path.is_file():
            continue
        scanned += 1
        code = masked(path.read_text(), path.suffix)
        for m in re.finditer(r'(?<=[^\s~]) {4,}(?=[^\s~])', code):
            errors.append(f'{location(file, code, m.start())}: fused code gap ({len(m[0])} spaces)')
    return scanned, errors


# Sample input: RESULT.md at the tree root is rejected; docs/notes.md passes.
def worker_report(root, files):
    errors = []
    scanned = 0
    for file in files:
        if not (root / file).is_file():
            continue
        scanned += 1
        if '.superpowers' in Path(file).parts:
            errors.append(f'{file}:1: worker report tracked at .superpowers')
        elif '/' not in file and (re.fullmatch(r'RESULT.*\.md', file) or re.fullmatch(r'REPORT.*\.md', file)
                                  or re.fullmatch(r'STUDY.*\.md', file) or file == 'RUN.md'
                                  or re.fullmatch(r'PREMISE.*\.txt', file)):
            errors.append(f'{file}:1: worker report tracked at root')
    return scanned, errors


def pane_members(root):
    code = masked((root / 'ui/Pane.qml').read_text(), '.qml')
    depth, depths = 0, []
    for char in code:
        depths.append(depth)
        depth += (char == '{') - (char == '}')
    declarations = r'\bproperty\s+\w+\s+(\w+)\s*:|\b(?:function|signal)\s+(\w+)\s*\('
    members = {m[1] or m[2] for m in re.finditer(declarations, code) if depths[m.start()] == 1}
    # FocusScope inherits Item; these names come from QtQuick's shipped qmltypes, not a hand list.
    qmltypes = Path('/usr/lib/x86_64-linux-gnu/qt6/qml/QtQuick/plugins.qmltypes')
    if not qmltypes.exists():
        qmltypes = Path('/usr/lib/qt6/qml/QtQuick/plugins.qmltypes')
    types = qmltypes.read_text()
    types += (qmltypes.parent.parent / 'builtins.qmltypes').read_text()
    for name in ('QQuickItem', 'QQuickFocusScope', 'QObject'):
        for match in re.finditer(r'Component\s*\{\s*\n\s*file:.*?\n\s*name: "' + name + '"', types):
            start = types.rfind('Component', 0, match.end())
            end = types.find('\n    Component', match.end())
            block = types[start:end if end >= 0 else len(types)]
            members.update(re.findall(r'(?:Property|Method|Signal)\s*\{\s*name: "(\w+)"', block))
    return members


def pane_references(code, file, members):
    refs = set()
    # QML Pane instances and properties are explicit; SettingsPanel's id pane is a SettingsPane.
    refs.update(re.findall(r'(?:Flea\.)?Pane\s*\{\s*id:\s*(\w+)', code))
    refs.update(re.findall(r'property\s+(?:var|Item|Pane)\s+(\w*[Pp]ane)\b', code))
    if file == 'ui/Pane.qml':
        refs.add('root')
    if file.endswith('.js'):
        refs.add('pane')
        # Root/p are conventional JS argument names; distinctive Pane accesses identify their role.
        for ref in set(re.findall(r'\b(\w+)\.(?:filterQuery|focusView|listInFlight|searchMode|selectionVersion|recentMode)\b', code)):
            refs.add(ref)
    refs.update(re.findall(r'\b(?:root\.)?(?:pane|currentPane|primaryPane|railPane|editPane)\b', code))
    for m in re.finditer(r'\b([\w.]+)\.(?:pane|currentPane|primaryPane|railPane|editPane)\b', code):
        refs.add(m[0])
    if file == 'ui/SettingsPanel.qml':
        refs.discard('pane')
    # Aliases retain their Pane role, including member references such as root.pane and holder.pane.
    for _ in range(ALIAS_FIXPOINT_CAP):
        before = set(refs)
        for m in re.finditer(r'\b(?:var|let|const)\s+(\w+)\s*=\s*([^\n;]+)', code):
            expression = m[2].split('?', 1)[-1]
            if any(re.search(r'(?<![\w.])' + re.escape(ref) + r'(?![\w.])', expression) for ref in refs):
                if re.fullmatch(r'[\w.\s|&?:]+', m[2].strip()):
                    refs.add(m[1])
        if refs == before:
            break
    return refs


def arguments(code, start, end):
    values = []
    depth = 0
    at = start
    for i in range(start, end):
        if code[i] in '({[':
            depth += 1
        elif code[i] in ')}]':
            depth -= 1
        elif code[i] == ',' and depth == 0:
            values.append(code[at:i].strip())
            at = i + 1
    values.append(code[at:end].strip())
    return values


def pane_flow(root, files, members):
    sources, scopes, imports, functions, calls = {}, {}, {}, {}, []
    for file in files:
        path = root / file
        if path.suffix not in ('.js', '.qml') or not file.startswith(('ui/', 'tests/js/')) or not path.is_file():
            continue
        source = path.read_text()
        code = member_code(source, path.suffix)
        sources[file] = code
        imports[file] = {alias: str((path.parent / name).resolve().relative_to(root))
                         for name, alias in re.findall(r'import\s+"([^"\n]+\.js)"\s+as\s+(\w+)', source)}
        top = {'start': 0, 'end': len(code), 'params': [], 'refs': set(), 'returns': False}
        if path.suffix == '.qml':
            top['refs'] = pane_references(code, file, members)
        scopes[file] = [top]
        for m in re.finditer(r'\bfunction\s*(\w*)\s*\(([^)]*)\)\s*(?::\s*\w+\s*)?\{', code):
            end = closing(code, m.end() - 1, '{', '}')
            scope = {'start': m.end(), 'end': end, 'params': [a.strip().split(':')[0] for a in m[2].split(',')],
                     'refs': set(), 'returns': False}
            scopes[file].append(scope)
            if m[1]:
                functions[(file, m[1])] = scope
        for scope in scopes[file]:
            if scope is top:
                continue
            body = list(code[scope['start']:scope['end']])
            for child in scopes[file]:
                if scope['start'] < child['start'] < child['end'] < scope['end']:
                    body[child['start'] - scope['start']:child['end'] - scope['start']] = '~' * (child['end'] - child['start'])
            body = ''.join(body)
            scope['body'] = body
            scope['refs'] = pane_references(body, file, members)
            scope['refs'] = {ref for ref in scope['refs'] if ref in scope['params'] or re.search(r'(?<![\w.])' + re.escape(ref) + r'(?!\w)', body)}
            scope['returns'] = bool(re.search(r'\breturn\s+(' + '|'.join(re.escape(r) for r in scope['refs']) + r')\s*(?:;|$)', body)) if scope['refs'] else False
            scope['returns'] |= bool(re.search(r'\breturn\s*\{', body) and stub_objects(body))
            if re.search(r'function\s+(?:\w*[Pp]ane\w*|root)\s*\(', code[max(0, scope['start'] - 100):scope['start']]):
                returned = re.search(r'\breturn\s+(\w+)\s*$', body)
                if returned:
                    scope['returns'] = True
                    scope['refs'].add(returned[1])
        for m in re.finditer(r'\b(\w+)(?:\.(\w+))?\s*\(', code):
            if re.search(r'function\s*$', code[:m.start()]):
                continue
            target = (imports[file].get(m[1]), m[2]) if m[2] else (file, m[1])
            end = closing(code, m.end() - 1, '(', ')')
            assignment = re.search(r'\b(?:var|let|const)\s+(\w+)\s*=\s*$', code[:m.start()])
            calls.append((file, m.start(), target, arguments(code, m.end(), end), assignment[1] if assignment else None))

    def at(file, offset):
        candidates = [scope for scope in scopes[file] if scope['start'] <= offset <= scope['end']]
        return min(candidates, key=lambda scope: scope['end'] - scope['start'])

    def references(file, offset):
        result = set()
        for scope in sorted(scopes[file], key=lambda scope: scope['start']):
            if scope['start'] <= offset <= scope['end']:
                result.difference_update(scope['params'])
                result.update(scope['refs'])
        return result

    # Pane roles flow through scoped function arguments and factory results, never through child objects.
    for _ in range(len(sources)):
        changed = False
        for file, offset, target, values, assignment in calls:
            if target not in functions:
                continue
            local = at(file, offset)
            dest = functions[target]
            known = references(file, offset)
            if assignment and dest['returns'] and assignment not in local['refs']:
                local['refs'].add(assignment)
                changed = True
            for value, parameter in zip(values, dest['params']):
                if value in ('null', 'undefined', 'true', 'false') or not re.fullmatch(r'[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*', value):
                    continue
                if value in known and parameter not in dest['refs'] and not (file.startswith('tests/') and target[0].startswith('ui/')):
                    dest['refs'].add(parameter)
                    changed = True
                if parameter in dest['refs'] and value not in known:
                    local['refs'].add(value)
                    changed = True
        for file, entries in scopes.items():
            for scope in entries[1:]:
                known = references(file, scope['start'])
                for m in re.finditer(r'\b(?:var|let|const)\s+(\w+)\s*=\s*([\w.]+)\s*(?:;|$)', scope['body'], re.MULTILINE):
                    if m[2] in known and m[1] not in scope['refs']:
                        scope['refs'].add(m[1])
                        changed = True
        if not changed:
            break
    return sources, references


def paneprops(root, files, sample=False):
    members = pane_members(root)
    errors = []
    scanned = 0
    allowances = {} if sample else stub_allowances()
    used = set()
    sources, references = pane_flow(root, files, members)
    for file in files:
        path = root / file
        if path.suffix not in ('.js', '.qml') or not file.startswith(('ui/', 'tests/js/')) or not path.is_file():
            continue
        scanned += 1
        code = sources[file]
        for m in re.finditer(r'\b([\w.]+)\.(\w+)\s*(?:=(?!=)|\+=|-=|\*=|/=|\+\+|--)', code):
            if m[1] not in references(file, m.start()) or m[2] in members:
                continue
            key = (file, m[2])
            if key in allowances and file.startswith('tests/js/'):
                used.add(key)
            else:
                errors.append(f'{location(file, code, m.start())}: {m[1]}.{m[2]} absent from Pane/FocusScope')
        if file.startswith('tests/js/'):
            # Object-literal fields in Pane factories count even when production only reads them.
            for start, end in stub_objects(code):
                for field, at in literal_fields(code, start, end):
                    if field in members:
                        continue
                    key = (file, field)
                    if key in allowances:
                        used.add(key)
                    else:
                        errors.append(f'{location(file, code, at)}: stub field {field} absent from Pane/FocusScope')
    for key in allowances.keys() - used:
        if (root / key[0]).is_file():
            errors.append(f'{key[0]}: stale stub instrumentation allowance {key[1]}')
    return scanned, sorted(set(errors))


# Sample input: "    function menuSelectionIdentity() {" at brace depth 1 in ui/Pane.qml names a Pane function.
def pane_functions(root):
    code = masked((root / 'ui/Pane.qml').read_text(), '.qml')
    depth, depths = 0, []
    for char in code:
        depths.append(depth)
        depth += (char == '{') - (char == '}')
    return {m[1] for m in re.finditer(r'\bfunction\s+(\w+)\s*\(', code) if depths[m.start()] == 1}


# Sample input: id !== pane.identityOf reads a Pane function without its call; id !== pane.identityOf() passes.
def pane_function_value(root, files, sample=False):
    functions = pane_functions(root)
    errors = []
    scanned = 0
    sources, references = pane_flow(root, files, functions)
    for file in files:
        path = root / file
        if path.suffix not in ('.js', '.qml') or not file.startswith(('ui/', 'tests/js/')) or not path.is_file():
            continue
        scanned += 1
        code = sources[file]
        for m in re.finditer(r'\b([\w.]+)\.(\w+)', code):
            if m[2] not in functions or m[1] not in references(file, m.start()):
                continue
            if re.match(r'\s*\(', code[m.end():]):
                continue
            if re.search(r'\btypeof\s*\(?\s*$', code[:m.start()]):
                continue
            left = re.search(r'(===|!==|==|!=)\s*$', code[:m.start()])
            right = re.match(r'\s*(===|!==|==|!=)', code[m.end():])
            if left or right:
                errors.append(f'{location(file, code, m.start())}: {m[1]}.{m[2]} compared without its call')
    return scanned, sorted(set(errors))


# Sample input: "var A = 1, B = f(2, 3),\n    C = [4]" names A, B and C; a comma inside brackets starts no name.
def declared_names(blank, start):
    names, depth, expecting = [], 0, True
    # blank is the module with strings and comments spaced out, so a comma or bracket inside one starts no name.
    tail = blank[start:]
    for m in re.finditer(r'[(){}\[\],;\n]|[A-Za-z_$][\w$]*|[^\s]', tail):
        token = m[0]
        if token in '([{':
            depth += 1
        elif token in ')]}':
            depth -= 1
        elif depth == 0 and token == ';':
            break
        elif depth == 0 and token == '\n':
            # A list goes on past a line that ends in a comma or whose next token is one.
            if not re.search(r',\s*$', tail[:m.start()]) and not re.match(r'\s*,', tail[m.end():]):
                break
        elif depth == 0 and token == ',':
            expecting = True
        elif depth == 0 and expecting and re.fullmatch(r'[A-Za-z_$][\w$]*', token):
            names.append(token)
            expecting = False
        else:
            expecting = False
    return names


# Sample input: "function start(root) {", "async function go() {" or "var NOTE = {" at column 0 names a module member; indented declarations are local.
def module_names(code):
    blank = masked(code, '.js', ' ')
    names = {m[1] for m in re.finditer(r'^(?:async\s+)?function\s+(\w+)\s*\(', blank, re.M)}
    for m in re.finditer(r'^(?:var|let|const)\s', blank, re.M):
        names.update(declared_names(blank, m.end()))
    return names


# Sample input: Search.run(pane) after import "js/Search.js" as Search is rejected when Search.js defines no function run; Search.start(pane) passes.
def dead_module_call(root, files):
    modules = {}
    for file in files:
        path = root / file
        if file.startswith('ui/') and file.endswith('.js') and path.is_file():
            source = path.read_text()
            if re.search(r'^\s*\.pragma\s+library\b', masked(source, '.js'), re.M):
                modules[file] = module_names(source)
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if path.suffix not in ('.js', '.qml') or not file.startswith('ui/') or not path.is_file():
            continue
        scanned += 1
        source = path.read_text()
        code = masked(source, path.suffix)
        for imported in re.finditer(r'^\s*\.?import\s+"([^"]+\.js)"\s+as\s+(\w+)', source, re.M):
            module = os.path.normpath(os.path.join(os.path.dirname(file), imported[1]))
            alias = imported[2]
            # A local named like the alias hides the module there, so the file is not judged for it; a call argument is no local.
            name = r'\b' + alias + r'\b'
            local = r'\b(?:var|let|const|function)\s+' + alias + r'\b|(?:\bfunction\b[^(){}]*|\bcatch\s*)\([^()]*' + name + r'[^()]*\)|\([^()]*' + name + r'[^()]*\)\s*=>|' + name + r'\s*=>'
            if module not in modules or re.search(local, code):
                continue
            for call in re.finditer(r'(?<![\w.$])' + alias + r'\.(\w+)\s*\(', code):
                if call[1] not in modules[module]:
                    errors.append(f'{location(file, code, call.start())}: {alias}.{call[1]} is called but {module} defines no function {call[1]}')
    return scanned, errors


def literal_fields(code, start, end):
    depth = 0
    previous = '{'
    for m in re.finditer(r'[{}(),?:]|[A-Za-z_$][\w$]*|[^\s~]', code[start:end]):
        token = m[0]
        if depth == 0 and previous in ('{', ',') and re.fullmatch(r'[A-Za-z_$][\w$]*', token):
            if re.match(r'\s*:', code[start + m.end():end]):
                yield token, start + m.start()
        if token in '{(':
            depth += 1
        elif token in '})':
            depth -= 1
        if depth == 0:
            previous = token


def stub_objects(code):
    spans, owners = [], []
    for m in re.finditer(r'function\s*(\w*)\s*\([^)]*\)\s*\{', code):
        owners.append((m.end(), closing(code, m.end() - 1, '{', '}'), m[1]))
    for m in re.finditer(r'\{', code):
        prefix = code[:m.start()]
        if not re.search(r'(?:=|return|\(|,)\s*$', prefix):
            continue
        end = closing(code, m.start(), '{', '}')
        fields = {name for name, _ in literal_fields(code, m.end(), end)}
        containing = [owner for owner in owners if owner[0] <= m.start() < owner[1]]
        owner = min(containing, key=lambda entry: entry[1] - entry[0]) if containing else None
        factory = owner and re.search(r'[Pp]ane|^root$', owner[2]) and re.search(r'(?:return|\b(?:var|let|const)\s+\w+\s*=)\s*$', prefix)
        if factory or len(fields & {'cursorIndex', 'focusView', 'filterQuery', 'searchMode', 'listInFlight', 'recentMode', 'rowFor', 'pendingSelect'}) >= 2:
            spans.append((m.end(), end))
    return spans


def closing(code, start, left, right):
    depth = 0
    for i in range(start, len(code)):
        if code[i] == left:
            depth += 1
        elif code[i] == right:
            depth -= 1
            if depth == 0:
                return i
    return len(code)


def stub_allowances():
    # Only test observation fields belong here; production Pane writes can never use this list.
    return {
        ('tests/js/archivecards.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/archivecards.js', 'stuck'): 'Records sticky lines the stub status shows.',
        ('tests/js/clipboard.js', 'asked'): 'Records collision requests and captured cut snapshots.',
        ('tests/js/clipboard.js', 'localAtSend'): 'Records the local clipboard when the backend request is sent.',
        ('tests/js/clipboard.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/clipboard.js', 'sent'): 'Records system clipboard requests emitted by the stub backend.',
        ('tests/js/columns.js', 'refreshed'): 'Records refresh calls through stub methods.',
        ('tests/js/columns.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/columns.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/columns.js', 'stuck'): 'Records the window offset held by a stub backend.',
        ('tests/js/cursorreturn.js', 'sentLocate'): 'Records locate requests the windowed landing emits.',
        ('tests/js/cursorreturn.js', 'sentWindow'): 'Records window requests the located reply emits.',
        ('tests/js/ddtarget.js', 'askLog'): 'Records request rows and menu identities.',
        ('tests/js/ddtarget.js', 'asked'): 'Records request calls through stub methods.',
        ('tests/js/ddtarget.js', 'moved'): 'Records setCursor calls through stub methods.',
        ('tests/js/ddtarget.js', 'openedPath'): 'Records the path requested through open.',
        ('tests/js/ddtarget.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/filter-cursor.js', 'said'): 'Records stub message emissions.',
        ('tests/js/filter-cursor.js', 'trashed'): 'Records backend.trash calls.',
        ('tests/js/filterfixture.js', 'contexts'): 'Records showRow or pointer dispatch context arguments.',
        ('tests/js/filterfixture.js', 'picked'): 'Stores the stub selection behind selection methods.',
        ('tests/js/filterfixture.js', 'scrolled'): 'Records the row requested through showRow.',
        ('tests/js/filterwire.js', 'dropped'): 'Records swap.drop calls.',
        ('tests/js/focus-forward.js', 'rowsRead'): 'Records rowFor inputs.',
        ('tests/js/focus-forward.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/focus.js', 'clipRequests'): 'Records system clipboard requests dispatched by menu actions.',
        ('tests/js/focus-lines.js', 'picked'): 'Stores the stub selection behind selection methods.',
        ('tests/js/focus-lines.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/focus-lines.js', 'sentQueries'): 'Records backend.searchQuery calls through stub methods.',
        ('tests/js/focus-lines.js', 'openedScopes'): 'Records backend.searchOpen calls through stub methods.',
        ('tests/js/focus-lines.js', 'closedSessions'): 'Records backend.searchClose calls through stub methods.',
        ('tests/js/focus-lines.js', 'relisted'): 'Records openWithoutHistory calls through stub methods.',
        ('tests/js/focus-lines.js', 'cancelledCalls'): 'Records backend.searchCancel calls through stub methods.',
        ('tests/js/focus.js', 'relisted'): 'Records openWithoutHistory calls through stub methods.',
        ('tests/js/focus.js', 'acted'): 'Records act calls through stub methods.',
        ('tests/js/focus.js', 'asked'): 'Records request calls through stub methods.',
        ('tests/js/focus.js', 'cancelled'): 'Records cancel calls through stub methods.',
        ('tests/js/focus.js', 'climbed'): 'Records openParent calls through stub methods.',
        ('tests/js/focus.js', 'copied'): 'Records copy calls through stub methods.',
        ('tests/js/focus.js', 'hiddenAsked'): 'Records ejectHidden calls.',
        ('tests/js/focus.js', 'isError'): 'Records the error argument of the stub message signal.',
        ('tests/js/focus.js', 'linked'): 'Records pasteLink calls.',
        ('tests/js/focus.js', 'listed'): 'Records openWithoutHistory arguments.',
        ('tests/js/focus.js', 'made'): 'Records backend.mkdir calls through stub methods.',
        ('tests/js/focus.js', 'openedCopyAs'): 'Records openCopyAsFlyout calls.',
        ('tests/js/focus.js', 'openedPasteAs'): 'Records openPasteAsFlyout calls.',
        ('tests/js/focus.js', 'parked'): 'Records the view position showRow was handed by Escape over a filter.',
        ('tests/js/focus.js', 'pasted'): 'Records paste calls through stub methods.',
        ('tests/js/focus.js', 'retreated'): 'Records escapePressed calls through stub methods.',
        ('tests/js/focus.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/focus.js', 'started'): 'Records start calls through stub methods.',
        ('tests/js/foldersorts.js', 'forgotten'): 'Records backend.forgetSort calls.',
        ('tests/js/foldersorts.js', 'remembered'): 'Records backend.rememberSort calls.',
        ('tests/js/foldersorts.js', 'sent'): 'Records requests from stub backend methods.',
        ('tests/js/foldersorts.js', 'sorts'): 'Records backend.sort calls.',
        ('tests/js/keyreach.js', 'pathBars'): 'Records pathBarRequested calls through stub methods.',
        ('tests/js/keyreach.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/keyreach.js', 'switched'): 'Records switchPane calls through stub methods.',
        ('tests/js/marquee.js', 'contexts'): 'Records showRow or pointer dispatch context arguments.',
        ('tests/js/menu.js', 'said'): 'Records stub message emissions.',
        ('tests/js/nav.js', 'cancelled'): 'Records cancel calls through stub methods.',
        ('tests/js/nav.js', 'cleared'): 'Records clearSelection calls through stub methods.',
        ('tests/js/nav.js', 'refuses'): 'Injects a refusal in the stub backend.list method.',
        ('tests/js/nav.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/nav.js', 'selected'): 'Records the row the stub backend.list request names to select.',
        ('tests/js/nav.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/navnewest.js', 'cleared'): 'Counts clearSelection calls through stub methods.',
        ('tests/js/navnewest.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/navnewest.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/navnewest.js', 'stopped'): 'Counts the waiting listings the stub swap was told to let go.',
        ('tests/js/ops.js', 'asked'): 'Records request calls through stub methods.',
        ('tests/js/ops.js', 'permitted'): 'Records requested permission-dialog arguments.',
        ('tests/js/ops.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/ops.js', 'seen'): 'Records captured stub method arguments.',
        ('tests/js/pathbar.js', 'asked'): 'Records request calls through stub methods.',
        ('tests/js/pickerkeys.js', 'accept'): 'Records the picker window accept call.',
        ('tests/js/pickerkeys.js', 'accepted'): 'Records the paths each accept call carried.',
        ('tests/js/pickerkeys.js', 'cancel'): 'Records the picker window cancel call.',
        ('tests/js/pickerkeys.js', 'cancelled'): 'Records whether the picker window was cancelled.',
        ('tests/js/pickerkeys.js', 'marks'): 'Holds the checked paths of the stub picker window.',
        ('tests/js/pickerkeys.js', 'toggleMark'): 'Records the picker window toggleMark call.',
        ('tests/js/pickerkeys.js', 'toggled'): 'Records the indices toggleMark was called with.',
        ('tests/js/placemenu.js', 'copied'): 'Records copy calls through stub methods.',
        ('tests/js/placemenu.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/placemenu.js', 'tabbed'): 'Records openNewTab calls.',
        ('tests/js/placemenu.js', 'terminal'): 'Records openTerminal arguments.',
        ('tests/js/previewkeys.js', 'closed'): 'Records preview.close calls through stub methods.',
        ('tests/js/previewkeys.js', 'played'): 'Records preview.play calls through stub methods.',
        ('tests/js/previewkeys.js', 'switched'): 'Records preview switch calls through stub methods.',
        ('tests/js/railkeys.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/recentmode.js', 'cancelled'): 'Records cancel calls through stub methods.',
        ('tests/js/recentmode.js', 'holds'): 'Records swap.hold requests.',
        ('tests/js/recentmode.js', 'listed'): 'Records list or open requests.',
        ('tests/js/recentmode.js', 'superseded'): 'Records whether the last openWithoutHistory passed supersede.',
        ('tests/js/recentmode.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/reclick.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/reclick.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/reload.js', 'asked'): 'Records request calls through stub methods.',
        ('tests/js/reload.js', 'listed'): 'Records list or open requests.',
        ('tests/js/reload.js', 'refreshed'): 'Records refresh calls through stub methods.',
        ('tests/js/reload.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/reload.js', 'windowed'): 'Records requested backend windows.',
        ('tests/js/runfile.js', 'acted'): 'Records act calls through stub methods.',
        ('tests/js/runfile.js', 'qlOpened'): 'Records Quick Look opens through the stub preview.',
        ('tests/js/runfile.js', 'ranText'): 'Records Open as text calls through the stub run lane.',
        ('tests/js/runfile.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/search.js', 'relisted'): 'Records openWithoutHistory inputs.',
        ('tests/js/search.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/searchnoblank.js', 'sent'): 'Records the listing paths the stub backend is asked for.',
        ('tests/js/slowclick.js', 'cancelled'): 'Records cancel calls through stub methods.',
        ('tests/js/slowclick.js', 'did'): 'Records dispatch calls through stub methods.',
        ('tests/js/slowclick.js', 'picked'): 'Stores the stub selection behind selection methods.',
        ('tests/js/sort.js', 'cleared'): 'Records clearSelection calls through stub methods.',
        ('tests/js/sort.js', 'committed'): 'Records commitRename calls through stub methods.',
        ('tests/js/sort.js', 'cursor'): 'Records setCursor inputs, separate from runtime cursorIndex.',
        ('tests/js/sort.js', 'remembered'): 'Records backend.rememberSort requests.',
        ('tests/js/sort.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/sort.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/status.js', 'picked'): 'Stores fixture selection behind selectedIndices.',
        ('tests/js/status.js', 'said'): 'Records stub message emissions.',
        ('tests/js/status.js', 'stampAtSay'): 'Records the trash stamp at the time the message signal fires.',
        ('tests/js/status.js', 'trashedIdx'): 'Records indices passed to backend.trash.',
        ('tests/js/swap.js', 'acted'): 'Records act calls through stub methods.',
        ('tests/js/swap.js', 'asked'): 'Records request calls through stub methods.',
        ('tests/js/swap.js', 'cleared'): 'Records clearSelection calls through stub methods.',
        ('tests/js/swap.js', 'dropped'): 'Records swap.drop calls.',
        ('tests/js/swap.js', 'droppedOut'): 'Records listInFlight at swap.drop time.',
        ('tests/js/swap.js', 'holds'): 'Records swap.hold requests.',
        ('tests/js/swap.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/swap.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/tabrestore.js', 'land'): 'Records the callback used by pending tab restoration.',
        ('tests/js/tabs-switch.js', 'listed'): 'Records list or open requests.',
        ('tests/js/tabs-switch.js', 'preferenceHidden'): 'Simulates external ViewState.hidden in the stub listing.',
        ('tests/js/tabs-switch.js', 'sorted'): 'Records backend.sort calls.',
        ('tests/js/tabs-switch.js', 'superseded'): 'Resets the flag the stub openWithoutHistory records for supersede.',
        ('tests/js/tabs.js', 'superseded'): 'Resets the flag the stub openWithoutHistory records for supersede.',
        ('tests/js/tabsfixture.js', 'clearedAtOnce'): 'Records openWithoutHistory clearAtOnce options.',
        ('tests/js/tabsfixture.js', 'listed'): 'Records list or open requests.',
        ('tests/js/tabsfixture.js', 'preferenceHidden'): 'Simulates the external ViewState.hidden preference during a stub listing.',
        ('tests/js/tabsfixture.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/tabsfixture.js', 'sorted'): 'Records backend.sort inputs.',
        ('tests/js/tabsfixture.js', 'superseded'): 'Records whether the last openWithoutHistory passed supersede.',
        ('tests/js/tabsfixture.js', 'windows'): 'Records backend.window inputs.',
        ('tests/js/tap.js', 'cancelled'): 'Records cancel calls through stub methods.',
        ('tests/js/tap.js', 'contexts'): 'Records showRow or pointer dispatch context arguments.',
        ('tests/js/tap.js', 'cursor'): 'Records setCursor inputs, separate from runtime cursorIndex.',
        ('tests/js/tap.js', 'did'): 'Records dispatch calls through stub methods.',
        ('tests/js/tap.js', 'picked'): 'Stores the stub selection behind selection methods.',
        ('tests/js/trash.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/trash.js', 'trashedIdx'): 'Records indices passed to backend.trash.',
        ('tests/js/watch.js', 'cleared'): 'Records clearSelection calls through stub methods.',
        ('tests/js/watch.js', 'contexts'): 'Records showRow or pointer dispatch context arguments.',
        ('tests/js/watch.js', 'cursorSetTo'): 'Records setCursor calls.',
        ('tests/js/watch.js', 'marked'): 'Records selection.clear and selection.toggle calls during preference re-anchoring.',
        ('tests/js/watch.js', 'said'): 'Records messages emitted by the stub message signal.',
        ('tests/js/watch.js', 'selectedAt'): 'Records indices requested through selectOnly.',
        ('tests/js/watch.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/watch.js', 'want'): 'Supplies a fixture-only selection sequence read by the stub selectedIndices method.',
        ('tests/js/xwrl4.js', 'cursorSetTo'): 'Records setCursor and selectOnly calls across the watched re-read.',
        ('tests/js/xwrl4.js', 'sent'): 'Records backend requests emitted by stub methods.',
        ('tests/js/xwwatch.js', 'cursorCtx'): 'Records the context argument passed to setCursor.',
        ('tests/js/xwwatch.js', 'cursorSetTo'): 'Records setCursor and selectOnly calls across the watched re-read.',
        ('tests/js/xwwatch.js', 'sent'): 'Records backend requests emitted by stub methods.',
    }


def del_printable(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if path.suffix not in ('.qml', '.js') or not file.startswith('ui/') or not path.is_file():
            continue
        scanned += 1
        source = path.read_text()
        code = masked(source, path.suffix)
        aliases = {'event.text'}
        for _ in range(ALIAS_FIXPOINT_CAP):
            before = set(aliases)
            for m in re.finditer(r'\b(?:var|let|const)\s+(\w+)\s*=\s*([\w.]+)\b', code):
                if m[2] in aliases:
                    aliases.add(m[1])
            if aliases == before:
                break
        for alias in aliases:
            pattern = r'\b' + re.escape(alias) + r'(?:\.length\s*(?:===?|!==?|>=|<=|>|<)|\.(?:charCodeAt|codePointAt)\s*\(|\s*(?:>=|<=|>|<)(?!=))'
            for m in re.finditer(pattern, code):
                errors.append(f'{location(file, code, m.start())}: printable event.text decision bypasses Input.isPrintable')
        if 'Input.isPrintable' in code:
            imports = re.findall(r'import\s+"([^"\n]+)"\s+as\s+Input\b', source)
            if not any((path.parent / name).resolve() == root / 'ui/js/Input.js' for name in imports):
                errors.append(f'{file}: Input.isPrintable does not import the shared Input.js')
        for m in re.finditer(r'\b(?:[\w.]*\.)?isPrintable\s*\(\s*event\.text', code):
            if not m[0].startswith('Input.isPrintable'):
                errors.append(f'{location(file, code, m.start())}: printable decision uses a second helper')
    return scanned, sorted(set(errors))


def qmllint_command(root, binary, out, files, missing):
    # Sample input: qmllint --json out.json -I tests/staticgates-stubs/qs.Commons ui/A.qml
    command = [binary, '--json', str(out)]
    for module in (QMLLINT_ALWAYS_STUB, *sorted(missing)):
        stubs = (root / QMLLINT_STUBS / module).resolve()
        if stubs.is_dir():
            command += ['-I', str(stubs)]
    return command + files


def qmllint_missing_modules(root, binary, modules=QMLLINT_PROBED_MODULES):
    # A probe without any -I names the modules the real Qt and Quickshell installs lack, and a probe that cannot answer raises.
    with tempfile.TemporaryDirectory(prefix='staticgates-qsprobe-') as scratch:
        probe = Path(scratch) / 'Probe.qml'
        probe.write_text(''.join(f'import {module}\n' for module in modules) + 'import QtQuick\nItem {}\n')
        out = Path(scratch) / 'diagnostics.json'
        try:
            result = subprocess.run([binary, '--json', str(out), str(probe)], cwd=root,
                                    capture_output=True, text=True, timeout=QMLLINT_TIMEOUT_SECONDS)
            if not out.exists():
                raise ValueError(f'returned {result.returncode} without JSON: {result.stderr[:STDERR_EXCERPT_LENGTH]}')
            # Sample input: {"files":[{"filename":"Probe.qml","warnings":[{"id":"import","message":"Failed to import Quickshell. Are your import paths set up properly?"}]}]}
            files = json.loads(out.read_text())['files']
            return {module for module in modules for file in files for warning in file.get('warnings', [])
                    if warning['id'] == 'import' and warning['message'].startswith(f'Failed to import {module}.')}
        except subprocess.TimeoutExpired:
            reason = f'timed out after {QMLLINT_TIMEOUT_SECONDS} s'
        except json.JSONDecodeError as error:
            reason = f'invalid JSON: {error}'
        except KeyError as error:
            reason = f'JSON lacks {error}'
        except (TypeError, AttributeError) as error:
            reason = f'JSON of the wrong shape: {error}'
        except (OSError, ValueError) as error:
            reason = str(error)
    raise ValueError(f'qmllint import probe failed: {reason}')


def qml_undeclared_read(root, files, sample=False):
    qml = [f for f in files if f.startswith('ui/') and f.endswith('.qml') and (root / f).is_file()]
    if not qml:
        return 0, []
    binary = os.environ.get('FLEA_QMLLINT', '/usr/lib/qt6/bin/qmllint')
    if not Path(binary).is_file() or not os.access(binary, os.X_OK):
        return len(qml), [f'qmllint unavailable: {binary}']
    try:
        missing = qmllint_missing_modules(root, binary)
    except ValueError as error:
        return len(qml), [str(error)]
    print('STATICGATE qml-undeclared-read PROBE missing=' + (','.join(sorted(missing)) or 'none'))
    with tempfile.TemporaryDirectory(prefix='staticgates-qmllint-') as scratch:
        out = Path(scratch) / 'diagnostics.json'
        result = subprocess.run(qmllint_command(root, binary, out, qml, missing), cwd=root, capture_output=True, text=True, timeout=QMLLINT_TIMEOUT_SECONDS)
        if not out.exists():
            return len(qml), [f'qmllint returned {result.returncode} without JSON: {result.stderr[:STDERR_EXCERPT_LENGTH]}']
        try:
            # Sample input: {"files":[{"filename":"ui/A.qml","warnings":[{"id":"unqualified","line":1,"column":1,"length":5}]}]}
            data = json.loads(out.read_text())
            reported = {str((root / f['filename']).resolve().relative_to(root)) for f in data['files']}
        except (ValueError, KeyError) as error:
            return len(qml), [f'qmllint returned invalid JSON: {error}']
        if reported != set(qml):
            return len(qml), ['qmllint did not report every requested file']
        current = collections.Counter()
        for file in data['files']:
            path = (root / file['filename']).resolve()
            lines = path.read_text().splitlines()
            for warning in file.get('warnings', []):
                if warning['id'] == 'unqualified':
                    n, column, size = warning['line'], warning['column'], warning['length']
                    name = lines[n - 1][column - 1:column - 1 + size]
                    current[(str(path.relative_to(root)), n, name)] += 1
        baseline = collections.Counter()
        if not sample:
            profile = 'quickshell-missing' if 'Quickshell' in missing else 'quickshell-present'
            active = False
            found = False
            for row, line in enumerate((root / 'tests/staticgates-unqualified.tsv').read_text().splitlines(), 1):
                if line.startswith('# profile '):
                    active = line == '# profile ' + profile
                    found |= active
                    continue
                if not line or line.startswith('#') or not active:
                    continue
                try:
                    # Sample input: ui/A.qml\t1\tasker\tDeclared QML id.
                    file, n, name, reason = line.split('\t', 3)
                    n = int(n)
                    if not reason.strip():
                        raise ValueError('unqualified baseline entry lacks a reason')
                except ValueError as error:
                    return len(qml), [f'tests/staticgates-unqualified.tsv:{row}: {error}']
                if (root / file).is_file():
                    baseline[(file, n, name)] += 1
            if not found:
                return len(qml), [f'no unqualified baseline for import profile {profile}']
        errors = [f'{f}:{n}: new unqualified read {name}' for f, n, name in (current - baseline).elements()]
        errors += [f'{f}:{n}: stale unqualified allowance {name}' for f, n, name in (baseline - current).elements()]
        return len(qml), errors



def qmlcachegen_binary():
    override = os.environ.get('FLEA_QMLCACHEGEN')
    # An explicit override is used or refused, never skipped for a system binary.
    if override:
        if Path(override).is_file() and os.access(override, os.X_OK):
            return override
        raise ValueError('FLEA_QMLCACHEGEN is not an executable file: ' + override)
    for binary in QMLCACHEGEN_PATHS:
        if Path(binary).is_file() and os.access(binary, os.X_OK):
            return binary
    raise ValueError('qmlcachegen unavailable; tried: ' + ', '.join(QMLCACHEGEN_PATHS))


# Sample input: Item { property int wire: 0; property int wire: 1 }
def compile_qml(root, files):
    if not files:
        return []
    binary = qmlcachegen_binary()
    with tempfile.TemporaryDirectory(prefix='staticgates-qmlcachegen-') as scratch:
        def compile_file(file):
            out = Path(scratch) / (file + 'c')
            out.parent.mkdir(parents=True, exist_ok=True)
            result = subprocess.run([binary, '--only-bytecode', '--resource-path', '/' + file,
                                     file, '-o', str(out)], cwd=root, capture_output=True,
                                    text=True, timeout=QMLCACHEGEN_TIMEOUT_SECONDS)
            if result.returncode:
                diagnostic = (result.stderr or result.stdout).strip().removeprefix('Error compiling qml file: ')
                # Sample input: A.qml:4:18: error: Duplicate property name\nA.qml:6:18: error: Duplicate property name
                lines = [line for line in diagnostic.splitlines() if line.strip()]
                return lines or [f'{file}: qmlcachegen exited {result.returncode} without diagnostics']
            if not out.is_file() or not out.stat().st_size:
                return [f'{file}: qmlcachegen returned success without bytecode']
            return []

        with ThreadPoolExecutor(max_workers=QMLCACHEGEN_POOL_SIZE) as pool:
            return [error for errors in pool.map(compile_file, files) for error in errors]


def qml_duplicate_member(root, files):
    qml = [file for file in files if file.startswith(('ui/', 'tests/'))
           and file.endswith('.qml') and (root / file).is_file()]
    js = [file for file in files if file.startswith('ui/')
          and file.endswith('.js') and (root / file).is_file()]
    errors = compile_qml(root, qml)
    for file in js:
        members = {}
        for line, source in enumerate((root / file).read_text().splitlines(), 1):
            # This check relies on the column-0 house style for top-level JS declarations.
            # Sample input: function wire() {} or var wire = null, starting at column 0.
            match = re.match(r'(?:function[ \t]+([A-Za-z_$][\w$]*)[ \t]*\(|(?:var|const|let)[ \t]+([A-Za-z_$][\w$]*)(?![\w$]))', source)
            if not match:
                continue
            name = match[1] or match[2]
            if name in members:
                errors.append(f'{file}:{line}: {name} declared again (first at {members[name]})')
            else:
                members[name] = line
    return len(qml) + len(js), errors


# Sample input: ui/Sample.qml holding "WorkerScript { source: \"W.js\" }" in code is rejected; only ui/CountedWorker.qml may hold one, and it must announce in code.
def worker_announce(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if not file.startswith('ui/') or path.suffix != '.qml' or not path.is_file():
            continue
        scanned += 1
        text = path.read_text()
        code = masked(text, '.qml')
        if file == COUNTED_WORKER:
            # The call sits in code, so the literal at its argument offset is the real one, never a comment's copy.
            calls = re.finditer(r'\bconsole\.info\(\s*startLog\s*,\s*', code)
            if not any(text.startswith('"WORKER_STARTED ', call.end()) for call in calls):
                errors.append(f'{file}:1: no console.info(startLog, "WORKER_STARTED " ...) call in code')
            continue
        for m in re.finditer(r'\bWorkerScript\s*\{', code):
            errors.append(f'{location(file, code, m.start())}: WorkerScript outside {COUNTED_WORKER}')
    return scanned, errors


# Sample input: "import qs.Commons as Commons", "import qs.Commons 1.0 as Commons", ".import qs.Commons 1.0 as Commons".
COMMONS_IMPORT = {suffix: r'^[ \t]*' + dot + r'import[ \t]+qs\.Commons(?:[ \t]+\d+(?:\.\d+)?)?[ \t]+as[ \t]+Commons[ \t]*;?[ \t]*$'
                  for suffix, dot in (('.qml', ''), ('.js', r'\.'))}


# Qt 6.12's QtQuick exports a Color singleton that wins over an unqualified qs.Commons one, so QML and JS read Omarchy's as Commons.Color.
# Sample input: "color: Color.accent", "Color?.accent" and "Color['accent']" are rejected; "Commons.Color.accent", "Theme.color", "myColor" and a comment pass.
def commons_color(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if path.suffix not in ('.qml', '.js') or not path.is_file():
            continue
        scanned += 1
        code = masked(path.read_text(), path.suffix)
        for m in re.finditer(r'(?<![\w.$])Color(?![\w$])', code):
            errors.append(f'{location(file, code, m.start())}: bare Color, which Qt 6.12 resolves to QtQuick\'s own; read Commons.Color')
        # Every file binds Commons itself, a script by .import, so no read depends on which QML imported the script.
        read = re.search(r'(?<![\w.$])Commons\s*\.', code)
        if read and not re.search(COMMONS_IMPORT[path.suffix], code, re.M):
            errors.append(f'{location(file, code, read.start())}: reads Commons without import qs.Commons as Commons')
    return scanned, errors


# Every transition in ui/ runs on OutCubic; Linear stays for the fixed-rate publish smoothing.
# Sample input: "easing.type: Easing.OutQuad" and "Easing.InQuad" are rejected; "Easing.OutCubic", "Easing.Linear" and a comment pass.
def easing(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if not file.startswith('ui/') or path.suffix not in ('.qml', '.js') or not path.is_file():
            continue
        scanned += 1
        code = masked(path.read_text(), path.suffix)
        for m in re.finditer(r'(?<![\w.$])Easing\s*\.\s*(\w+)', code):
            if m[1] not in ('OutCubic', 'Linear'):
                errors.append(f'{location(file, code, m.start())}: Easing.{m[1]}, every transition uses Easing.OutCubic')
    return scanned, errors


# Sample input: x = "a//b" // 'c...' keeps the literals and drops the tail.
def ellipsis_code(line, quote=None):
    # A // tail is a comment only outside a string literal, so a URI keeps its slashes.
    if quote != '`':
        quote = None
    out = ['`'] if quote == '`' else []
    i = 0
    while i < len(line):
        char = line[i]
        if quote is not None:
            out.append(char)
            if char == '\\':
                out.append(line[i + 1:i + 2])
                i += 2
                continue
            if char == quote:
                quote = None
        elif char in '"\'`':
            quote = char
            out.append(char)
        elif char == '/' and line[i + 1:i + 2] == '/':
            break
        else:
            out.append(char)
        i += 1
    # A template still open at the line end closes here so the pattern sees its tail.
    return ''.join(out) + ('`' if quote == '`' else ''), quote


# Sample input: /* "Saving..." */ keeps code; "//" keeps tails and "'" keeps quotes.
def ellipsis_blocks(text):
    # Blank block comments keeping newlines; // and quotes end at newline, backticks span.
    out = []
    quote = None
    blocked = False
    lined = False
    i = 0
    while i < len(text):
        char = text[i]
        pair = text[i:i + 2]
        if quote is not None:
            out.append(char)
            if char == '\\':
                nxt = text[i + 1:i + 2]
                out.append(nxt)
                if nxt == '\n' and quote != '`':
                    quote = None
                i += 2
                continue
            if (char == '\n' and quote != '`') or char == quote:
                quote = None
        elif blocked:
            if pair == '*/':
                blocked = False
                out.append('  ')
                i += 2
                continue
            out.append('\n' if char == '\n' else ' ')
        elif lined:
            out.append(char)
            if char == '\n':
                lined = False
        elif pair == '//':
            lined = True
            out.append(pair)
            i += 2
            continue
        elif pair == '/*':
            blocked = True
            out.append('  ')
            i += 2
            continue
        elif char in '"\'`':
            quote = char
            out.append(char)
        else:
            out.append(char)
        i += 1
    return ''.join(out)


# Sample input: label: 'Saving...' is rejected; "Saving…" and a /* "Saving..." */ block pass.
def ellipsis(root, files):
    errors = []
    scanned = 0
    for file in files:
        path = root / file
        if path.suffix not in ('.qml', '.js') or not file.startswith('ui/') or not path.is_file():
            continue
        scanned += 1
        quote = None
        for n, line in enumerate(ellipsis_blocks(path.read_text()).splitlines(), 1):
            code, quote = ellipsis_code(line, quote)
            for m in re.finditer(r'["\'`][^"\'`\n]*[A-Za-z]\.\.\.[^"\'`\n]*["\'`]', code):
                errors.append(f'{file}:{n}: three full stops after a letter, use the ellipsis character')
    return scanned, sorted(set(errors))


# Sample input: extern "C" { fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int; } is rejected; the same declaration with a ... tail passes.
def extern_variadic(root, files):
    errors = []
    scanned = 0
    declarations = re.compile(r'\bfn\s+(fcntl|prctl|ioctl|open|openat|syscall|execl|execle|execlp|printf|fprintf|sprintf|snprintf|dprintf)\b\s*\(([^)]*)\)')
    for file in files:
        path = root / file
        if path.suffix != '.rs' or not path.is_file() or path.is_symlink():
            continue
        scanned += 1
        code = masked(path.read_text(), '.rs')
        # The "C" literal masks to tildes, so an extern block opens as extern ~~~ {.
        for block in re.finditer(r'\bextern\s*~+\s*\{', code):
            end = closing(code, block.end() - 1, '{', '}')
            for m in declarations.finditer(code, block.end(), end):
                if '...' in m[2]:
                    continue
                errors.append(f'{location(file, code, m.start())}: fn {m[1]} declared without its C variadic ... tail')
    return scanned, errors


CHECKS = dict(zip(GATES, (conflict_marker, fused_line, paneprops, del_printable, qml_undeclared_read, qml_duplicate_member, worker_announce,
                          commons_color, extern_variadic, pane_function_value, dead_module_call, worker_report, easing, ellipsis)))


def negative_controls(root):
    samples = {
        'conflict-marker': ('sample.txt', '<<<<<<< left\n'),
        'fused-line': ('sample.js', 'function broken() {    run() }\n'),
        'paneprops': ('ui/js/Sample.js', 'function broken(pane) { pane.removedProperty = true }\n'),
        'del-printable': ('ui/js/Sample.js', 'function key(event) { return event.text.length === 1 && event.text >= " " }\n'),
        'qml-undeclared-read': ('ui/Sample.qml', 'import QtQuick\nItem { function readRecent() { return asker } }\n'),
        'qml-duplicate-member': ('ui/Pane.qml', 'import QtQuick\nFocusScope {\n'
                                 '    readonly property alias wire: wire\n'
                                 '    readonly property alias wire: wire\n}\n'),
        'worker-announce': ('ui/Sample.qml', 'import QtQuick\nItem { WorkerScript { source: "W.js" } }\n'),
        'commons-color': ('shelf/Sample.qml', 'import QtQuick\nimport qs.Commons\nRectangle { color: Color.accent }\n'),
        'extern-variadic': ('src/Sample.rs', 'extern "C" {\n    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;\n}\n'),
        'pane-function-value': ('ui/js/Sample.js', 'function f(pane, id) { return id !== pane.identityOf }\n'),
        'dead-module-call': ('ui/Sample.qml', 'import QtQuick\nimport "js/Search.js" as Search\nItem { function f() { Search.run(this) } }\n'),
        'worker-report': ('RESULT.md', 'run notes\n'),
        'easing': ('ui/Sample.qml', 'import QtQuick\nItem { NumberAnimation { easing.type: Easing.OutQuad } }\n'),
        'ellipsis': ('ui/Sample.qml', 'Item { property string busy: "Saving..." }\n'),
    }
    errors = []
    script = root / 'tests/staticgates.py'
    with tempfile.TemporaryDirectory(prefix='staticgates-controls-') as scratch:
        # Each worker-report branch and root name needs its own planted file and sample dir.
        cases = list(samples.items()) + [('qml-duplicate-member', ('ui/js/Sample.js',
                                         '.pragma library\nfunction wire() {}\nfunction wire() {}\n')),
                                       ('extern-variadic', ('src/Other.rs',
                                         'extern "C" {\n    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;\n}\n')),
                                       ('worker-report', ('.superpowers/hyg1/NOTES.md', 'run notes\n')),
                                       ('worker-report', ('REPORT-1.md', 'run notes\n')),
                                       ('worker-report', ('STUDY-1.md', 'run notes\n')),
                                       ('worker-report', ('RUN.md', 'run notes\n')),
                                       ('worker-report', ('PREMISE-1.txt', 'run notes\n'))]
        for gate, (file, text) in cases:
            sample = Path(scratch) / gate / Path(file).stem
            path = sample / file
            path.parent.mkdir(parents=True)
            path.write_text(text)
            if gate == 'paneprops':
                (sample / 'ui/Pane.qml').write_text('import QtQuick\nFocusScope { property int cursorIndex: 0 }\n')
            if gate == 'dead-module-call':
                (sample / 'ui/js').mkdir()
                (sample / 'ui/js/Search.js').write_text('.pragma library\nfunction start(root) {}\n')
            if gate == 'pane-function-value':
                (sample / 'ui/Pane.qml').write_text('import QtQuick\nFocusScope { function identityOf() { return "" }\n}\n')
            result = subprocess.run([sys.executable, str(script), '--gate', gate, '--sample', str(sample)], capture_output=True, text=True)
            diagnostic = next((line for line in result.stdout.splitlines() if line.startswith(f'STATICGATE {gate} FAIL ')), '')
            expected = {'conflict-marker': 'unresolved conflict marker', 'fused-line': 'fused code gap',
                        'paneprops': 'pane.removedProperty absent', 'del-printable': 'printable event.text decision',
                        'qml-undeclared-read': 'new unqualified read asker',
                        'worker-announce': 'WorkerScript outside ui/CountedWorker.qml',
                        'commons-color': 'bare Color',
                        'extern-variadic': 'declared without its C variadic',
                        'pane-function-value': 'compared without its call',
                        'dead-module-call': 'defines no function run',
                        'worker-report': 'worker report tracked at',
                        'easing': 'every transition uses Easing.OutCubic',
                        'ellipsis': 'three full stops',
                        'qml-duplicate-member': ('wire declared again (first at 2)' if file.endswith('.js') else 'Duplicate alias name')}[gate]
            if (result.returncode != 1 or expected not in diagnostic or file + ':' not in diagnostic
                    or 'STATICGATES FAIL gates=1' not in result.stdout):
                errors.append(f'{gate}: planted defect was not rejected: {result.stdout} {result.stderr}')
            else:
                # The planted defect's verdict word is dropped, so a passing run prints no FAIL token for CI.
                print(f'STATICGATE {gate} RED exit=1 rejected={diagnostic.split(" FAIL ", 1)[-1]}')
            if gate == 'pane-function-value' and not errors[-1:]:
                positive = Path(scratch) / (gate + '-positive')
                (positive / 'ui/js').mkdir(parents=True)
                (positive / 'ui/js/Sample.js').write_text('function f(pane, id) { return id !== pane.identityOf() }\n')
                (positive / 'ui/Pane.qml').write_text('import QtQuick\nFocusScope { function identityOf() { return "" }\n}\n')
                called = subprocess.run([sys.executable, str(script), '--gate', gate, '--sample', str(positive)], capture_output=True, text=True)
                if called.returncode != 0 or 'compared without its call' in called.stdout:
                    errors.append(f'{gate}: called function was not accepted: {called.stdout} {called.stderr}')
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--gate', choices=GATES)
    parser.add_argument('--root', type=Path, default=Path.cwd())
    parser.add_argument('--sample', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    root = (args.sample or args.root).resolve()
    try:
        files = sorted(str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()) if args.sample else inventory(root)
    except (OSError, ValueError, tarfile.TarError) as error:
        print(f'STATICGATE inventory FAIL {error}')
        print(f'STATICGATES FAIL gates={1 if args.gate else len(GATES)}')
        return 1
    controls = [] if args.gate or args.sample else negative_controls(root)
    for error in controls:
        print(f'STATICGATE negative-control FAIL {error}')
    failed = len(controls)
    for name in ((args.gate,) if args.gate else GATES):
        selected = inventory(root, tracked=True) if name in ('conflict-marker', 'qml-duplicate-member') and not args.sample else files
        try:
            kwargs = {'sample': True} if args.sample and name in ('paneprops', 'qml-undeclared-read') else {}
            count, errors = CHECKS[name](root, selected, **kwargs)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            count, errors = 0, [str(error)]
        for error in errors:
            print(f'STATICGATE {name} FAIL {error}')
        if not errors:
            print(f'STATICGATE {name} PASS files={count}')
        failed += bool(errors)
    print(f'STATICGATES {"FAIL" if failed else "PASS"} gates={1 if args.gate else len(GATES)}')
    return int(bool(failed))


if __name__ == '__main__':
    raise SystemExit(main())
