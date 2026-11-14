"""Exercise the production checker with isolated valid twins and named mutants."""

import csv
import io
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys


repo, root = map(Path, sys.argv[1:])
assert root.is_absolute() and (root / '.suite-inventory-test-root').is_file()
csv.field_size_limit(16 * 1024 * 1024)
fixture = repo / 'tests/fixtures/suite-inventory'
tool = [sys.executable, '-B', str(repo / 'tools/flea-suite-inventory')]
generator = [sys.executable, '-B', str(repo / 'tools/flea-parity')]
checks = 0
mutants = 0


def check(condition, label):
    global checks
    assert condition, label
    checks += 1


def execute(command, expected=0, env=None):
    result = subprocess.run(command, capture_output=True, text=True, check=False, env=env)
    check(result.returncode == expected,
          f'expected exit={expected}, got {result.returncode}: {result.stdout}\n{result.stderr}')
    return result


def parse(text):
    return list(csv.DictReader(io.StringIO(text), delimiter='\t'))


parity = root / 'parity.tsv'
execute(generator + ['--suite-cases', '--tree', str(repo),
                     '--layout', str(fixture / 'layouts/default.layout'), '--out', str(parity)])
options = ['--evidence', str(fixture / 'suite-evidence.txt'), '--layouts', str(fixture / 'layouts')]
baseline = execute(tool + options + ['--parity', str(parity)]).stdout
check(baseline.startswith('SUITE_INVENTORY PASS suites=') and 'retired=3' in baseline.split(), 'committed inventory passes')
check(execute(tool + options).stdout == baseline, 'default checker invokes the same g06 projection')
small = repo / 'tests/fixtures/parity'
small_args = ['--tree', str(small), '--layout', str(small / 'ci/default.layout')]
complete = execute(generator + small_args + ['--design', str(small / 'design-hig'),
                                            '--pending', str(small / 'PENDING.md')]).stdout
projection = execute(generator + small_args + ['--suite-cases', '--design', str(root / 'absent'),
                                              '--pending', str(root / 'absent')]).stdout
check(parse(projection) == [row for row in parse(complete) if row['kind'] in ('suite', 'js-test', 'qml-test')],
      'g06 projection is byte-equivalent rows without external scope or design reads')
rows = parse((repo / 'tests/suite-dispositions.tsv').read_text())
by_name = {row['suite']: row for row in rows}
check(len(by_name) == len(rows), 'every committed suite has one row')
check(sum(row['state'] == 'active' for row in rows) >= 303, 'all original active suites remain active')


def sandbox(name):
    directory = root / name
    directory.mkdir()
    shutil.copyfile(repo / 'tests/suite-dispositions.tsv', directory / 'dispositions.tsv')
    shutil.copyfile(repo / 'tests/run-all.sh', directory / 'run-all.sh')
    shutil.copyfile(fixture / 'suite-evidence.txt', directory / 'suite-evidence.txt')
    shutil.copytree(fixture / 'layouts', directory / 'layouts')
    return directory


def command(directory):
    local_tool = directory / 'tools/flea-suite-inventory'
    checker = [sys.executable, '-B', str(local_tool)] if local_tool.is_file() else tool
    return checker + ['--evidence', str(directory / 'suite-evidence.txt'),
                   '--layouts', str(directory / 'layouts'), '--parity', str(parity),
                   '--dispositions', str(directory / 'dispositions.tsv'),
                   '--runner', str(directory / 'run-all.sh')]


def update_row(directory, name, **changes):
    path = directory / 'dispositions.tsv'
    data = parse(path.read_text())
    for row in data:
        if row['suite'] == name:
            row.update(changes)
    with path.open('w') as stream:
        writer = csv.DictWriter(stream, rows[0].keys(), delimiter='\t', lineterminator='\n')
        writer.writeheader()
        writer.writerows(data)


def replace(path, old, new):
    text = path.read_text()
    check(old in text, f'mutation anchor exists in {path.name}')
    path.write_text(text.replace(old, new, 1))


def mutant(name, kind, suite, change, repair):
    global mutants
    directory = sandbox(name)
    change(directory)
    result = execute(command(directory), expected=1)
    lines = [line for line in result.stderr.splitlines() if line.startswith(f'SUITE_INVENTORY {kind} suite={suite}:')]
    check(len(lines) == 1, f'{name} has its own diagnostic: {result.stderr}')
    check('SUITE_INVENTORY PASS' not in result.stdout, f'{name} never prints a pass')
    (directory / 'mutant.log').write_text(result.stderr)
    repair(directory)
    twin = execute(command(directory))
    check(twin.stdout.startswith('SUITE_INVENTORY PASS '), f'{name} valid twin passes')
    mutants += 1
    print(f'SUITE_INVENTORY MUTANT {name} exit=1 twin=0: {lines[0]}')


mutant('orphan-run-all', 'missing-row', 'inventory-orphan',
       lambda d: replace(d / 'run-all.sh', 'headless="', 'headless="inventory-orphan '),
       lambda d: replace(d / 'run-all.sh', 'headless="inventory-orphan ', 'headless="'))


def duplicate(directory):
    path = directory / 'dispositions.tsv'
    with path.open('a') as stream:
        stream.write(path.read_text().splitlines(True)[1])


mutant('duplicated-row', 'duplicate-row', rows[0]['suite'], duplicate,
       lambda d: shutil.copyfile(repo / 'tests/suite-dispositions.tsv', d / 'dispositions.tsv'))
mutant('retired-no-replacement', 'retired-replacement', 'bench',
       lambda d: update_row(d, 'bench', state='retired', replacement='absent-suite'),
       lambda d: update_row(d, 'bench', replacement='suite-inventory'))


def remove_evidence(directory):
    path = directory / 'suite-evidence.txt'
    path.write_text(''.join(line for line in path.read_text().splitlines(True) if not line.startswith('suite-inventory|')))


mutant('active-no-evidence', 'missing-evidence', 'suite-inventory', remove_evidence,
       lambda d: shutil.copyfile(fixture / 'suite-evidence.txt', d / 'suite-evidence.txt'))
rule = next(line for line in (fixture / 'suite-evidence.txt').read_text().splitlines(True)
            if line.startswith('suite-inventory|'))
zero_rule = rule.rsplit('|', 1)[0] + '|0\n'
mutant('zero-floor', 'evidence-floor', 'suite-inventory',
       lambda d: replace(d / 'suite-evidence.txt', rule, zero_rule),
       lambda d: replace(d / 'suite-evidence.txt', zero_rule, rule))


def layout_duplicate(directory):
    path = directory / 'layouts/default.layout'
    path.write_text(path.read_text() + 'east duplicate suites suite-inventory\n')


mutant('layout-duplicate', 'layout-duplicate', 'suite-inventory', layout_duplicate,
       lambda d: replace(d / 'layouts/default.layout', 'east duplicate suites suite-inventory\n', ''))
mutant('layout-miss', 'layout-missing', 'protocol',
       lambda d: replace(d / 'layouts/default.layout', 's1 suites protocol ', 's1 suites '),
       lambda d: replace(d / 'layouts/default.layout', 's1 suites ', 's1 suites protocol '))
mutant('unknown-parity', 'unknown-parity', 'suite-inventory',
       lambda d: update_row(d, 'suite-inventory', parity='case:inventory-unknown'),
       lambda d: update_row(d, 'suite-inventory', parity=by_name['suite-inventory']['parity']))


def scheduled_retired(directory):
    update_row(directory, 'suite-inventory', state='retired', replacement='js')
    remove_evidence(directory)


def restore_active(directory):
    update_row(directory, 'suite-inventory', state='active', replacement=by_name['suite-inventory']['replacement'])
    shutil.copyfile(fixture / 'suite-evidence.txt', directory / 'suite-evidence.txt')


mutant('scheduled-retired', 'headless-inactive', 'suite-inventory', scheduled_retired, restore_active)


def excluded_member(directory):
    path = directory / 'layouts/default.layout'
    path.write_text(path.read_text() + 'east excluded suites menu-copyas-reveal\n')


mutant('excluded-layout-member', 'layout-inactive', 'menu-copyas-reveal', excluded_member,
       lambda d: replace(d / 'layouts/default.layout', 'east excluded suites menu-copyas-reveal\n', ''))


def script_snapshot(directory):
    (directory / 'tools').mkdir()
    shutil.copyfile(repo / 'tools/flea-suite-inventory', directory / 'tools/flea-suite-inventory')
    (directory / 'tests').mkdir()
    for name, row in by_name.items():
        if row['state'] == 'active':
            shutil.copy2(repo / 'tests' / f'{name}.sh', directory / 'tests' / f'{name}.sh')


def missing_script(directory):
    script_snapshot(directory)
    (directory / 'tests/protocol.sh').rename(directory / 'tests/protocol.sh.missing')


def non_executable_script(directory):
    script_snapshot(directory)
    (directory / 'tests/protocol.sh').chmod(0o600)


mutant('missing-script', 'missing-script', 'protocol', missing_script,
       lambda d: (d / 'tests/protocol.sh.missing').rename(d / 'tests/protocol.sh'))
mutant('non-executable-script', 'non-executable-script', 'protocol', non_executable_script,
       lambda d: shutil.copystat(repo / 'tests/protocol.sh', d / 'tests/protocol.sh'))


def without_aur_suites(path):
    # The frozen stage runner and layout still schedule the two AUR suites this tree removed with the AUR packages.
    for name in ('aurpush', 'aur-versions'):
        replace(path, f' {name} ', ' ')


cross_tree = sandbox('cross-tree-layout')
script_snapshot(cross_tree)
shutil.copyfile(fixture / 'batches/default.runner', cross_tree / 'tests/run-all.sh')
without_aur_suites(cross_tree / 'tests/run-all.sh')
cross_command = tool + ['--layout-only', '--tree', str(cross_tree),
                        '--layout', str(cross_tree / 'layouts/default.layout'),
                        '--evidence', str(cross_tree / 'suite-evidence.txt')]
red = execute(cross_command, expected=1)
check('layout-unregistered suite=jobs:' in red.stderr, 'F1 suites cannot be scheduled on the stage runner')
check('SUITE_LAYOUT PASS' not in red.stdout, 'cross-tree layout never prints a pass')
shutil.copyfile(fixture / 'batches/default.layout', cross_tree / 'layouts/default.layout')
without_aur_suites(cross_tree / 'layouts/default.layout')
twin = execute(cross_command)
check('SUITE_LAYOUT PASS layout=default.layout suites=298' in twin.stdout,
      'the stage runner passes against its own layout')
mutants += 1
diagnostic = next(line for line in red.stderr.splitlines() if 'layout-unregistered suite=jobs:' in line)
print(f'SUITE_INVENTORY MUTANT cross-tree-layout exit=1 twin=0: {diagnostic}')

ci_input = root / 'selected-ci-input'
ci_input.mkdir()
(ci_input / 'worker-files.sha256').touch()
shutil.copyfile(fixture / 'layouts/default.layout', ci_input / 'suite-inventory.layout')
shutil.copyfile(fixture / 'suite-evidence.txt', ci_input / 'suite-evidence.txt')
environment = os.environ.copy()
for key in ('FLEA_CI_LAYOUT', 'FLEA_CI_EVIDENCE'):
    environment.pop(key, None)
environment['FLEA_CI_INPUT_DIR'] = str(ci_input)
entry = ['bash', '-c', 'source "$1"; suite_inventory_layout "$2"',
         'suite-inventory-input', str(repo / 'tests/suite-inventory.sh'), str(repo)]
check(execute(entry, env=environment).stdout == baseline, 'suite reads the CI-staged selected layout and evidence')
shutil.copyfile(fixture / 'batches/default.layout', ci_input / 'suite-inventory.layout')
without_aur_suites(ci_input / 'suite-inventory.layout')
check('layout-missing suite=jobs:' in execute(entry, expected=1, env=environment).stderr,
      'suite rejects a selected layout missing suites registered by its own tree')
shutil.copyfile(fixture / 'layouts/default.layout', ci_input / 'suite-inventory.layout')
check(execute(entry, env=environment).stdout == baseline, 'selected-layout control has a passing twin')
(ci_input / 'suite-inventory.layout').rename(ci_input / 'saved.layout')
check('input-error:' in execute(entry, expected=1, env=environment).stderr,
      'CI input cannot silently fall back to committed fixture layouts')
(ci_input / 'saved.layout').rename(ci_input / 'suite-inventory.layout')
check(execute(entry, env=environment).stdout == baseline, 'restored staged layout passes')


def unschedule(directory, name):
    path = directory / 'run-all.sh'
    path.write_text(re.sub(r'^headless="([^"\n]*)"$',
                          lambda match: 'headless="' + ' '.join(word for word in match[1].split() if word != name) + '"',
                          path.read_text(), flags=re.M))
    for path in (directory / 'layouts').glob('*.layout'):
        lines = []
        for line in path.read_text().splitlines(True):
            words = shlex.split(line, comments=True)
            if len(words) > 3 and words[2] == 'suites' and name in words[3:]:
                line = ' '.join(words[:3] + [word for word in words[3:] if word != name]) + '\n'
            lines.append(line)
        path.write_text(''.join(lines))


inherited = sandbox('default-inheritance')
(inherited / 'layouts/inherited.layout').write_text('default\n')
check(execute(command(inherited)).stdout == baseline, 'inherited default counts all base suites once')
(inherited / 'layouts/inherited.layout').write_text('default\neast extra suites protocol\n')
check('layout-duplicate suite=protocol:' in execute(command(inherited), 1).stderr,
      'inherited and explicit suite duplicate is refused')
(inherited / 'layouts/inherited.layout').write_text('default\n')
replace(inherited / 'layouts/default.layout', 's1 suites protocol ', 's1 suites ')
check('suite=protocol: inherited.layout' in execute(command(inherited), 1).stderr,
      'inherited layout miss is refused')
replacement = sandbox('replacement-evidence')
update_row(replacement, 'parity', state='retired', replacement='suite-inventory')
remove_evidence(replacement)
check('retired-replacement suite=parity:' in execute(command(replacement), 1).stderr,
      'replacement evidence is required even when the replacement is active')
for name, mechanism in [('signalarity', 'obsolete: QML signal/emit arity'),
                        ('connections-style', 'obsolete: QML Connections')]:
    directory = sandbox('retirement-' + name)
    update_row(directory, name, state='retired', replacement=mechanism)
    unschedule(directory, name)
    check('retired=4' in execute(command(directory)).stdout.split(), f'{name} obsolete mechanism can retire')
    update_row(directory, 'parity', state='retired', replacement=mechanism)
    check('retired-replacement suite=parity:' in execute(command(directory), 1).stderr,
          'obsolete mechanism exception does not exempt other suites')
excluded = sandbox('excluded-is-not-active')
update_row(excluded, 'rename-grid-bottom', state='active')
check('unrun-row suite=rename-grid-bottom:' in execute(command(excluded), 1).stderr,
      'not_run entry cannot masquerade as an active headless suite')
for name, kind in [('qml-compile', 'evidence-unaccounted'), ('ui-trash', 'parity-unaccounted')]:
    directory = sandbox('unaccounted-' + name)
    path = directory / 'dispositions.tsv'
    path.write_text(''.join(line for line in path.read_text().splitlines(True) if not line.startswith(name + '\t')))
    check(f'{kind} suite={name}:' in execute(command(directory), 1).stderr,
          f'{name} cannot disappear from the evidence or g06 inventory')
check(mutants == 13, 'all thirteen requested mutants ran with passing twins')
print(baseline.strip())
print(f'suite-inventory: {checks} checks, 0 failed; {mutants} mutants rejected with passing twins')
