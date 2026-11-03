"""Independent lexical audit of failure sites in a test tree; never execute inputs."""

import collections
import csv
import re
from pathlib import Path


FORMS = ('throw', 'fail', 'finish-false', 'FAIL-output', 'nonzero-exit',
         'assertion', 'failure-collection', 'wrapper')
ASSERTIONS = {'check', 'expect', 'verify', 'compare', 'fuzzyCompare', 'tryCompare', 'tryVerify', 'assert'}


def lex(text):
    result, offset = [], 0
    while offset < len(text):
        start, char = offset, text[offset]
        if char.isspace():
            offset += 1
            continue
        if text.startswith('//', offset):
            end = text.find('\n', offset)
            offset = len(text) if end < 0 else end
            continue
        if text.startswith('/*', offset):
            end = text.find('*/', offset + 2)
            assert end >= 0, 'unclosed comment'
            offset = end + 2
            continue
        if char in ('"', "'", '`'):
            offset += 1
            while offset < len(text):
                if text[offset] == '\\':
                    offset += 2
                elif text[offset] == char:
                    offset += 1
                    break
                else:
                    offset += 1
        elif char == '/' and (not result or result[-1][0] in
                              ('=', '(', '[', '{', ',', ':', ';', '!', '?', 'return', 'throw', '&', '|')):
            offset, bracket = offset + 1, False
            while offset < len(text) and text[offset] != '\n':
                if text[offset] == '\\':
                    offset += 2
                    continue
                if text[offset] == '[':
                    bracket = True
                elif text[offset] == ']':
                    bracket = False
                elif text[offset] == '/' and not bracket:
                    offset += 1
                    while offset < len(text) and text[offset].isalpha():
                        offset += 1
                    break
                offset += 1
            else:
                offset = start + 1
        elif char.isalnum() or char in ('_', '$'):
            offset += 1
            while offset < len(text) and (text[offset].isalnum() or text[offset] in ('_', '$')):
                offset += 1
        else:
            offset += 1
        result.append((text[start:offset], start))
    return result


def scan(text, excluded=None):
    tokens = lex(text)
    words = [token[0] for token in tokens]
    pairs, stack = {}, []
    for index, word in enumerate(words):
        if word in ('(', '[', '{'):
            stack.append(index)
        elif word in (')', ']', '}'):
            assert stack and words[stack[-1]] == {')': '(', ']': '[', '}': '{'}[word], (word, tokens[index][1])
            pairs[stack.pop()] = index
    assert not stack, 'unclosed delimiter'
    definitions, declarations = [], set()
    for index, word in enumerate(words[:-2]):
        names, body = [], None
        if word == 'function':
            args = index + 1
            if words[args] != '(':
                names.append(words[args])
                declarations.add(args)
                args += 1
            if index > 1 and words[index - 1] in ('=', ':'):
                names.append(words[index - 2])
            body = pairs.get(args, -2) + 1
        elif (word not in ('if', 'for', 'while', 'switch', 'catch', 'with') and index and
              words[index - 1] in ('{', ',', ';', '}') and words[index + 1] == '('):
            body = pairs.get(index + 1, -2) + 1
            if body in pairs:
                names.append(word)
                declarations.add(index)
        elif word == '=' and words[index + 1] == '>':
            args = next((start for start, end in pairs.items() if end == index - 1), index - 1)
            if args > 1 and words[args - 1] in ('=', ':'):
                names.append(words[args - 2])
                body = index + 2
        if not names or body is None or body >= len(words):
            continue
        if words[body] == '{':
            start, end = body, pairs[body]
        elif word == '=':
            start, end = body - 1, body
            while end < len(words) and words[end] not in (',', ';', ')', ']', '}'):
                end = pairs.get(end, end) + 1
                if (end < len(words) and '\n' in text[tokens[end - 1][1] + len(words[end - 1]):tokens[end][1]] and
                        words[end - 1] not in '.?:+-*/%&|^=!<>' and words[end] not in '.?:+-*/%&|^=!<>'):
                    break
        else:
            continue
        definitions.extend((name, start, end) for name in names)
    sites, calls = {}, {}
    for index, word in enumerate(words):
        previous = words[index - 1] if index else ''
        following = words[index + 1] if index + 1 < len(words) else ''
        end = pairs.get(index + 1)
        declaration = index in declarations or end is not None and end + 1 < len(words) and words[end + 1] == '{'
        forms = set()
        if word == 'throw' and previous not in ('.', 'function') and following != ':' and not declaration:
            forms.add('throw')
        if following == '(' and end is not None and not declaration and previous != 'function' and word not in (
                'if', 'for', 'while', 'switch', 'catch', 'with', 'function', 'throw'):
            calls[index] = word
            arguments = words[index + 2:end]
            if word == 'fail':
                forms.add('fail')
            elif word in ASSERTIONS:
                forms.add('assertion')
            first, cursor = [], index + 2
            while cursor < end and words[cursor] != ',':
                finish = pairs.get(cursor, cursor)
                first.extend(words[cursor:finish + 1])
                cursor = finish + 1
            literal = ''.join(first).strip('()')
            if word == 'finish' and literal == 'false':
                forms.add('finish-false')
            if any(arg.startswith(('"', "'", '`')) and re.search(r'\bFAIL\b', arg) for arg in arguments) or (
                    word == 'error' and index > 1 and words[index - 2] == 'console'):
                forms.add('FAIL-output')
            if word in ('exit', 'quit') and index > 1 and words[index - 2] == 'Qt':
                zero = not literal or literal in ('false', 'null')
                try:
                    zero |= float(literal) == 0
                except ValueError:
                    try:
                        zero |= int(literal, 0) == 0
                    except ValueError:
                        pass
                if not zero:
                    forms.add('nonzero-exit')
                elif excluded is not None:
                    excluded['successful-Qt-exit'] += 1
            if word == 'push' and index > 1 and words[index - 2] in ('failures', 'fails', 'errors', 'bad'):
                forms.add('failure-collection')
        if forms:
            sites[index] = forms
    edges, wrappers = collections.defaultdict(set), set()
    for index in sites.keys() | calls.keys():
        owners = [(name, end - start) for name, start, end in definitions if start < index < end]
        for name, size in owners:
            if size != min(length for _, length in owners):
                continue
            if index in sites:
                wrappers.add(name)
            if index in calls:
                edges[name].add(calls[index])
    while True:
        added = {name for name, callees in edges.items() if callees & wrappers} - wrappers
        if not added:
            break
        wrappers |= added
    for index, name in calls.items():
        if name in wrappers:
            sites.setdefault(index, set()).add('wrapper')
    return [(text.count('\n', 0, tokens[index][1]) + 1,
             tokens[index][1] - text.rfind('\n', 0, tokens[index][1]), forms)
            for index, forms in sorted(sites.items())]


def audit(tree, rows, report=None):
    tree = Path(tree)
    indexed = collections.defaultdict(list)
    for row in rows:
        if row['kind'] in ('qml-test', 'js-test') and row.get('column'):
            indexed[row['source'], int(row['column'])].append(row)
    paths = sorted((tree / 'tests').glob('*.qml')) + sorted((tree / 'tests').glob('*.js'))
    paths += sorted((tree / 'tests/js').glob('*.js'))
    totals, matched, excluded, missing = collections.Counter(), collections.Counter(), collections.Counter(), []
    details = []
    for path in paths:
        name = str(path.relative_to(tree))
        for line, column, forms in scan(path.read_text(), excluded):
            source = f'{name}:{line}'
            candidates = indexed[source, column]
            represented = len(candidates) == 1 and forms <= set(candidates[0]['sink'].split(','))
            for form in forms:
                totals[form] += 1
                matched[form] += represented
            if not represented:
                missing.append((source, column, sorted(forms), [row['sink'] for row in candidates]))
            details.append((name, line, column, ','.join(sorted(forms)), ','.join(row['id'] for row in candidates)))
    if report:
        with Path(report).open('w') as stream:
            writer = csv.writer(stream, delimiter='\t', lineterminator='\n')
            writer.writerow(('file', 'line', 'column', 'forms', 'rows'))
            writer.writerows(details)
    assert not missing, f'unrepresented failure sites ({len(missing)}): {missing[:20]}'
    return {form: (totals[form], matched[form]) for form in FORMS}, len(paths), dict(excluded)
