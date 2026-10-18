"""Shell nesting and scope checks for the parser-child locale inventory."""
import re

TOKENS = re.compile(r"\n|;;&|;;|;&|&&|\|\||[();|&<>]|[^\s();|&<>]+")
LOOPS = {"for", "while", "until", "select"}
BYTE_TOOLS = ('flea-bench-manifest', 'flea-matched-fixture', 'flea-showcase')
GIO_UTF8_OBSERVED = 'OBSERVED gio preserves UTF-8 display names: gio kept UTF-8 under LC_ALL=C'


# Sample input: case "$1" in a|b) : ;; esac followed by f() { export LC_ALL=C.UTF-8; }.
def top_level(masked, *, case_patterns=True, operator_boundaries=True, strict=True):
    top = list(masked)
    tokens = list(TOKENS.finditer(masked))
    stack = []
    command = True
    function = False
    cursor = 0

    def hide(start, end):
        for index in range(start, end):
            if top[index] != '\n':
                top[index] = 'Q'

    def fail(message, offset):
        if strict:
            raise ValueError(f'{message} at line {masked.count(chr(10), 0, offset) + 1}')

    def close(kinds, word, offset):
        if stack and stack[-1]['kind'] in kinds:
            stack.pop()
        else:
            fail('unmatched ' + word, offset)

    for number, token in enumerate(tokens):
        word = token[0]
        start, end = token.span()
        if stack:
            hide(cursor, end)
        cursor = end
        frame = stack[-1] if stack else None
        if frame and frame['kind'] == '[[':
            if word == ']]':
                stack.pop()
                command = False
            continue
        if frame and frame['kind'] == 'arithmetic':
            if word == '(':
                frame['depth'] += 1
            elif word == ')':
                frame['depth'] -= 1
                if frame['depth'] == 0:
                    stack.pop()
                    command = False
            continue
        if frame and frame['kind'] == 'case':
            if frame['phase'] in {'word', 'in'}:
                if frame['phase'] == 'in' and word == 'in':
                    frame.update(phase='pattern', first=True, depth=0)
                    continue
                if word == '\n':
                    continue
                if word not in {'(', ')'}:
                    frame['phase'] = 'in'
                    continue
            elif frame['phase'] == 'pattern' and case_patterns:
                if word == 'esac' and frame['first']:
                    stack.pop()
                elif word == '(':
                    if not frame['first']:
                        frame['depth'] += 1
                    frame['first'] = False
                elif word == ')':
                    if frame['depth']:
                        frame['depth'] -= 1
                    else:
                        frame['phase'] = 'body'
                        command = True
                elif word != '\n':
                    frame['first'] = False
                continue
        if word in {';;', ';&', ';;&'}:
            if frame and frame['kind'] == 'case' and frame['phase'] == 'body':
                frame.update(phase='pattern', first=True, depth=0)
            else:
                fail('case terminator without an arm', start)
            command = True
        elif word == '(':
            following = tokens[number + 1] if number + 1 < len(tokens) else None
            kind = 'arithmetic' if following and following[0] == '(' and following.start() == end else '('
            stack.append({'kind': kind, 'depth': 1})
            command = operator_boundaries
        elif word == ')':
            close({'('}, word, start)
            command = True
        elif word == '{' and (command or function):
            stack.append({'kind': '{'})
            command = True
            function = False
        elif word == '}' and command:
            close({'{'}, word, start)
            command = False
        elif word == '[[' and command:
            stack.append({'kind': '[['})
        elif word in {'\n', ';'}:
            command = True
        elif word in {'|', '&&', '||', '&'}:
            command = operator_boundaries
        elif command and word in {'if', 'case'} | LOOPS:
            stack.append({'kind': word, 'phase': 'word'})
            command = word in {'if', 'while', 'until'}
        elif command and word in {'fi', 'done', 'esac'}:
            close({'fi': {'if'}, 'done': LOOPS, 'esac': {'case'}}[word], word, start)
            command = False
        elif command and word in {'then', 'elif', 'else', 'do'}:
            wanted = LOOPS if word == 'do' else {'if'}
            if not stack or stack[-1]['kind'] not in wanted:
                fail('unmatched ' + word, start)
            command = operator_boundaries
        elif command and word == 'function':
            function = True
            command = False
        elif not (command and word in {'!', 'time'}):
            command = False
        if stack:
            hide(start, end)
    if stack:
        fail('unclosed ' + stack[-1]['kind'], len(masked))
    return ''.join(top)


# Sample input: an actual tools/flea-matched-fixture line ending with # locale-bytes: count raw names.
def unmarked_rows(api, repo, target):
    path = repo / 'tools' / target
    source = path.read_text()
    mutated, count = re.subn(r'[ \t]+# locale-bytes:[^\n]*', '', source)
    if not count or mutated == source:
        raise ValueError(target + ' marker control changed no text')
    return [row for row in api['inventory'](repo, [(path, mutated)]) if not row[3] and row[2] != 'shell nesting']


# Sample input: case "$x" in a) ;; esac followed by top-level or function-local locale exports.
def self_checks(api, check):
    code_mask = api["code_mask"]
    commands = api["commands"]
    parsed = api["parsed"]
    exported = api["exported"]
    scoped = api["scoped"]
    scope_for = api["scope_for"]
    for source in ['x="${FOO:-$(grep -m1 bar file)}"',
                   'x=${FOO:-$(grep -m1 bar file)}',
                   'x="${a:-${b:-$(grep -m1 bar file)}}"',
                   'x="`grep -m1 bar file`"']:
        check('command substitution remains code: ' + source,
              sum(name == 'grep' for _, name, _ in commands(source)) == 1)
    check('nested parameter braces are masked together', '}' not in code_mask('x=${a:-${b}}'))
    check('quoted braces stay inside the parameter',
          '}' not in code_mask('x=${a:-"}"}'))
    check('quoted data is not executable', not any(name == 'grep' for _, name, _ in commands(
        "echo 'grep bar file'\n# grep bar file\ncat <<'DATA'\ngrep bar file\nDATA\n")))
    for name, suffix in [('magick', ' "$f" \\\n  -format "%k" info:'),
                         ('ffmpeg', ' -hide_banner \\\n  -encoders')]:
        check('continued ' + name + ' flags are parsed', parsed(name, suffix))
    check('next command flags do not classify magick', not parsed('magick', ' file.png\necho -format'))
    scope = 'export LC_ALL=C.UTF-8\nunset LANGUAGE\n'
    check('top-level scope before parser is accepted', exported(scope + 'grep bar file\n'))
    for label, source in [('late', 'grep bar file\n' + scope),
                          ('function', 'f() {\n' + scope + '}\ngrep bar file\n'),
                          ('subshell', '(\n' + scope + ')\ngrep bar file\n'),
                          ('conditional', 'if false; then\n' + scope + 'fi\ngrep bar file\n'),
                          ('heredoc', "cat <<'DATA'\n" + scope + 'DATA\ngrep bar file\n'),
                          ('comment', '# export LC_ALL=C.UTF-8\n# unset LANGUAGE\ngrep bar file\n')]:
        check(label + ' scope is rejected', not exported(source))
    marked = "LC_ALL=C LANGUAGE= sed 's/.*\\.//' # locale-bytes: count raw filename extensions"
    check('marked byte matcher is accepted', scoped('LC_ALL=C LANGUAGE= ', marked, 'sed'))
    check('byte matcher without marker is rejected', not scoped('LC_ALL=C LANGUAGE= ', 'sed x', 'sed'))
    check('quoted marker is not a comment', not scoped(
        'LC_ALL=C LANGUAGE= ', "sed '# locale-bytes: quoted data'", 'sed'))
    check('byte marker requires its reason', not scoped(
        'LC_ALL=C LANGUAGE= ', "sed x # locale-bytes: ", 'sed'))
    check('byte marker cannot authorize a text child', not scoped('LC_ALL=C LANGUAGE= ', marked, 'qs'))
    unmarked = scope + 'LC_ALL=C LANGUAGE= sed x\n'
    start, name, prefix = next(row for row in commands(unmarked) if row[1] == 'sed')
    check('global UTF-8 scope does not hide an unmarked byte override',
          not scope_for(unmarked, start, name, prefix))
    own_source = api["Path"](api["__file__"]).read_text()
    for name in ['code_mask', 'commands']:
        preceding = own_source[:own_source.index('def ' + name + '(')].rstrip().splitlines()[-1]
        check('sample input above ' + name, 'Sample input:' in preceding)
    scope_checks(api, check)
    repo = api['Path'](api['__file__']).resolve().parents[1]
    for target in BYTE_TOOLS:
        try:
            failures = unmarked_rows(api, repo, target)
        except ValueError as error:
            check(str(error), False)
        else:
            check(target + ' missing markers fail its own audit', bool(failures))


# Sample input: printf x | while read l; do :; done followed by the locale exports.
def scope_checks(api, check):
    scope = 'export LC_ALL=C.UTF-8\nunset LANGUAGE\n'
    grep = 'grep bar file\n'

    def expect(label, source, expected):
        try:
            actual = api['exported'](source)
        except ValueError:
            actual = None
        check(label, actual is expected)

    for pattern in ['a)', '(a)', 'a|b)', '@(a|b))']:
        case = 'case "$1" in ' + pattern + ' : ;; esac\n'
        expect('global scope after case ' + pattern, case + scope + grep, True)
        expect('function scope after case ' + pattern,
               case + 'f() {\n' + scope + '}\n' + grep, False)
        expect('subshell scope after case ' + pattern,
               case + '(\n' + scope + ')\n' + grep, False)
    for label, prefix in [('pipe loop', 'printf x | while read l; do :; done\n'),
                          ('parenthesized if', '(if true; then :; fi)\n'),
                          ('nested if', 'if true; then if false; then :; fi; fi\n'),
                          ('case fallthrough', 'case x in a) : ;& b) : ;;& c) : ;; esac\n')]:
        expect(label + ' preserves later global scope', prefix + scope + grep, True)
    for label, prefix in [('and if', 'true && if true; then\n'),
                          ('or if', 'false || if true; then\n'),
                          ('nested if', 'if a; then if b; then :; fi\n')]:
        expect(label + ' keeps exports inside its block', prefix + scope + 'fi\n' + grep, False)
    repo = api['Path'](api['__file__']).resolve().parents[1]
    for malformed in ['done\n', ')\n', '}\n', 'if true; then :; done\n',
                      'while true; do :\n', 'if true; then :\n']:
        source = scope + malformed
        try:
            api['export_scope'](source)
        except ValueError:
            rejected = True
        else:
            rejected = False
        check('unbalanced scope raises: ' + malformed.strip(), rejected)
        rows = list(api['inventory'](repo, [(repo / 'tools/nesting-control', source)]))
        check('unbalanced file fails audit: ' + malformed.strip(),
              len(rows) == 1 and rows[0][2] == 'shell nesting' and not rows[0][3])
    for separator in ['\r', '\f']:
        source = '# data' + separator + 'extra\nLC_ALL=C LANGUAGE= sed x # locale-bytes: count raw names\n'
        start, name, prefix = next(row for row in api['commands'](source) if row[1] == 'sed')
        try:
            scoped = api['scope_for'](source, start, name, prefix)
        except IndexError:
            scoped = False
        check('newline-only physical line with ' + repr(separator), scoped)
        rows = list(api['inventory'](repo, [(repo / 'tools/line-control', source)]))
        check('inventory line matches scope line with ' + repr(separator),
              len(rows) == 1 and rows[0][1] == 2 and rows[0][3])


# Sample input: --regress-uniq with FAIL matched fixture uniq child environment: C|unset.
def emit_detection(audit, args, errors, observations=()):
    signatures = {
        'shell': {
            'parameters': ('command substitution remains code:',),
            'continuations': ('continued magick flags are parsed', 'continued ffmpeg flags are parsed'),
            'exports': ('function scope is rejected', 'subshell scope is rejected'),
            'case-scope': ('global scope after case a)', 'function scope after case a)',
                           'subshell scope after case a)'),
            'block-boundaries': ('pipe loop preserves later global scope',
                                 'nested if keeps exports inside its block'),
            'block-balance': ('unbalanced scope raises:', 'unbalanced file fails audit:'),
            'physical-lines': ('newline-only physical line with', 'inventory line matches scope line with'),
        },
        'parsers': {
            'uniq': ('matched fixture uniq child environment',),
            'caller': ('process_identity preserves subsequent child locale',),
            'charset': ('minimum name length counts CJK characters', 'gio preserves UTF-8 display names',
                        'ps preserves UTF-8 process arguments'),
            'decimal': ('German sys_seconds and cpu_s use dot decimals',),
            'fixture-scope': ('matched fixture sort child environment', 'matched fixture uniq child environment'),
            'manifest-byte-mode': ('manifest counts a Latin-1 name as jpg',),
            'fixture-byte-mode': ('matched fixture counts a Latin-1 name as jpg',),
        },
    }
    # CI gio can preserve UTF-8 under C; only that check's own observation can replace its failure.
    for flag, labels in signatures[audit].items():
        if getattr(args, 'regress_' + flag.replace('-', '_'), False) and all(
                any(error.startswith('FAIL ' + label) for error in errors) or
                (label == 'gio preserves UTF-8 display names' and GIO_UTF8_OBSERVED in observations)
                for label in labels):
            print('DETECTED --regress-' + flag)
