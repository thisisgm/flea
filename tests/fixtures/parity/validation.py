"""Exercise inventory validation and proof identity through the CLI."""

import copy
import csv
import hashlib
import os
from pathlib import Path
import shutil
import subprocess


def validate(context):
    root, fixture = (context[name] for name in ('root', 'fixture'))
    check, run, parse = (context[name] for name in ('check', 'run', 'parse'))
    options, rows, auditor = (context[name] for name in ('options', 'rows', 'auditor'))
    for form in auditor['FORMS']:
        original = next(row for row in rows if form in row['sink'].split(','))
        remaining = set(original['sink'].split(',')) - {form}
        wrong = 'throw' if form == 'assertion' else 'assertion'
        for replacement in (remaining, remaining | {wrong}):
            altered = copy.deepcopy(rows)
            victim = next(row for row in altered if row['id'] == original['id'])
            victim['sink'] = ','.join(sorted(replacement))
            try:
                auditor['audit'](fixture, altered)
            except AssertionError as error:
                check('unrepresented failure sites' in str(error) and original['source'] in str(error)
                      and form in str(error), f'wrong {form} has a source-specific rejection')
            else:
                raise AssertionError(f'completeness accepts corrupted {form}: {replacement!r}')

    tree = root / 'validation-tree'
    shutil.copytree(fixture, tree)
    tree_options = ['--tree', str(tree), '--pending', str(tree / 'PENDING.md'),
                    '--design', str(tree / 'design-hig'), '--layout', str(tree / 'ci/default.layout')]
    schema = tree / 'src/uischema.rs'
    original_schema = schema.read_text()
    for document, key in [('{"list":[{"x":1},{"y":2}]}', 'list'),
                          ('{"group":{"list":[1,{"x":1}]}}', 'group.list'),
                          ('{"list":[[{"x":1}]]}', 'list')]:
        schema.write_text('pub const DEFAULTS: &str = r#"' + document + '"#;\n')
        run(tree_options, code=2, reason=f'DEFAULTS array of objects is unsupported at key {key}')
    schema.write_text(original_schema)
    check(run(tree_options) == run(options), 'supported scalar arrays keep the complete fixture inventory')

    status = root / 'validation-status.tsv'
    for value in ('Green', 'gren', 'green '):
        status.write_text(f'id\tstatus\tevidence\nipc:flea.ready\t{value}\t\n')
        run(tree_options + ['--status', str(status)], code=2,
            reason=f'unknown status for ipc:flea.ready: {value!r}')
    for value in ('open', 'green', 'ruled out'):
        status.write_text(f'id\tstatus\tevidence\nipc:flea.ready\t{value}\tevidence.txt\n')
        marked = parse(run(tree_options + ['--status', str(status)]))
        check(next(row for row in marked if row['id'] == 'ipc:flea.ready')['status'] == value,
              f'accepted status remains exact: {value}')

    outside = root / 'outside-proof.txt'
    outside.write_text('Evidence outside the input tree.\n')
    (tree / 'escape').symlink_to(outside)
    for evidence in ('../outside-proof.txt', str(outside), 'escape'):
        status.write_text(f'id\tstatus\tevidence\nipc:flea.ready\tgreen\t{evidence}\n')
        run(tree_options + ['--status', str(status)], code=2,
            reason=f'evidence path escapes --tree for ipc:flea.ready: {evidence}')
    for evidence in ('evidence.txt', str(tree / 'evidence.txt')):
        status.write_text(f'id\tstatus\tevidence\nipc:flea.ready\tgreen\t{evidence}\n')
        marked = parse(run(tree_options + ['--status', str(status)]))
        proof = next(row for row in marked if row['id'] == 'ipc:flea.ready')
        check(proof['status'] == 'green' and proof['evidence'] == 'evidence.txt',
              'both inside-tree evidence forms retain green and normalized provenance')

    source = tree / 'tests/js/identity.js'
    source.write_text('function run() {\n check("same", state === 1);\n check("same", state === 2);\n}\n')
    initial = run(tree_options)
    before = root / 'identity-before.tsv'
    before.write_text(initial)
    first = next(row for row in parse(initial) if row['source'] == 'tests/js/identity.js:2')
    status.write_text(f'id\tstatus\tevidence\n{first["id"]}\tgreen\tevidence.txt\n')
    source.write_text(source.read_text().replace('function run() {\n',
                      'function run() {\n check("same", state === 0);\n'))
    changed = parse(run(tree_options + ['--before', str(before), '--status', str(status)]))
    green = next(row for row in changed if row['status'] == 'green')
    check(green['id'] == first['id'] and green['source'] == 'tests/js/identity.js:3',
          'same-label insertion retains proof on its original expression')
    source.write_text('// unrelated source insertion\n' + source.read_text())
    shifted = parse(run(tree_options + ['--status', str(status)]))
    check(any(row['id'] == first['id'] and row['source'] == 'tests/js/identity.js:4' for row in shifted),
          'comment insertion shifts source metadata without changing content identity')

    source.write_text('function run() { check("same", true); check("same", true); }\n')
    repeated = run(tree_options)
    repeated_rows = [row for row in parse(repeated) if row['source'].startswith('tests/js/identity.js:')]
    check(len(repeated_rows) == 2 and len({row['id'] for row in repeated_rows}) == 2,
          'repeated expressions retain separate content-bound source sites')
    before.write_text(repeated)
    first = repeated_rows[0]
    status.write_text(f'id\tstatus\tevidence\n{first["id"]}\tgreen\tevidence.txt\n')
    source.write_text('function run() { check("same", true); check("same", true); check("same", true); }\n')
    run(tree_options + ['--before', str(before)], code=2, reason='previous rows disappeared without reasons:')
    run(tree_options + ['--status', str(status)], code=2, reason='invalid, duplicate or unknown status row:')

    pending = tree / 'PENDING.md'
    pending.write_text('## 0.4.0\n| Same title | alpha |\n| Same title | beta |\n')
    initial = run(tree_options)
    before.write_text(initial)
    first = next(row for row in parse(initial) if row['kind'] == 'scope' and 'alpha' in row['details'])
    status.write_text(f'id\tstatus\tevidence\n{first["id"]}\tgreen\tevidence.txt\n')
    pending.write_text('## 0.4.0\n| Same title | gamma |\n| Same title | alpha |\n| Same title | beta |\n')
    changed = parse(run(tree_options + ['--before', str(before), '--status', str(status)]))
    check(any(row['id'] == first['id'] and row['status'] == 'green' and 'alpha' in row['details']
              for row in changed), 'scope content retains green across a same-title insertion')
    pending.write_text('## 0.4.0\n- Identical scope\n- Identical scope\n')
    run(tree_options, code=2, reason='duplicate row IDs:')
    pending.write_text('## 0.4.0\n- TUI dual pane parity\n- TUI Quick Look\n- #299 i18n Grid\n')
    marked = [row for row in parse(run(tree_options)) if row['kind'] == 'scope']
    assigned = {row['details'].split(': ', 1)[1]: (row['family'], row['owner']) for row in marked}
    check(assigned == {'- TUI dual pane parity': ('Drag, clipboard, windows', 'T'),
                       '- TUI Quick Look': ('Drag, clipboard, windows', 'T'),
                       '- #299 i18n Grid': ('Settings', 'G0.10')},
          'explicit scope rules outrank overlapping generic patterns')

    no_git = root / 'source-export'
    (no_git / 'tests').mkdir(parents=True)
    (no_git / 'tests/rename-cost.qml').write_text('// A changed live test never supplies frozen bytes.\n')
    corpus = context['corpus']
    archive_sources = list(corpus['sources'](no_git, fixture, check))
    check(len(archive_sources) == corpus['SOURCE_COUNT'], 'a source export reads every immutable source without Git')
    check(next(data for path, data in archive_sources if path == Path('tests/rename-cost.qml')) !=
          (no_git / 'tests/rename-cost.qml').read_bytes(), 'a live corpus edit leaves the frozen oracle unchanged')
    corrupt = root / 'corrupt-corpus'
    corrupt.mkdir()
    (corrupt / 'frozen-sources.zip').symlink_to(fixture / 'frozen-sources.zip')
    # Sample input: path\tsha256 followed by tests/rename-cost.qml\t<64 hexadecimal digits> (TSV).
    manifest = list(csv.DictReader((fixture / 'frozen-sources.tsv').open(), delimiter='\t'))
    manifest[0]['sha256'] = '0' * 64
    with (corrupt / 'frozen-sources.tsv').open('w') as stream:
        writer = csv.DictWriter(stream, ('path', 'sha256'), delimiter='\t', lineterminator='\n')
        writer.writeheader()
        writer.writerows(manifest)
    try:
        list(corpus['sources'](no_git, corrupt, check))
    except AssertionError as error:
        check(f'frozen v0.3.12 source bytes: {manifest[0]["path"]}' in str(error),
              'the source-export fallback still refuses a wrong frozen hash')
    else:
        raise AssertionError('a corrupted frozen manifest was accepted')

    # A `git clone --depth 1` checkout lacks the frozen commit, so the corpus reads the hash-verified archive instead.
    git_env = {**os.environ, 'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': os.devnull}

    def git(*arguments):
        made = subprocess.run(['git', '-c', 'user.name=parity', '-c', 'user.email=parity@invalid', *arguments],
                              env=git_env, capture_output=True, text=True)
        check(made.returncode == 0, f'git {arguments[0]} for the shallow corpus clone: {made.stderr}')

    shallow_origin, shallow_clone = root / 'shallow-origin', root / 'shallow-clone'
    git('init', '-q', str(shallow_origin))
    for generation in ('first', 'second'):
        git('-C', str(shallow_origin), 'commit', '-q', '--allow-empty', '-m', generation)
    git('clone', '-q', '--depth', '1', shallow_origin.as_uri(), str(shallow_clone))
    check((shallow_clone / '.git/shallow').is_file(), 'the corpus clone is shallow')
    shallow_sources = dict(corpus['sources'](shallow_clone, fixture, check))
    check(not corpus['pinned_commit'](shallow_clone, check), 'the shallow clone lacks the frozen commit')
    check(len(shallow_sources) == corpus['SOURCE_COUNT']
          and all(data == (context['frozen_tree'] / path).read_bytes() for path, data in shallow_sources.items()),
          'a shallow clone yields the frozen corpus from the committed copy')
    try:
        list(corpus['sources'](shallow_clone, corrupt, check))
    except AssertionError as error:
        check(f'frozen v0.3.12 source bytes: {manifest[0]["path"]}' in str(error),
              'the shallow-clone fallback still refuses a wrong frozen hash')
    else:
        raise AssertionError('a shallow clone accepted a corrupted frozen manifest')

    # A .git file that is no gitfile makes rev-parse exit 128 there; an empty .git directory would find the parent repo.
    broken = root / 'broken-repo'
    broken.mkdir()
    (broken / '.git').write_text('not a git directory\n')
    calls = []
    looked_up = corpus['pinned_commit'](broken, lambda condition, label: calls.append((bool(condition), label)))
    check(len(calls) == 1 and not calls[0][0] and 'Git could not look up the frozen commit' in calls[0][1]
          and 'invalid gitfile format' in calls[0][1] and looked_up is False,
          f'a broken repository fails the frozen commit lookup by name: {calls}')

    # Sample input: old-id\tnew-id\tsource\tcolumn\tsink\tsource-sha256 (TSV header).
    migration = list(csv.DictReader((fixture / 'case-id-migration.tsv').open(), delimiter='\t'))
    # Sample input: id\tsource followed by qml-test:tests/preview.qml:test_named\ttests/preview.qml:10 (TSV).
    snapshots = {name: list(csv.DictReader((fixture / name).open(), delimiter='\t'))
                 for name in ('preserved-cases.tsv', 'restored-cases.tsv')}
    expected_migrations = sum(map(len, snapshots.values()))
    check(len(migration) == expected_migrations and len({row['old-id'] for row in migration}) == expected_migrations,
          'all 64 preserved and 22 restored fixture identities have an audited migration')
    original_rows = {row['id']: row for row in rows + parse(context['group_before'])}
    for item in migration:
        row = original_rows[item['new-id']]
        check(all(row[key] == item[key] for key in ('source', 'column', 'sink')),
              f'migration retains source, column and sink for {item["old-id"]}')
        path = fixture / row['source'].rsplit(':', 1)[0]
        if not path.exists():
            path = context['frozen_tree'] / row['source'].rsplit(':', 1)[0]
        check(hashlib.sha256(path.read_bytes()).hexdigest() == item['source-sha256'],
              f'migration remains bound to the exact original source bytes: {item["old-id"]}')
    for snapshot in snapshots.values():
        for row in snapshot:
            check(sum(item['new-id'] == row['id'] and item['source'] == row['source'] for item in migration) == 1,
                  f'fixture migration retains exactly one original source site: {row["id"]}')
