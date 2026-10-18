#!/usr/bin/env bash
# Generate fixture and live inventories without executing any input scripts.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
mkdir -p "$repo/.superpowers/g06"
test_root=$(mktemp -d "$repo/.superpowers/g06/parity.XXXXXX")
: > "$test_root/.parity-test-root"
cleanup() {
    [[ -n "$test_root" && "$test_root" = /* && "$test_root" == "$repo/.superpowers/g06/parity."* && -f "$test_root/.parity-test-root" ]] && rm -rf -- "$test_root"
}
trap cleanup EXIT

python3 -B - "$repo" "$test_root" <<'PY'
import collections
import csv
import io
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys

repo, root = map(Path, sys.argv[1:])
assert root.is_absolute() and (root / '.parity-test-root').is_file()
tool = [sys.executable, '-B', str(repo / 'tools/flea-parity')]
fixture = repo / 'tests/fixtures/parity'
options = ['--tree', str(fixture), '--pending', str(fixture / 'PENDING.md'),
           '--design', str(fixture / 'design-hig'), '--layout', str(fixture / 'ci/default.layout')]
checks = 0


def check(condition, label):
    global checks
    assert condition, label
    checks += 1


def run(args, code=0):
    result = subprocess.run(tool + args, text=True, capture_output=True)
    check(result.returncode == code, f'exit {code}, got {result.returncode}: {result.stderr}')
    return result.stdout


def parse(text):
    return list(csv.DictReader(io.StringIO(text), delimiter='\t'))


output = run(options)
rows = parse(output)
counts = collections.Counter(row['kind'] for row in rows)
check(counts == {'qml': 3, 'js': 3, 'ipc': 3, 'binding': 9, 'schema': 7,
                 'qml-test': 79, 'js-test': 27, 'suite': 9, 'board': 2, 'scope': 7},
      f'exact artifact counts: {counts}')
check(len({row['id'] for row in rows}) == len(rows), 'IDs are unique')
check(output == run(options), 'inventory is deterministic')
check(all(row['status'] == 'open' for row in rows), 'status defaults to open')
by_id = {row['id']: row for row in rows}
check(by_id['qml:ui/Ipc.qml']['lines'] == str(len((fixture / 'ui/Ipc.qml').read_text().splitlines())),
      'QML line count comes from the input')
check(by_id['ipc:flea.ready']['source'] == 'ui/Ipc.qml:7', 'IPC declaration line is exact')
check(by_id['ipc:fleapicker.ready']['family'] == 'Picker', 'picker target has its own identity and owner')
check(not any('nested' in row['id'] or 'commented' in row['id'] for row in rows), 'IPC helpers are excluded')
binding = next(row for row in rows if row['kind'] == 'binding' and row['action'] == 'preview')
check((binding['key'], binding['context'], binding['preset']) == ('ctrl+P', 'listing,preview', 'default'),
      'binding carries chord, context and preset')
check(any(row['kind'] == 'binding' and row['action'] == '' and row['key'] == 'ctrl+X' for row in rows),
      'explicit inert key binding is retained')
check({'schema:preview', 'schema:preview.column', 'schema:preview.width'} <= by_id.keys(),
      'nested schema keys are inventoried')
check(by_id['schema:preview.width']['source'] == 'src/uischema.rs:3', 'same-line nested schema location')
check(not any('ignored' in row['id'] for row in rows), 'schema stops at DEFAULTS')
check(by_id['js:ui/js/Unmapped.js']['family'] == 'unassigned', 'unknown input stays unassigned')
for name, expected in [('backend', 'no'), ('preview', 'yes'), ('wrapper', 'yes'), ('quoted', 'yes'),
                       ('live', 'yes'), ('variable', 'yes'), ('constructed', 'yes'),
                       ('sibling', 'yes'), ('package', 'yes')]:
    check(by_id[f'suite:{name}']['qml-bound'] == expected, f'QML binding of {name}')
check('tests/preview.sh:2' in by_id['suite:wrapper']['details'], 'wrapper records transitive evidence')
check(all(row['owner'] == 'G0.10' for row in rows if row['kind'] == 'suite'), 'suite migration owner')
check('CHANGES.md:5' in by_id['board:ql-text']['details'] and
      'CHANGES.md:6' in by_id['board:ql-text']['details'], 'all board change lines are retained')
check(not any('Not in scope' in row['details'] for row in rows), 'scope boundaries exclude later sections')
check(sum(row['source'].startswith('tests/expect-cases.qml:') for row in rows) == 3,
      'g06-r1-1 and r5: both expect() sites and the primitive failure receipt are inventoried')
check(sum(row['source'].startswith('tests/preview-failures.qml:') for row in rows) == 5,
      'g06-r1-1 and r5: conditional, collected, logged and primitive failure receipts are inventoried')
check('tests/variable.sh:3' in by_id['suite:variable']['details'] and
      'tests/constructed.sh:4' in by_id['suite:constructed']['details'],
      'g06-r1-2: variable and constructed QML paths retain source evidence')
check(sum(row['source'].startswith('tests/preview-groups.qml:') for row in rows) == 7,
      'g06-r2-1 and r5: behavior assertions and primitive failure receipts are all inventoried')
check(sum(row['source'].startswith('tests/preview-check-groups.qml:') for row in rows) == 17,
      'g06-r3-1 and r5: every behavior failure and primitive failure receipt stays visible')
module = runpy.run_path(str(repo / 'tools/flea-parity'))
for name, expected in [('preview-wrappers.qml', {'check', 'finish', 'see', 'seeView', 'relay', 'cycleA',
                                               'cycleB', 'forward', 'arrow', 'block', 'unused', 'demand', 'plan'}),
                       ('preview-wrappers.js', {'record', 'deferred', 'indirect', 'block', 'first', 'second',
                                                'pass', 'forward', 'multiline'})]:
    text = (fixture / 'tests' / name).read_text()
    items = module['tokens'](text)
    wrappers = module['assertion_wrappers'](items, module['pairs'](items), module['pairs'](items, '(', ')'), text)
    check(wrappers == expected, f'g06-r4-1: per-file transitive wrappers in {name}: {wrappers}')
check(sum(row['source'].startswith('tests/preview-wrappers.qml:') for row in rows) == 25,
      'g06-r4-1 and r5: completion, deferred, recursive, assigned and arrow wrappers include all nested sinks')
check(sum(row['source'].startswith('tests/preview-wrappers.js:') for row in rows) == 15,
      'g06-r4-1: top-level JS wrapper calls include transitive methods and omit declarations')
check({'power reader is absent', 'power reader has the wrong owner', 'deferred view', 'stored view',
       'transitive view', 'recursive wrapper', 'assigned wrapper', 'expression arrow wrapper',
       'block arrow wrapper', 'top-level JS transitive case', 'top-level JS deferred case', 'multiline arrow wrapper'} <=
      {row['details'] for row in rows}, 'g06-r4-1: independent wrapper labels survive in the matrix')
check('tests/live.sh:2' in by_id['suite:sibling']['details'] and
      'PKGBUILD:2' in by_id['suite:package']['details'],
      'g06-r2-2: sibling and package dependencies propagate source evidence without execution')
auditor = runpy.run_path(str(fixture / 'completeness.py'))
fixture_forms, _, fixture_excluded = auditor['audit'](fixture, rows)
check(fixture_forms == {'throw': (5, 5), 'fail': (11, 11), 'finish-false': (4, 4),
                       'FAIL-output': (12, 12), 'nonzero-exit': (4, 4), 'assertion': (25, 25),
                       'failure-collection': (6, 6), 'wrapper': (64, 64)},
      f'g06-r5-1: independent fixture completeness by failure form: {fixture_forms}')
check(fixture_excluded == {'successful-Qt-exit': 4}, 'only literal successful Qt completions are excluded')
for form in auditor['FORMS']:
    victim = next(row for row in rows if form in row['sink'].split(','))
    try:
        auditor['audit'](fixture, [row for row in rows if row['id'] != victim['id']])
    except AssertionError as error:
        check('unrepresented failure sites' in str(error), f'g06-r5-1: removing a {form} row fails completeness')
    else:
        raise AssertionError(f'completeness missed removed {form} row')
preserved = list(csv.DictReader((fixture / 'preserved-cases.tsv').open(), delimiter='\t'))
check(len(preserved) == 64 and all(row['id'] in by_id and by_id[row['id']]['source'] == row['source']
                                 for row in preserved), 'g06-r5-1: every pre-inversion fixture case ID and source survives')

status = root / 'status.tsv'
status.write_text('id\tstatus\nevidence-missing\tgreen\n')
run(options + ['--status', str(status)], code=2)
status.write_text('id\tstatus\nipc:flea.ready\tgreen\n')
run(options + ['--status', str(status)], code=2)
status.write_text('id\tstatus\tevidence\nipc:flea.ready\tgreen\t\n')
run(options + ['--status', str(status)], code=2)
status.write_text('id\tstatus\tevidence\nipc:flea.ready\tgreen\tevidence.txt\nboard:picker\tgreen\t\n')
run(options + ['--status', str(status)], code=2)
status.write_text('id\tstatus\tevidence\nipc:flea.ready\tgreen\tmissing.log\n')
rejected_output = root / 'rejected.tsv'
run(options + ['--status', str(status), '--out', str(rejected_output)], code=2)
check(not rejected_output.exists(), 'green without existing evidence never writes a matrix')
status.write_text('id\tstatus\tevidence\nipc:flea.ready\tgreen\tevidence.txt\nboard:picker\truled out\t\n')
overlay = parse(run(options + ['--status', str(status)]))
check(sum(row['status'] == 'green' for row in overlay) == 1 and
      sum(row['status'] == 'ruled out' for row in overlay) == 1, 'exact status overlay')
check(next(row for row in overlay if row['id'] == 'ipc:flea.ready')['evidence'] == 'evidence.txt',
      'green evidence resolves against the tree and remains in the matrix')
summary = run(options + ['--status', str(status), '--summary'])
check('green\tpercent\t0.67\n' in summary and 'status\truled out\t1\n' in summary,
      'green percentage uses all 149 inventory rows')
status.write_text('id\tstatus\nboard:picker\truled out\n')
check(next(row for row in parse(run(options + ['--status', str(status)]))
           if row['id'] == 'board:picker')['status'] == 'ruled out',
      'non-green controller dispositions retain two-column compatibility')
saved = root / 'matrix.tsv'
check(run(options + ['--out', str(saved), '--summary']) == run(options + ['--summary']),
      'out writes the matrix while summary stays on stdout')
check(saved.read_text() == output, 'saved TSV equals stdout TSV')
check(run(options + ['--before', str(saved)]) == output, 'preservation gate retains every prior fixture ID')
invalid_before = root / 'invalid-before.tsv'
invalid_before.write_text('')
run(options + ['--before', str(invalid_before)], code=2)
invalid_before.write_text('id\njs:ui/js/Unmapped.js\njs:ui/js/Unmapped.js\n')
run(options + ['--before', str(invalid_before)], code=2)
markdown = run(options + ['--md'])
check(markdown.startswith('| kind | id | source |') and '\\|' in markdown, 'Markdown header and pipe escaping')
check(len(markdown.splitlines()) == len(rows) + 2, 'one Markdown row per artifact')
status.write_text('id\tstatus\nmissing:row\tgreen\n')
run(options + ['--status', str(status)], code=2)
status.write_text('ipc:flea.ready\tgreen\tevidence.txt\nipc:flea.ready\topen\n')
run(options + ['--status', str(status)], code=2)

copy = root / 'tree'
shutil.copytree(fixture, copy)
modified_options = ['--tree', str(copy), '--pending', str(copy / 'PENDING.md'),
                    '--design', str(copy / 'design-hig'), '--layout', str(copy / 'ci/default.layout')]
check(run(modified_options) == output, 'relocating the complete fixture preserves output bytes')
source = copy / 'ui/Ipc.qml'
source.write_text('// source insertion\n' + source.read_text())
modified = parse(run(modified_options))
check({row['id'] for row in modified} == by_id.keys(), 'source insertions preserve all IDs')
check(next(row for row in modified if row['id'] == 'ipc:flea.ready')['source'] == 'ui/Ipc.qml:8',
      'source insertion updates source line')
group = copy / 'tests/preview-groups.qml'
text = group.read_text()
group.write_text(text.replace('function verify() {', 'function verify() { check(false, "inserted group failure");', 1))
mutated = parse(run(modified_options + ['--before', str(saved)]))
check(len(mutated) == len(modified) + 1 and {row['id'] for row in modified} < {row['id'] for row in mutated},
      'g06-r2-1: inserting check(false) into verify changes the inventory without losing a prior row')
unmapped = copy / 'ui/js/Unmapped.js'
renamed = unmapped.with_suffix('.txt')
unmapped.rename(renamed)
run(modified_options + ['--before', str(saved)], code=2)
reasons = root / 'removed.tsv'
reasons.write_text('id\treason\njs:ui/js/Unmapped.js\t\n')
run(modified_options + ['--before', str(saved), '--removed', str(reasons)], code=2)
reasons.write_text('id\treason\njs:ui/js/Unmapped.js\tFixture intentionally changes this file extension.\n')
run(modified_options + ['--before', str(saved), '--removed', str(reasons)])
renamed.rename(unmapped)
run(modified_options + ['--before', str(saved), '--removed', str(reasons)], code=2)
(copy / 'tests/preview.sh').write_text('#!/bin/bash\ncat ui/Preview.qml\n')
modified = parse(run(modified_options))
check(next(row for row in modified if row['id'] == 'suite:preview')['qml-bound'] == 'yes',
      'reading QML source also binds a suite to QML')
(copy / 'tests/preview.sh').write_text('#!/bin/bash\nbash tests/wrapper.sh\n')
modified = parse(run(modified_options))
check(next(row for row in modified if row['id'] == 'suite:wrapper')['qml-bound'] == 'no',
      'a helper cycle terminates without inventing QML execution')
(copy / 'ci/default.layout').write_text('east broken suites absent\n')
run(modified_options, code=2)

relocated = []
for label in ('input-a', 'input-b'):
    container = root / label
    shutil.copytree(fixture, container / 'tree')
    shutil.copytree(fixture / 'design-hig', container / 'design-hig')
    shutil.copytree(fixture / 'ci', container / 'ci')
    shutil.copyfile(fixture / 'PENDING.md', container / 'PENDING.md')
    overlay_path = container / 'status.tsv'
    overlay_path.write_text(f'id\tstatus\tevidence\nipc:flea.ready\tgreen\t{container}/tree/evidence.txt\n')
    relocated.append(run(['--tree', str(container / 'tree'), '--pending', str(container / 'PENDING.md'),
                          '--design', str(container / 'design-hig'), '--layout', str(container / 'ci/default.layout'),
                          '--status', str(overlay_path)]))
check(relocated[0] == relocated[1], 'external boards, scope, layout and green evidence relocate byte-identically')
check(all(not Path(row['source'].rsplit(':', 1)[0]).is_absolute() for row in parse(relocated[0])),
      'all source metadata is relative to the input tree, including external inputs')
unknown = copy / 'tests/unrecognized.qml'
unknown.write_text('import QtTest\nTestCase { function scenario() { requireCondition(true) } }\n')
(copy / 'ci/default.layout').write_text((fixture / 'ci/default.layout').read_text())
run(modified_options, code=2)

frozen = root / 'frozen-groups'
shutil.copytree(fixture, frozen)
for name in ('figure-component', 'markdown-endfit', 'rename-cost', 'button-system', 'bootload', 'headerhandles',
             'startup-objects', 'folderviews-flow', 'markdown-linearity'):
    shutil.copyfile(Path('/home/gm/flea-work/h323/tests') / f'{name}.qml', frozen / f'tests/{name}.qml')
frozen_options = ['--tree', str(frozen), '--pending', str(frozen / 'PENDING.md'),
                  '--design', str(frozen / 'design-hig'), '--layout', str(frozen / 'ci/default.layout')]
group_before = run(frozen_options)
restored = list(csv.DictReader((fixture / 'restored-cases.tsv').open(), delimiter='\t'))
check(len(restored) == 22 and {row['id'] for row in restored} <= {row['id'] for row in parse(group_before)},
      'g06-r2-1: all 22 previously inventoried frozen row identities are restored')
sources = {row['source'] for row in parse(group_before) if row['kind'] == 'qml-test'}
check('tests/markdown-linearity.qml:409' in sources,
      'g06-r5-1: real postfix-division behavior throw has an individual row')
linearity = frozen / 'tests/markdown-linearity.qml'
original = linearity.read_text()
needle = '            var postfixDivision = "i++ / 2";'
check(original.count(needle) == 1, 'g06-r5-1: real throw mutation anchor is unique')
linearity.write_text(original.replace(needle, '            throw new Error("review unconditional coverage failure"); ' + needle.lstrip(), 1))
mutated = parse(run(frozen_options))
check(len(mutated) == len(parse(group_before)) + 1 and
      {row['id'] for row in parse(group_before)} < {row['id'] for row in mutated} and
      len(linearity.read_text().splitlines()) == len(original.splitlines()),
      'g06-r5-1: caught unconditional exception adds one row and preserves all old IDs and source lines')
linearity.write_text(original)
for name, pattern, expected, needle, injection in [
        ('startup-objects', r'\bfinish\(false\s*,', 45, 'function checkDemand() {',
         ' root.finish(false, "review unconditional startup failure");'),
        ('folderviews-flow', r'\broot\.(?:see|seeView)\(', 57, 'function plan(remember) {',
         ' root.see("review unconditional folder failure", function () { return false }, true);')]:
    source = frozen / f'tests/{name}.qml'
    original = source.read_text()
    sites = {f'tests/{name}.qml:{number}' for number, line in enumerate(original.splitlines(), 1)
             if re.search(pattern, line) and (name != 'folderviews-flow' or number > 97)}
    check(len(sites) == expected and sites <= sources,
          f'g06-r4-1: every independent wrapper site in real {name} has a row')
    check(original.count(needle) == 1, f'g06-r4-1: mutation anchor is unique in {name}')
    source.write_text(original.replace(needle, needle + injection, 1))
    mutation = parse(run(frozen_options))
    check(len(mutation) == len(parse(group_before)) + 1 and
          {row['id'] for row in parse(group_before)} < {row['id'] for row in mutation} and
          len(source.read_text().splitlines()) == len(original.splitlines()),
          f'g06-r4-1: real {name} unconditional wrapper failure adds one case and preserves old IDs and lines')
    source.write_text(original)
for name, sites in [('bootload', (76, 80)), ('headerhandles', (160, 170, 175, 179, 185, 190))]:
    check({f'tests/{name}.qml:{number}' for number in sites} <= sources,
          f'g06-r3-1: every independent failure in real {name}.check() is inventoried')
    source = frozen / f'tests/{name}.qml'
    original = source.read_text()
    source.write_text(original.replace('function check() {',
                      'function check() { root.fail("review unconditional group failure");', 1))
    mutation = run(frozen_options)
    check(len(parse(mutation)) == len(parse(group_before)) + 1 and
          {row['id'] for row in parse(group_before)} < {row['id'] for row in parse(mutation)},
          f'g06-r3-1: unconditional fail in real {name}.check() adds one row without losing any ID')
    source.write_text(original)
figure = frozen / 'tests/figure-component.qml'
figure.write_text(figure.read_text().replace('function verify() {',
                  'function verify() { probe.check(false, "inserted frozen failure");', 1))
group_after = run(frozen_options)
check(len(parse(group_after)) == len(parse(group_before)) + 1 and
      {row['id'] for row in parse(group_before)} < {row['id'] for row in parse(group_after)},
      'g06-r2-1: a check(false) in the real figure verify changes output and preserves its old cases')

real = parse(run(['--tree', str(repo), '--pending', '/home/gm/flea-ops/claude/releases/PENDING.md']))
real_counts = collections.Counter(row['kind'] for row in real)
check(set(real_counts) == set(counts) and all(value > 0 for value in real_counts.values()),
      f'real tree has every artifact kind: {real_counts}')
check(len({row['id'] for row in real}) == len(real), 'real-tree IDs are unique')
for name in ('sheet-query', 'settings-columns', 'network-alias'):
    check(any(row['kind'] == 'qml-test' and row['source'].startswith(f'tests/{name}.qml:') for row in real),
          f'g06-r1-1: real {name} harness cannot silently yield zero cases')
    sites = {f'tests/{name}.qml:{number}' for number, line in
             enumerate((repo / f'tests/{name}.qml').read_text().splitlines(), 1)
             if re.search(r'\bexpect\s*\(', line) and not re.search(r'\bfunction\s+expect\s*\(', line)}
    check(sites <= {row['source'] for row in real if row['kind'] == 'qml-test'},
          f'g06-r1-1: every expect() source site in {name} has a parity row')
for name in ('empty-state', 'connections-style', 'drag-look235', 'figure-package'):
    finding = 'g06-r1-2' if name in ('empty-state', 'connections-style') else 'g06-r2-2'
    check(next(row for row in real if row['id'] == f'suite:{name}')['qml-bound'] == 'yes',
          f'{finding}: real {name} remains QML-bound')
frozen_tree = Path('/home/gm/flea-work/h323')
frozen_rows = parse(run(['--tree', str(frozen_tree), '--pending', '/home/gm/flea-ops/claude/releases/PENDING.md']))
sink_counts, scanned, excluded = auditor['audit'](frozen_tree, frozen_rows, root / 'sink-sites.tsv')
check(scanned == 533, f'g06-r5-1: every frozen QML/JS test is scanned: {scanned}')
for form, (sites, represented) in sink_counts.items():
    check(sites > 0 and sites == represented, f'g06-r5-1: completeness for {form}: {sites}/{represented}')
    print(f'parity sinks: {form}: {sites} sites, {represented} rows')
print(f'parity exclusions: {excluded}; known successful Qt exits only; comments, literal data and declarations are not sink sites')
unassigned = sum(row['family'] == 'unassigned' for row in real)
check(unassigned / len(real) < 0.05, f'real-tree unassigned fraction: {unassigned}/{len(real)}')
print(f'parity: {checks} checks, 0 failed; fixture {len(rows)} rows; real {len(real)} rows; unassigned {unassigned}')
PY
