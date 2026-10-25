#!/bin/bash
# Checks product launches, frontend isolation and identities without a display.
set -eu
cd "$(dirname "$0")/.."
scratch=$(mktemp -d)
: > "$scratch/.flea-test-sandbox"
trap 'case $scratch in /*/*) [ -n "$scratch" ] && [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
python3 -B - "$scratch" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from importlib.machinery import SourceFileLoader
from unittest.mock import patch

root = Path(sys.argv[1])
sys.path.insert(0, str(Path('tools').resolve()))
S = SourceFileLoader('product_search', 'tools/flea-field-search').load_module()
N = SourceFileLoader('product_nav', 'tools/flea-field-nav').load_module()
I = SourceFileLoader('product_instant', 'tools/flea-field-instant').load_module()
R = SourceFileLoader('product_readme', 'tools/flea-bench-readme').load_module()
S.ENV = dict(os.environ)
S.STATE_ROOT = str(root / 'harness-state')

def stub(name, version):
    path = root / name / 'bin' / 'flea'
    path.parent.mkdir(parents=True)
    path.write_text('#!/bin/sh\ncase "$1" in\n--version) echo "flea ' + version + '";;\n--ui-state) echo \'{"view":"grid"}\';;\nesac\n')
    path.chmod(0o755)
    return str(path)

fresh, old = stub('candidate', '0.4.0'), stub('anchor', '0.3.12')
gui = Path(fresh).with_name('flea-gui')
gui.write_text('native product fixture')
gui.chmod(0o755)
ctx = {'states': {}, 'out_dir': str(root)}
arms = [{'id': name, 'entrant': 'flea', 'flea_bin': binary, 'frontend': frontend, 'view': 'grid'}
        for name, binary, frontend in [('fresh', fresh, 'gpui'), ('anchor', old, 'qml')]]
with patch.dict(os.environ, {'FLEA_FRONTEND': 'gpui'}):
    launches = [S.entrant_argv(arm, '/fixture with spaces', ctx) for arm in arms]
    for arm, (argv, env) in zip(arms, launches):
        assert argv == [arm['flea_bin'], '--gui', '/fixture with spaces']
        assert env['FLEA_FRONTEND'] == arm['frontend']
        assert Path(env['XDG_STATE_HOME']).is_relative_to(root)
        assert S.entrant_spec(arm)['cls'] == ('flea' if arm['frontend'] == 'gpui' else S.FLEA_CLASS)
        N.require_seeded_view(arm, env)
    assert launches[0][1]['XDG_STATE_HOME'] != launches[1][1]['XDG_STATE_HOME']
    # The QML cache outlives the run's state, one per binary by its sha256.
    for arm, (argv, env) in zip(arms, launches):
        digest = hashlib.sha256(Path(arm['flea_bin']).read_bytes()).hexdigest()
        assert env['QML_DISK_CACHE_PATH'] == str(root / 'harness-state/qmlcache' / digest), env
        assert S.entrant_argv(arm, '/fixture', {'states': {}, 'out_dir': str(root)})[1]['QML_DISK_CACHE_PATH'] == env['QML_DISK_CACHE_PATH']
    with patch.object(S.Product, 'output', wraps=S.Product.output) as observed:
        identities = {arm['id']: S.arm_identity(arm) for arm in arms}
    assert identities['fresh']['version'] == 'flea 0.4.0'
    assert identities['anchor']['version'] == 'flea 0.3.12'
    assert identities['fresh']['gui_sha256'] == hashlib.sha256(gui.read_bytes()).hexdigest()
    assert R.search_version(['anchor'], 'flea', {}, {}, '0.4.0', 'fixture', [], identities) == 'flea 0.3.12'
    assert S.entrant_argv({'entrant': 'strata', 'strata_bin': fresh}, '/fixture', {})[0][0] == fresh
    assert S.entrant_argv({'entrant': 'strata', 'strata_bin': old}, '/fixture', {})[0][0] == old
    assert I.external_clients([{'class': 'flea', 'pid': 7}, {'class': 'flea', 'pid': 8}], {7}) == [{'class': 'flea', 'pid': 8}]

# The actual list adapter must discover the owned native instance, not invoke qs.
proc = type('Process', (), {'pid': os.getpid()})()
client = I.Client(fresh, frontend='gpui')
reply = 'Instance 9:\n  Process ID: 9\n  Config path: /fixture/ui/boot/shell.qml\n'
with patch.object(I.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, stdout=reply)), patch.object(I.S, 'descendants', return_value={9}):
    assert I.qs_pid({'tree': '/fixture'}, proc, client) == 9
assert client.environment({})['FLEA_IPC_KIND'] == 'gpui'

# Read the real entrant builder, including both Strata channels and the TUI anchor.
text = Path('tools/flea-field-bench').read_text()
block = text[text.index('build_entrants() {'):text.index('\nbuild_entrants\n')]
env = dict(os.environ, FLEA_FRONTEND='gpui', FLEA_UI='/candidate/ui', FLEA_BIN=fresh,
           FLEA_QMLCACHE='/cache', FLEA_STATE='/state', ANCHOR_REPO='/anchor', ANCHOR_BIN=old,
           ANCHOR_UI='/anchor/ui', ANCHOR_QMLCACHE='/cache-old', ANCHOR_STATE='/state-old',
           STRATA_BIN=fresh, STRATA_NIGHTLY_BIN=old, TUI_CONF='/config', TUI_CLASS='bench',
           TUI_TERM='kitty', FIXTURE='/fixture', WITH_ANCHOR='yes')
rows = subprocess.check_output(['bash', '-c', block + '\nbuild_entrants\nprintf "%s\\n" "${ENTRANTS[@]}"'], env=env, text=True).splitlines()
roster = {row.split('|')[0]: row.split('|', 7) for row in rows}
assert roster['flea'][2:5] == ['flea', 'flea-gui', str(gui)]
assert 'FLEA_FRONTEND=gpui' in roster['flea'][7] and ' --gui ' in roster['flea'][7]
assert 'FLEA_FRONTEND=qml' in roster['flea-anchor'][7]
assert old + ' --tui ' in roster['flea-tui-anchor'][7]
assert old in roster['strata-nightly'][7]
cap = Path('tools/flea-bench-capability').read_text()
cap_block = cap[cap.index('# capability roster begin'):cap.index('# capability roster end')]
cap_env = dict(env, FLEA_REPO=str(Path.cwd()))
cap_rows = subprocess.check_output(['bash', '-c', cap_block + '\nprintf "%s\\n" "${CAP_ROWS[@]}"'], env=cap_env, text=True).splitlines()
assert any(row.startswith('flea|gui|flea|flea-gui|') for row in cap_rows)
assert any(row.startswith('strata-nightly|gui|') for row in cap_rows)
assert all('$' not in row for row in cap_rows if '|gui|' in row)
targets = cap[cap.index('select_process_targets() {'):cap.index('\nowned_recorded()', cap.index('select_process_targets() {'))]
for selected, wanted in [('flea', 'flea-gui|' + str(gui)), ('strata-nightly', 'strata|')]:
    result = subprocess.check_output(['bash', '-c', targets + '\nselect_process_targets\nprintf "%s\\n" "${PROCESS_TARGETS[@]}"'], env=dict(cap_env, CAP_ONLY=selected), text=True)
    assert wanted in result.splitlines()
native_repo = root / 'native-enumeration'
helper = native_repo / 'tools/flea-ipc'
helper.parent.mkdir(parents=True)
helper.write_text('#!/bin/sh\n[ "$FLEA_IPC_KIND" = gpui ] && [ "$FLEA_IPC_HELPER" = "$0" ] || exit 2\n[ "${CAP_NATIVE_OCCUPIED:-}" != yes ] || echo "Shell ID: flea"\nexit "${CAP_NATIVE_STATUS:-0}"\n')
helper.chmod(0o755)
enumerate_native = cap[cap.index('require_flea_enumeration() {'):cap.index('\ncap_terminate()', cap.index('require_flea_enumeration() {'))]
for extra, wanted in [({}, 0), ({'CAP_NATIVE_OCCUPIED': 'yes'}, 1), ({'CAP_NATIVE_STATUS': '2'}, 1)]:
    result = subprocess.run(['bash', '-c', enumerate_native + '\nrequire_flea_enumeration'], env=dict(cap_env, FLEA_REPO=str(native_repo), **extra), capture_output=True)
    assert result.returncode == wanted, (result.returncode, result.stderr)
entry = {'fields': ['flea-tui-anchor', 'tui', 'source', 'abc123', 'v0.3.12'], 'raw': 'fixture'}
assert R.section_version('flea-tui-anchor', entry, '0.4.0') == '0.3.12, source abc123'
assert R.display_for('flea-tui-anchor', '0.3.12, source abc123') == '**Flea TUI 0.3.12**'

# B5 archives need no source checkout, and their measured identities cannot drift.
archive_rows = []
for name, version in [('strata-stable', 'strata 1.0.0'),
                      ('strata-nightly', 'strata 1.0.1-nightly.20261018'), ('xplr', 'xplr 1.1.2')]:
    binary = root / name / ('xplr' if name == 'xplr' else 'strata')
    binary.parent.mkdir()
    binary.write_text('#!/bin/sh\necho "' + version + '"\n')
    binary.chmod(0o755)
    archive_rows.append(dict(id=name, tag='v' + version.split()[1],
        url='https://example.invalid/releases/' + name + '.tar.gz', sha256='a' * 64,
        binary_path=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        source_commit='1' * 40 if name != 'xplr' else None,
        tag_commit='2' * 40 if name != 'xplr' else None,
        expected_version=version, version_check=dict(rc=0, stdout=version)))
receipt = root / 'rival-versions.json'
receipt.write_text(json.dumps(dict(archives=archive_rows, packages={})))
bindir = root / 'b5-bin'
bindir.mkdir()
(bindir / 'xplr').symlink_to(archive_rows[2]['binary_path'])
b5_env = dict(env, B5_RIVAL_MANIFEST=str(receipt), STRATA_BIN=archive_rows[0]['binary_path'],
              STRATA_NIGHTLY_BIN=archive_rows[1]['binary_path'], PATH=str(bindir) + ':' + os.environ['PATH'])
preflight = text[text.index('# archive provenance begin'):text.index('# archive provenance end')]
preflight_env = dict(b5_env, MANIFEST_TOOL=str(Path('tools/flea-bench-manifest').resolve()))
preflight_script = 'only_has() { return 0; }\nfail() { echo "$*" >&2; exit 1; }\n' + preflight
assert subprocess.run(['bash', '-c', preflight_script], env=preflight_env, capture_output=True).returncode == 0
# An unset nightly is not configured, never the current directory.
no_nightly = subprocess.run(['bash', '-c', preflight_script], env=dict(preflight_env, STRATA_NIGHTLY_BIN=''), capture_output=True, text=True)
assert no_nightly.returncode == 0, no_nightly.stderr
rows = subprocess.check_output(['bash', '-c', block + '\nbuild_entrants\nprintf "%s\\n" "${ENTRANTS[@]}"'], env=b5_env, text=True).splitlines()
archive_roster = {row.split('|')[0]: row.split('|', 7) for row in rows}
assert 'strata' not in archive_roster
for asset in archive_rows[:2]:
    assert asset['binary_path'] in archive_roster[asset['id']][7]
    arm = dict(id=asset['id'], entrant='strata', strata_bin=asset['binary_path'],
               archive_id=asset['id'], rival_manifest=str(receipt))
    checked = S.arm_identity(arm)
    assert checked['version'] == asset['expected_version']
    assert checked['archive'] == asset
    assert checked['rival_manifest']['receipt']['archives'] == archive_rows
for name, row in roster.items():
    if name != 'strata':
        assert archive_roster[name] == row if name != 'strata-nightly' else archive_rows[1]['binary_path'] in archive_roster[name][7]
fixture = root / 'archive-fixture'
fixture.mkdir()
(fixture / 'a.jpg').touch()
manifest = root / 'archive.manifest.md'
run = subprocess.run(['tools/flea-bench-manifest', str(manifest), str(fixture)],
                     input='strata-stable|gui|strata\nstrata-nightly|gui|strata\nxplr|tui|xplr\n',
                     env=dict(b5_env, FLEA_REPO=str(Path.cwd())), capture_output=True, text=True)
assert run.returncode == 0, run.stderr
text = manifest.read_text()
assert 'built locally from this checkout' not in text and 'prebuilt x86_64 binary' not in text
assert '## B5 rival receipt' in text and '1' * 40 in text and '2' * 40 in text
parsed = R.manifest_section(str(manifest))['entries']
for asset in archive_rows:
    assert parsed[asset['id']]['fields'][3] == asset['expected_version'].split()[1]
assert R.display_for('strata-stable', '1.0.0') == 'Strata stable'
# Exercise the real manifest-to-bracket path for both GUI archives and the TUI archive.
csv_path = root / 'archive.csv'
header = ('id,run,mapped_ms,settled_ms,pss_kb,cpu_s,child_pss_kb,thumbs_n,pss_anon_kb,'
          'uss_kb,drm_kb,thumbs_by_dir,cpu_tree_s,thumbs_by_format,ranked,preview_ms,'
          'cpu_reaped_s,cpu_sys_s,dirty_wb_kb,pgpgout_pre_kb,pgpgout_launch_kb,write_clean,'
          'rerun,first_rows_ms')
csv_path.write_text(header + '\n' + ''.join(
    asset['id'] + ',1,424,900,73628,0.41,0,0,1,1,1,-,0.41,-,yes,-,0.59,0.10,0,0,0,1,0,-\n'
    for asset in archive_rows))
report_tool = os.environ.get('FIELD_PRODUCT_REPORT', 'tools/flea-bench-report')
report = subprocess.run([report_tool, str(csv_path)], capture_output=True, text=True)
assert report.returncode == 0, report.stderr
for asset in archive_rows:
    version = asset['expected_version'].split()[1]
    assert '| `%s` | %s |' % (asset['id'], version) in report.stdout
    assert asset['url'] in report.stdout and asset['binary_sha256'] in report.stdout
assert manifest.read_text() == text, 'report must preserve the full archive receipt'
# Each mutation is refused by its own check: the copy at another path has the stable bytes and version.
twin = root / 'twin' / 'strata'
twin.parent.mkdir()
twin.write_bytes(Path(archive_rows[0]['binary_path']).read_bytes())
twin.chmod(0o755)
differs = 'archive binary hash or version differs from receipt'
for mutation, wanted in (('wrong-path', 'launched binary differs from archive receipt'), ('wrong-hash', differs),
                         ('wrong-version', differs), ('no-version-check', 'archive receipt missing version_check'),
                         ('bad-version-check', 'invalid archive receipt version_check'), ('changed-binary', differs)):
    asset = dict(archive_rows[0])
    binary = asset['binary_path']
    if mutation == 'wrong-path':
        binary = str(twin)
    elif mutation == 'wrong-hash':
        asset['binary_sha256'] = 'b' * 64
    elif mutation == 'wrong-version':
        asset['expected_version'] = 'strata 0.19.0'
    elif mutation == 'no-version-check':
        del asset['version_check']
    elif mutation == 'bad-version-check':
        asset['version_check'] = 'ok'
    else:
        Path(binary).write_text('#!/bin/sh\necho "strata 0.19.0"\n')
    bad_receipt = root / (mutation + '.json')
    bad_receipt.write_text(json.dumps(dict(archives=[asset])))
    try:
        S.Product.archive_identity(str(bad_receipt), asset['id'], binary)
    except ValueError as error:
        assert wanted in str(error), (mutation, error)
    else:
        raise AssertionError('accepted ' + mutation)
wrong_xplr = subprocess.run(['python3', '-B', 'tools/flea_bench_product.py', str(receipt), 'xplr', fresh], capture_output=True, text=True)
assert wrong_xplr.returncode == 1 and 'launched binary differs from archive receipt: ' + fresh in wrong_xplr.stderr, wrong_xplr.stderr
changed = subprocess.run(['bash', '-c', preflight_script], env=preflight_env, capture_output=True, text=True)
assert changed.returncode == 1 and 'archive provenance refused for strata-stable' in changed.stderr, changed.stderr
assert 'hash or version differs from receipt: strata-stable' in changed.stderr, changed.stderr
# A one-word version works as a row; a version of three words is refused by name.
for name, printed, ok in (('bare', '0.22.0', True), ('wordy', 'strata 0.22.0-nightly (abc)', False)):
    binary = root / name / 'strata'
    binary.parent.mkdir()
    binary.write_text('#!/bin/sh\necho "' + printed + '"\n')
    binary.chmod(0o755)
    asset = dict(archive_rows[0], id=name, binary_path=str(binary), expected_version=printed,
                 binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), version_check=dict(rc=0, stdout=printed))
    one = root / (name + '.json')
    one.write_text(json.dumps(dict(archives=[asset])))
    row = subprocess.run(['python3', '-B', 'tools/flea_bench_product.py', str(one), name, str(binary), '--row', 'gui'], capture_output=True, text=True)
    if ok:
        assert row.returncode == 0 and row.stdout.startswith('bare gui archive 0.22.0 {'), row
    else:
        assert row.returncode == 1 and 'archive version is not "name version"' in row.stderr, row
# Under B5 a missing xplr is named, never a silent exit.
no_xplr = ':'.join(d for d in os.environ['PATH'].split(':') if not os.path.exists(os.path.join(d, 'xplr')))
missing = subprocess.run(['tools/flea-bench-manifest', str(root / 'no-xplr.md'), str(fixture)], input='xplr|tui|xplr\n',
                         env=dict(b5_env, FLEA_REPO=str(Path.cwd()), PATH=no_xplr), capture_output=True, text=True)
assert missing.returncode == 1 and 'B5 entrant xplr is absent from PATH' in missing.stderr, missing
# A strata row with no source checkout reads absent, never the caller's own checkout.
plain_env = dict(env, FLEA_REPO=str(Path.cwd()), STRATA_SRC='', STRATA_BIN=archive_rows[1]['binary_path'])
plain_env.pop('STRATA_NIGHTLY_SRC', None)
plain = subprocess.run(['tools/flea-bench-manifest', str(root / 'no-src.md'), str(fixture)],
                       input='strata|gui|strata\nstrata-nightly|gui|strata\n', env=plain_env, capture_output=True, text=True)
assert plain.returncode == 0, plain.stderr
own = subprocess.check_output(['git', 'describe', '--tags', '--always'], text=True).strip()
for name in ('strata', 'strata-nightly'):
    line = next(l for l in (root / 'no-src.md').read_text().splitlines() if l.startswith(name + ' '))
    assert ' source absent ' in line and own not in line, line
# build_sha names the tree only for a binary built inside it.
tree = root / 'tree'
subprocess.run(['git', 'init', '-q', str(tree)], check=True)
subprocess.run(['git', '-C', str(tree), '-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '--allow-empty', '-m', 't'], check=True)
head = subprocess.check_output(['git', '-C', str(tree), 'rev-parse', 'HEAD'], text=True).strip()
built = tree / 'target/release/flea'
built.parent.mkdir(parents=True)
built.write_bytes(Path(fresh).read_bytes())
built.chmod(0o755)
for binary, wanted in ((str(built), head), (fresh, 'unknown')):
    got = S.Product.identity({'entrant': 'flea', 'flea_dir': str(tree), 'flea_bin': binary, 'frontend': 'qml'}, [binary])
    assert got['build_sha'] == wanted, (binary, got)
# A flea-gui outside the arms' trees is never a target, so the harness neither refuses on it nor ends it.
elsewhere = root / 'elsewhere' / 'flea-gui'
elsewhere.parent.mkdir()
elsewhere.write_bytes(Path('/usr/bin/sleep').read_bytes())
elsewhere.chmod(0o755)
foreign = subprocess.Popen([str(elsewhere), '30'])
try:
    S.GUI_BINARIES[:] = S.flea_gui_binaries(arms)
    assert foreign.pid not in [p for c, p in S.alive_targets() if c == 'flea-gui']
    S.GUI_BINARIES[:] = [str(elsewhere)]
    assert foreign.pid in [p for c, p in S.alive_targets() if c == 'flea-gui']
finally:
    foreign.kill()
    foreign.wait()
    S.GUI_BINARIES[:] = []
# An entrant with no working --version (pcmanfm 1.4.0 exits 1) reports its package version.
pkgbin = root / 'pkgbin'
pkgbin.mkdir()
for name, body in [('noversion', '#!/bin/sh\necho "Unknown option --version"\nexit 1\n'),
                   ('silent', '#!/bin/sh\nexit 0\n'),
                   ('pacman', '#!/bin/sh\ncase "$1" in\n-Qqo) [ "${2##*/}" = unowned ] && exit 1; echo fakepkg;;\n-Q) echo "$2 9.9-1";;\nesac\n')]:
    (pkgbin / name).write_text(body)
    (pkgbin / name).chmod(0o755)
(pkgbin / 'unowned').write_text('#!/bin/sh\nexit 1\n')
(pkgbin / 'unowned').chmod(0o755)
with patch.dict(os.environ, {'PATH': str(pkgbin) + ':' + os.environ['PATH']}):
    for name in ('noversion', 'silent'):
        got = S.Product.identity({'entrant': name}, [name])
        assert got['version'] == 'fakepkg 9.9-1' and got['version_source'] == 'pacman', got
        assert got['sha256'] == hashlib.sha256((pkgbin / name).read_bytes()).hexdigest()
    try:
        S.Product.identity({'entrant': 'unowned'}, ['unowned'])
    except RuntimeError as failure:
        assert 'no version for' in str(failure) and 'package fallback' in str(failure)
    else:
        raise AssertionError('accepted a binary with neither --version nor a package')
assert identities['fresh']['version_source'] == '--version'
print('field-product: all passed')
PY
