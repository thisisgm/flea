#!/bin/bash
# Checks product launches, frontend isolation, identities, build binding, manifest close and eligibility without a display.
set -eu
cd "$(dirname "$0")/.."
scratch=$(mktemp -d)
: > "$scratch/.flea-test-sandbox"
trap 'case $scratch in /*/*) [ -n "$scratch" ] && [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
# The packaged interface on the host (minipc has one) must not decide which tree a fixture build binds.
export FLEA_BENCH_PACKAGED_UI="$scratch/no-packaged-ui"
python3 -B - "$scratch" <<'PY'
import hashlib
import json
import os
import socket
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

root = Path(sys.argv[1])
sys.path.insert(0, str(Path('tools').resolve()))
from flea_import import load_source

S = load_source('product_search', 'tools/flea-field-search')
N = load_source('product_nav', 'tools/flea-field-nav')
I = load_source('product_instant', 'tools/flea-field-instant')
R = load_source('product_readme', 'tools/flea-bench-readme')
S.ENV = dict(os.environ)
S.STATE_ROOT = str(root / 'harness-state')

def stub(name, version):
    path = root / name / 'bin' / 'flea'
    path.parent.mkdir(parents=True)
    path.write_text('#!/bin/sh\ncase "$1" in\n--version) echo "flea ' + version + '";;\n--ui-state) echo \'{"view":"grid"}\';;\nesac\n')
    path.chmod(0o755)
    return str(path)

fresh, old = stub('candidate', '0.4.0'), stub('anchor', '0.3.12')
# Both stubs resolve the interface files their tree would ship, which a QML build requires.
(root / 'ui/boot').mkdir(parents=True)
(root / 'ui/boot/shell.qml').write_text('import QtQuick\n')
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
        assert S.entrant_spec(arm)['cls'] == S.FLEA_CLASS
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
    assert I.external_clients([{'class': 'com.thisisgm.flea', 'pid': 7}, {'class': 'com.thisisgm.flea', 'pid': 8}], {7}) == [{'class': 'com.thisisgm.flea', 'pid': 8}]

# The actual list adapter must discover the owned native instance, not invoke qs.
proc = type('Process', (), {'pid': os.getpid()})()
with patch.dict(os.environ, {}, clear=True):
    assert I.Client(fresh).kind == 'gpui'
    assert I.Client('qs').kind == 'qml'
with patch.dict(os.environ, {'FLEA_FRONTEND': ''}, clear=True):
    assert I.Client(fresh).kind == 'gpui'
    assert I.Client('qs').kind == 'qml'
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
assert roster['flea'][2:5] == ['com.thisisgm.flea', 'flea-gui', str(gui)]
assert 'FLEA_FRONTEND=gpui' in roster['flea'][7] and ' --gui ' in roster['flea'][7]
assert 'FLEA_FRONTEND=qml' in roster['flea-anchor'][7]
assert old + ' --tui ' in roster['flea-tui-anchor'][7]
assert old in roster['strata-nightly'][7]
cap = Path('tools/flea-bench-capability').read_text()
cap_block = cap[cap.index('# capability roster begin'):cap.index('# capability roster end')]
cap_env = dict(env, FLEA_REPO=str(Path.cwd()))
cap_rows = subprocess.check_output(['bash', '-c', cap_block + '\nprintf "%s\\n" "${CAP_ROWS[@]}"'], env=cap_env, text=True).splitlines()
assert any(row.startswith('flea|gui|com.thisisgm.flea|flea-gui|') for row in cap_rows)
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
        source_commit='1' * 40, tag_commit='2' * 40,
        expected_version=version, version_check=dict(rc=0, stdout=version)))
receipt = root / 'rival-versions.json'
# A B5 receipt is bound to the host and kernel that measured it; the B2 gate refuses any other.
receipt.write_text(json.dumps(dict(archives=archive_rows, packages={}, host=socket.gethostname(), kernel=os.uname().release)))
bindir = root / 'b5-bin'
bindir.mkdir()
(bindir / 'xplr').symlink_to(archive_rows[2]['binary_path'])
b5_env = dict(env, B5_RIVAL_MANIFEST=str(receipt), FLEA_RIVAL_MANIFEST=str(receipt), STRATA_BIN=archive_rows[0]['binary_path'],
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
archive_bins = {asset['id']: asset['binary_path'] for asset in archive_rows[:2]}
for name, row in roster.items():
    if name != 'strata':
        assert archive_roster[name] == row if name not in archive_bins else archive_bins[name] in archive_roster[name][7]
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
receipt_block = text.split('## B5 rival receipt\n', 1)[1].split('```json\n', 1)[1].split('\n```', 1)[0]
assert json.loads(receipt_block) == json.loads(receipt.read_text()), 'receipt fence must close on its own line'
complete_receipt = text.split('- complete rival receipt:', 1)[1].split('```json\n', 1)[1].split('\n```', 1)[0]
assert json.loads(complete_receipt) == json.loads(receipt.read_text()), 'complete receipt fence must close on its own line'
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
# Each mutation keeps both channels and reaches its own check; the twin has the stable bytes and version.
twin = root / 'twin' / 'strata'
twin.parent.mkdir()
twin.write_bytes(Path(archive_rows[0]['binary_path']).read_bytes())
twin.chmod(0o755)
differs = 'archive binary hash or version differs from receipt'
for mutation, wanted in (('wrong-path', 'launched binary differs from archive receipt'), ('wrong-hash', differs),
                         ('wrong-version', differs), ('no-version-check', 'archive receipt missing version_check'),
                         ('bad-version-check', 'invalid archive receipt version_check'), ('changed-binary', differs)):
    rows = [dict(row) for row in archive_rows]
    asset = rows[0]
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
    bad_receipt.write_text(json.dumps(dict(archives=rows)))
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
Path(archive_rows[0]['binary_path']).write_bytes(twin.read_bytes())
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
# Under B5 a missing xplr is named: each PATH dir holding xplr becomes a scratch dir of links to its other entries.
no_xplr = []
for n, d in enumerate(os.environ['PATH'].split(':')):
    if os.path.exists(os.path.join(d, 'xplr')):
        shim = root / ('no-xplr-%d' % n)
        shim.mkdir()
        for entry in os.listdir(d):
            if entry != 'xplr':
                (shim / entry).symlink_to(os.path.join(d, entry))
        d = str(shim)
    no_xplr.append(d)
no_xplr = ':'.join(no_xplr)
missing = subprocess.run(['tools/flea-bench-manifest', str(root / 'no-xplr.md'), str(fixture)], input='xplr|tui|xplr\n',
                         env=dict(b5_env, FLEA_REPO=str(Path.cwd()), PATH=no_xplr), capture_output=True, text=True)
assert missing.returncode == 1 and 'B5 entrant xplr is absent from PATH' in missing.stderr, missing
# A strata row with no source checkout reads absent, never the caller's own checkout.
plain_env = dict(env, FLEA_REPO=str(Path.cwd()), STRATA_SRC='', STRATA_BIN=archive_rows[1]['binary_path'])
plain_env.pop('STRATA_NIGHTLY_SRC', None)
plain = subprocess.run(['tools/flea-bench-manifest', str(root / 'no-src.md'), str(fixture)],
                       input='strata|gui|strata\nstrata-nightly|gui|strata\n', env=plain_env, capture_output=True, text=True)
assert plain.returncode == 0, plain.stderr
# A tree without git metadata (the CI archive) has no describe string for the row to leak; inside a checkout describe must answer.
inside = Path('.git').exists()
own = subprocess.check_output(['git', 'describe', '--tags', '--always'], text=True).strip() if inside else ''
for name in ('strata', 'strata-nightly'):
    line = next(l for l in (root / 'no-src.md').read_text().splitlines() if l.startswith(name + ' '))
    assert ' source absent ' in line and (not own or own not in line), line
    assert 'built locally from this checkout' not in line, line
# A strata row whose binary is unset reads "binary absent", never a shell error.
for name, var in (('strata', 'STRATA_BIN'), ('strata-nightly', 'STRATA_NIGHTLY_BIN')):
    unset_env = {k: v for k, v in plain_env.items() if k != var}
    unset = subprocess.run(['tools/flea-bench-manifest', str(root / ('unset-' + name + '.md')), str(fixture)],
                           input='strata|gui|strata\nstrata-nightly|gui|strata\n',
                           env=dict(unset_env, LAUNCH_ORDER='run 1: strata@- strata-nightly@-'), capture_output=True, text=True)
    assert unset.returncode == 0, unset.stderr
    line = next(l for l in (root / ('unset-' + name + '.md')).read_text().splitlines() if l.startswith(name + ' '))
    assert line.endswith('; binary absent') and 'not found' not in line + unset.stderr, (line, unset.stderr)
line = next(l for l in (root / 'no-src.md').read_text().splitlines() if l.startswith('strata '))
assert '; binary reports ' in line, line
# A qml flea arm with only flea_bin is refused before anything runs: its qs could never be matched or ended.
plan = root / 'qml-bin-only.json'
plan.write_text(json.dumps(dict(out=str(root / 'qml-bin-only.jsonl'), arms=[dict(id='bare', entrant='flea', flea_bin=fresh, frontend='qml')])))
with patch.object(S, 'ensure_scope', side_effect=AssertionError('measure reached ensure_scope')):
    try:
        S.measure(str(plan))
    except SystemExit as refusal:
        assert 'measure refuses arm bare: a qml flea arm needs flea_dir' in str(refusal), refusal
    else:
        raise AssertionError('measure accepted a qml arm with flea_bin alone')
# build_sha names the tree only for a binary built inside it.
tree = root / 'tree'
subprocess.run(['git', 'init', '-q', str(tree)], check=True)
subprocess.run(['git', '-C', str(tree), '-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '--allow-empty', '-m', 't'], check=True)
head = subprocess.check_output(['git', '-C', str(tree), 'rev-parse', 'HEAD'], text=True).strip()
built = tree / 'target/release/flea'
built.parent.mkdir(parents=True)
built.write_bytes(Path(fresh).read_bytes())
built.chmod(0o755)
(tree / 'ui/boot').mkdir(parents=True)
(tree / 'ui/boot/shell.qml').write_text('import QtQuick\n')
for binary, wanted in ((str(built), head), (fresh, 'unknown')):
    got = S.Product.identity({'entrant': 'flea', 'flea_dir': str(tree), 'flea_bin': binary, 'frontend': 'qml'}, [binary])
    assert got['build_sha'] == wanted, (binary, got)
    assert got['build']['source']['commit'] == wanted, (binary, got)
    if wanted == 'unknown':
        assert got['build']['source']['state'] == 'unknown', got
external = root / 'external-flea.md'
run = subprocess.run(['tools/flea-bench-manifest', str(external), str(fixture)],
                     input='flea|gui|flea\nflea-anchor|gui|flea\n',
                     env=dict(env, FLEA_REPO=str(tree), ANCHOR_REPO=str(tree), FLEA_FRONTEND='qml',
                              FLEA_UI=str(tree / 'ui'), ANCHOR_UI=str(tree / 'ui')), capture_output=True, text=True)
assert run.returncode == 0, run.stderr
for line in external.read_text().splitlines():
    if line.startswith(('flea ', 'flea-anchor ')):
        assert ' source unknown ' in line and head[:7] not in line, line
external_builds = external.read_text().split('## Flea build identities', 1)[1].split('```json\n', 1)[1].split('```', 1)[0]
for line in external_builds.splitlines():
    assert json.loads(line)['source'] == {'commit': 'unknown', 'state': 'unknown'}, line
selection = Path('tools/flea-field-bench').read_text().split('export FLEA_UI=', 1)[1].split('# manifest selection begin', 1)[0]
selection = 'export FLEA_UI=' + selection
for candidate, anchor, ok in ((str(built), '', True), (fresh, '', False), (str(built), fresh, False)):
    selected = subprocess.run(['bash', '-c', selection], env=dict(env, FLEA_REPO=str(tree), FLEA_BIN=candidate,
                              ANCHOR_REPO=str(root / 'anchor'), ANCHOR_BIN=anchor), capture_output=True, text=True)
    assert (selected.returncode == 0) == ok, (candidate, anchor, selected.returncode, selected.stderr)
    if not ok:
        assert 'is outside its source tree' in selected.stderr, selected.stderr
# Census registers the selected native binary before the real idle guard can refuse a leftover.
(root / 'fixtures.json').write_text(json.dumps({'root': str(root)}))
class IdleReached(Exception):
    pass
for chosen in (None, fresh):
    expected_gui = str(Path(chosen or str(tree / 'target/release/flea')).with_name('flea-gui'))
    def idle():
        assert I.S.GUI_BINARIES == [expected_gui], I.S.GUI_BINARIES
        raise IdleReached()
    with patch.object(I, 'parts', return_value={'probe': []}), patch.object(I.S, 'session_env', return_value={}), \
         patch.object(I.S, 'require_idle', side_effect=idle):
        try:
            I.run_part('probe', str(root), str(tree), str(root / 'census'), frontend='gpui', binary=chosen)
        except IdleReached:
            pass
        else:
            raise AssertionError('census never checked idle')
    I.S.GUI_BINARIES[:] = []
# A ledger failure refuses before the harness claims the machine or installs cleanup hooks.
bad_parent = root / 'not-a-directory'
bad_parent.touch()
with patch.object(S, 'LEDGER', str(bad_parent / 'runs.jsonl')), patch.object(S, 'alive_targets', return_value=[]), \
     patch.object(S.atexit, 'register') as register:
    try:
        S.require_idle()
    except SystemExit as refusal:
        assert 'cannot record harness run' in str(refusal) and str(bad_parent) in str(refusal), refusal
    else:
        raise AssertionError('a run with an unwritable ledger started')
    assert not S._RUN and not register.called, S._RUN
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
    # A running copy whose file was replaced reads "<path> (deleted)" and is still a target.
    elsewhere.unlink()
    elsewhere.write_bytes(Path('/usr/bin/sleep').read_bytes())
    assert os.readlink('/proc/%d/exe' % foreign.pid).endswith(' (deleted)')
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
print('field-product: launches and archives passed')
PY
mkdir "$scratch/own"
python3 -B - "$scratch/own" <<'PY'
import hashlib
import json
import os
import socket
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

root = Path(sys.argv[1])
sys.path.insert(0, str(Path('tools').resolve()))
from flea_import import load_source

P = load_source('bench_product', 'tools/flea_bench_product.py')
MANIFEST = str(Path('tools/flea-bench-manifest').resolve())
PIN = 'ac631dc8b9ef612b875edcc5dd9cfa5870dc77cc'
GIT = ['git', '-c', 'user.name=t', '-c', 'user.email=t@t', '-c', 'init.defaultBranch=main']


def refused(label, call, *args, **kwargs):
    try:
        call(*args, **kwargs)
    except (ValueError, RuntimeError, OSError) as error:
        return str(error)
    raise AssertionError('accepted ' + label)


def build(name, version, gui=None, ui=True, extra=''):
    """A source-tree build: bin/flea, an optional sibling, and ui/boot/shell.qml one level above."""
    tree = root / name
    binary = tree / 'target/release/flea'
    binary.parent.mkdir(parents=True)
    binary.write_text('#!/bin/sh\n# rustc version 1.92.0 (fixture 2026-10-01)\n' + extra
                      + 'case "$1" in --version) echo "' + version + '";; esac\n')
    binary.chmod(0o755)
    if gui is not None:
        sibling = binary.with_name('flea-gui')
        sibling.write_bytes(gui)
        sibling.chmod(0o755)
    if ui:
        (tree / 'ui/boot').mkdir(parents=True)
        (tree / 'ui/boot/shell.qml').write_text('import QtQuick\n')
    (tree / 'tools').mkdir()
    (tree / 'tools/flea-udisks').write_text('helper\n')
    (tree / 'Cargo.lock').write_text('source = "git+https://github.com/thisisgm/gpui-omarchy?rev=%s#%s"\n' % (PIN, PIN))
    subprocess.run(GIT + ['init', '-q', str(tree)], check=True)
    subprocess.run(GIT + ['-C', str(tree), 'add', '-A'], check=True)
    subprocess.run(GIT + ['-C', str(tree), 'commit', '-qm', 'fixture'], check=True)
    return tree, binary


checkout = b'/cargo/git/checkouts/gpui-omarchy-0123456789abcdef/' + PIN[:7].encode() + b'/src/lib.rs'
cand_tree, cand = build('candidate', '0.4.0', gui=b'native product ' + checkout)
anchor_tree, anchor = build('anchor', '0.3.12')

# Front end: GPUI is the default candidate, QML the anchor, anything else refused.
assert P.frontend({}) == 'gpui' and P.frontend({'frontend': 'qml'}) == 'qml'
refused('a third front end', P.frontend, {'frontend': 'gtk'})
assert P.window_class({'frontend': 'gpui'}) == 'com.thisisgm.flea' and P.window_class({'frontend': 'qml'}) == 'com.thisisgm.flea'

# A whole build is bound: launcher, sibling, interface files, helpers, source, compiler, seams, fork pin.
got = P.build_identity(str(cand), 'gpui', str(cand_tree))
assert got['launcher']['version'] == '0.4.0' and got['launcher']['sha256'] == hashlib.sha256(cand.read_bytes()).hexdigest()
assert got['sibling']['sha256'] == hashlib.sha256(cand.with_name('flea-gui').read_bytes()).hexdigest()
assert got['ui']['files'] == 1 and got['ui']['path'] == str((cand_tree / 'ui').resolve())
assert got['helpers']['flea-udisks'] == hashlib.sha256(b'helper\n').hexdigest() and got['helpers']['flea-mpris'] == 'absent'
assert got['compiler'] == ['rustc version 1.92.0 (fixture 2026-10-01)']
assert got['seams'] == [] and got['fork_pin'] == {'executed': [PIN[:7]], 'locked': PIN}
assert got['source']['state'] == 'clean' and len(got['source']['commit']) == 40
anchor_got = P.build_identity(str(anchor), 'qml', str(anchor_tree))
assert anchor_got['sibling'] == 'absent' and anchor_got['launcher']['version'] == '0.3.12'
# A changed interface file changes the resource digest, and an override directory wins over the tree.
before = anchor_got['ui']['sha256']
(anchor_tree / 'ui/boot/shell.qml').write_text('import QtQuick 2\n')
assert P.build_identity(str(anchor), 'qml')['ui']['sha256'] != before
assert P.build_identity(str(anchor), 'qml', str(anchor_tree))['source']['state'] == 'DIRTY'
override = root / 'override-ui'
(override / 'boot').mkdir(parents=True)
(override / 'boot/shell.qml').write_text('override\n')
assert P.build_identity(str(anchor), 'qml', ui=str(override))['ui']['path'] == str(override.resolve())

# Companion and resource controls: each missing part refuses the build.
_, lone = build('no-sibling', '0.4.0')
assert 'sibling' in refused('GPUI without flea-gui', P.build_identity, str(lone), 'gpui')
_, bare = build('no-ui', '0.3.12', ui=False)
assert 'interface files' in refused('QML without ui', P.build_identity, str(bare), 'qml')
refused('a missing launcher', P.build_identity, str(root / 'absent/flea'), 'gpui')
seamed_tree, seamed = build('seamed', '0.4.0', gui=b'FLEA_GUI_BENCH_VIEWPORT ' + checkout)
assert 'bench-seam' in refused('a bench seam build', P.build_identity, str(seamed), 'gpui')
assert P.build_identity(str(seamed), 'gpui', seams_allowed=True)['seams'] == ['bench-seam']
_, flashed = build('flashed', '0.4.0', gui=checkout, extra='# --loop-root\n')
assert 'flash-test-seam' in refused('a flash test seam build', P.build_identity, str(flashed), 'gpui')
# The product reads the packaged interface before the tree's for any binary, so the identity must too.
packaged = root / 'packaged-ui'
(packaged / 'boot').mkdir(parents=True)
(packaged / 'boot/shell.qml').write_text('packaged\n')
with patch.object(P, 'PACKAGED_UI', str(packaged), create=True):
    assert P.build_identity(str(anchor), 'qml')['ui']['path'] == str(packaged.resolve())
    assert P.build_identity(str(anchor), 'qml', ui=str(override))['ui']['path'] == str(override.resolve())
    # A field arm launches with FLEA_UI=<flea_dir>/ui, so its identity binds that tree over the package.
    tree_arm = {'id': 'a', 'entrant': 'flea', 'frontend': 'qml', 'flea_dir': str(anchor_tree)}
    assert P.identity(tree_arm, [str(anchor)])['build']['ui']['path'] == str((anchor_tree / 'ui').resolve())
assert P.resolve_ui(str(anchor), str(root / 'no-such-ui')) is not None
drift_tree, drift = build('drift', '0.4.0', gui=checkout.replace(PIN[:7].encode(), b'deadbee'))
assert 'differs from the locked pin' in refused('a drifted fork pin', P.build_identity, str(drift), 'gpui', str(drift_tree))
assert 'differs from the locked pin' in refused('an external build with a drifted fork pin', P.build_identity, str(drift), 'gpui', str(cand_tree))
(drift_tree / 'Cargo.lock').write_text((drift_tree / 'Cargo.lock').read_text() + 'source = "git+https://github.com/thisisgm/gpui-omarchy?rev=%s#%s"\n' % ('b' * 40, 'b' * 40))
assert 'exactly one' in refused('two fork pins', P.locked_fork_pins, str(drift_tree))

# A Flea arm's identity carries its build, and a candidate's GUI hash is its sibling's.
arm = {'id': 'flea', 'entrant': 'flea', 'flea_bin': str(cand), 'frontend': 'gpui', 'flea_dir': str(cand_tree)}
ident = P.identity(arm, [str(cand)])
assert ident['version'] == '0.4.0' and ident['gui_sha256'] == got['sibling']['sha256'] and ident['build']['frontend'] == 'gpui'
assert ident['build_sha'] == got['source']['commit']
refused('an anchor arm without ui', P.identity, {'id': 'a', 'entrant': 'flea', 'frontend': 'qml'}, [str(bare)])

# B5 archives: both Strata channels are mandatory, distinct and verified against the executed bytes.
archive_rows = []
for name, version in [('strata-stable', 'strata 1.1.0'),
                      ('strata-nightly', 'strata 1.0.1-nightly.20261018'), ('xplr', 'xplr 1.1.2')]:
    binary = root / name / ('xplr' if name == 'xplr' else 'strata')
    binary.parent.mkdir()
    binary.write_text('#!/bin/sh\necho "' + version + '"\n')
    binary.chmod(0o755)
    archive_rows.append(dict(id=name, tag='v' + version.split()[1],
        url='https://example.invalid/releases/' + name + '.tar.gz', sha256='a' * 64,
        binary_path=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        source_commit='1' * 40, tag_commit='2' * 40,
        expected_version=version, version_check=dict(rc=0, stdout=version)))
receipt = root / 'rival-versions.json'
# A B5 receipt is bound to the host and kernel that measured it; the B2 gate refuses any other.
receipt.write_text(json.dumps(dict(archives=archive_rows, packages={}, host=socket.gethostname(), kernel=os.uname().release)))
for asset in archive_rows:
    checked = P.archive_identity(str(receipt), asset['id'], asset['binary_path'])
    assert checked['version'] == asset['expected_version'] and checked['archive'] == asset
stable, nightly = archive_rows[0], archive_rows[1]
copy = root / 'stable-copy' / 'strata'
copy.parent.mkdir()
copy.write_bytes(Path(stable['binary_path']).read_bytes())
copy.chmod(0o755)
# Each mutation names the one check that must refuse it, so a deleted check cannot hide behind another.
mutations = {
    'wrong path': (lambda rows: rows[0].update(binary_path=str(copy)), 'launched binary differs'),
    'aliased channels': (lambda rows: rows[0].update(binary_path=nightly['binary_path']), 'alias the same executable'),
    'wrong hash': (lambda rows: rows[0].update(binary_sha256='b' * 64), 'hash or version differs'),
    'wrong version': (lambda rows: rows[0].update(expected_version='strata 0.19.0'), 'hash or version differs'),
    'short commit': (lambda rows: rows[0].update(source_commit='1' * 7), 'invalid archive source identity'),
    'plain URL': (lambda rows: rows[0].update(url='http://example.invalid/a.tar.gz'), 'actual ref and artifact URL'),
    'missing nightly': (lambda rows: rows.pop(1), 'mandatory'),
    'nightly as stable': (lambda rows: rows[0].update(expected_version='strata 1.0.1-nightly.1'), 'version channels differ'),
    'shared tag': (lambda rows: rows[1].update(tag=rows[0]['tag']), 'distinct tag'),
}
for label, (mutate, wanted) in mutations.items():
    rows = [dict(row) for row in archive_rows]
    mutate(rows)
    bad = root / ('bad-' + label.replace(' ', '-') + '.json')
    bad.write_text(json.dumps(dict(archives=rows)))
    launched = stable['binary_path'] if label == 'wrong path' else rows[0]['binary_path']
    message = refused(label, P.archive_identity, str(bad), 'strata-stable', launched)
    assert wanted in message, (label, message)
assert subprocess.run(['python3', '-B', 'tools/flea_bench_product.py', str(receipt), 'xplr', str(cand)], capture_output=True).returncode != 0

# The manifest: one front end, both Strata rows wherever Strata runs, a bound build per Flea row.
fixture = root / 'fixture'
fixture.mkdir()
(fixture / 'a.jpg').touch()
bindir = root / 'b5-bin'
bindir.mkdir()
(bindir / 'xplr').symlink_to(archive_rows[2]['binary_path'])
base_env = dict(os.environ, FLEA_REPO=str(cand_tree), FLEA_BIN=str(cand), ANCHOR_REPO=str(anchor_tree),
                ANCHOR_BIN=str(anchor), STRATA_BIN=stable['binary_path'], STRATA_NIGHTLY_BIN=nightly['binary_path'],
                PATH=str(bindir) + ':' + os.environ['PATH'], MANIFEST_TOOL=MANIFEST)
for key in ('FIELD_FRONTEND', 'FLEA_FRONTEND', 'B5_RIVAL_MANIFEST', 'FLEA_RIVAL_MANIFEST', 'FLEA_UI', 'ANCHOR_UI'):
    base_env.pop(key, None)
FULL = 'flea|gui|flea-gui\nflea-anchor|gui|qs\nflea-tui-anchor|tui|flea\nstrata|gui|strata\nstrata-nightly|gui|strata\nxplr|tui|xplr\n'


def manifest(name, rows=FULL, **extra):
    out = root / (name + '.manifest.md')
    run = subprocess.run([MANIFEST, str(out), str(fixture)], input=rows, env=dict(base_env, **extra),
                         capture_output=True, text=True)
    return run, out


run, good = manifest('good')
assert run.returncode == 0, run.stderr
text = good.read_text()
assert '- front end: gpui;' in text and '## Flea build identities' in text
blocks = [json.loads(line) for line in text.split('## Flea build identities', 1)[1].split('```json\n', 1)[1].split('```', 1)[0].splitlines()]
assert [b['frontend'] for b in blocks] == ['gpui', 'qml'], 'the TUI anchor row binds no GUI build'
assert blocks[0]['sibling']['sha256'] == got['sibling']['sha256'] and blocks[1]['launcher']['version'] == '0.3.12'
for key in ('gpu', 'display', 'theme', 'fonts', 'decoders', 'thumbnailers', 'storage', 'cache', 'accounting'):
    assert ('\n- %s: ' % key) in text, key
assert '- Strata stable: ' + stable['binary_path'] in text and '- Strata nightly: ' + nightly['binary_path'] in text
run, archived = manifest('archive', 'strata-stable|gui|strata\nstrata-nightly|gui|strata\nxplr|tui|xplr\n',
                         B5_RIVAL_MANIFEST=str(receipt), FLEA_RIVAL_MANIFEST=str(receipt))
assert run.returncode == 0, run.stderr
assert '## B5 rival receipt' in archived.read_text() and 'built locally from this checkout' not in archived.read_text()
assert 'strata-nightly gui archive 1.0.1-nightly.20261018' in archived.read_text()
assert manifest('no-strata', 'flea|gui|flea-gui\n', STRATA_BIN='', STRATA_NIGHTLY_BIN='')[0].returncode == 0
controls = {
    'a stable Strata row alone': dict(rows='strata|gui|strata\n'),
    'a nightly Strata row alone': dict(rows='strata-nightly|gui|strata\n'),
    'a Strata roster with no nightly binary': dict(STRATA_NIGHTLY_BIN=''),
    'disagreeing front ends': dict(FIELD_FRONTEND='gpui', FLEA_FRONTEND='qml'),
    'a third front end': dict(FIELD_FRONTEND='gtk'),
    'a GPUI candidate without its sibling': dict(FLEA_BIN=str(lone), FLEA_REPO=str(lone.parents[2])),
    'a QML anchor without interface files': dict(ANCHOR_BIN=str(bare), ANCHOR_REPO=str(bare.parents[2])),

    'an archive that differs from its receipt': dict(B5_RIVAL_MANIFEST=str(root / 'bad-wrong-hash.json')),
}
for label, extra in controls.items():
    rows = extra.pop('rows', FULL)
    assert manifest('control', rows, **extra)[0].returncode != 0, 'accepted ' + label
# A third front end is refused by the manifest itself, even on a roster with no Flea row to refuse it.
gtk_run = manifest('gtk', 'xplr|tui|xplr\n', FIELD_FRONTEND='gtk')[0]
assert gtk_run.returncode != 0 and 'front end must be gpui or qml' in gtk_run.stderr, gtk_run.stderr
# The seam control differs from a passing run only in its build, and fails for the seam alone.
seam_run = manifest('seam', FLEA_BIN=str(seamed), FLEA_REPO=str(seamed_tree))[0]
assert seam_run.returncode != 0 and 'bench-seam' in seam_run.stderr, seam_run.stderr
# A source-built Strata row records the binary that ran under every id the field bench uses.
run, sourced = manifest('strata-ids', 'strata-stable|gui|strata\nstrata-nightly|gui|strata\nstrata-nightly-channel|gui|strata\n',
                        STRATA_NIGHTLY_CHANNEL_BIN=nightly['binary_path'])
assert run.returncode == 0, run.stderr
rows = {line.split()[0]: line for line in sourced.read_text().splitlines() if line.startswith('strata')}
assert 'binary reports strata 1.1.0' in rows['strata-stable'], rows
assert 'binary reports strata 1.0.1-nightly' in rows['strata-nightly'] and 'binary reports strata 1.0.1-nightly' in rows['strata-nightly-channel'], rows
assert all(' source absent ' in line for line in rows.values()), 'a Strata row with no source tree described another checkout'
# Strata is required where it runs: the launch order says what runs, and the roster alone does not.
assert manifest('strata-scoped', LAUNCH_ORDER='run 1: flea@1 xplr@1', STRATA_BIN='', STRATA_NIGHTLY_BIN='')[0].returncode == 0
# The field bench writes an absent row as id@-, which launches nothing.
assert manifest('strata-absent', LAUNCH_ORDER='run 1: flea@1 strata@- strata-nightly@- xplr@1\nrun 2: xplr@1 strata@- flea@1',
                STRATA_BIN='', STRATA_NIGHTLY_BIN='')[0].returncode == 0
half_run = manifest('strata-half', LAUNCH_ORDER='run 1: flea@1 strata@2 strata-nightly@- xplr@1', STRATA_NIGHTLY_BIN='')[0]
assert half_run.returncode != 0 and 'both the stable and the nightly row' in half_run.stderr, half_run.stderr
lone_run = manifest('strata-lone', LAUNCH_ORDER='run 1: flea@1 strata@2 xplr@1', STRATA_NIGHTLY_BIN='')[0]
assert lone_run.returncode != 0 and 'both the stable and the nightly row' in lone_run.stderr, lone_run.stderr
assert manifest('strata-both', LAUNCH_ORDER='run 1: strata-nightly@1 strata@2 flea@3')[0].returncode == 0

# Close and verify: only one complete close over the exact CSV bytes passes.
csv = root / 'good.csv'
csv.write_text('id,run,thumbs_n,thumbs_by_format,ranked,preview_ms\nflea,1,10,jpg=1,yes,-\nstrata,1,12,jpg=1,yes,-\n')


def verify(out, data=csv):
    return subprocess.run([MANIFEST, str(out), 'verify', str(data)], env=base_env, capture_output=True).returncode


assert verify(good) != 0, 'an open manifest verified'
close = subprocess.run([MANIFEST, str(good), 'close', str(csv)], env=base_env, capture_output=True, text=True)
assert close.returncode == 0, close.stderr
assert good.read_text().rstrip('\n').endswith('- close complete: yes') and verify(good) == 0
changed = root / 'changed.csv'
changed.write_text(csv.read_text().replace('10', '11'))
assert verify(good, changed) != 0, 'a changed CSV verified'
truncated = root / 'truncated.manifest.md'
truncated.write_text(good.read_text().rsplit('- close complete: yes', 1)[0])
assert verify(truncated) != 0, 'a close without its completion marker verified'
doubled = root / 'doubled.manifest.md'
doubled.write_text(good.read_text() + '\n## Run close\n- close complete: yes\n')
assert verify(doubled) != 0, 'a second close verified'
assert subprocess.run([MANIFEST, str(good), 'close', str(csv)], env=base_env, capture_output=True).returncode != 0
columns = csv.read_text().splitlines()[0].split(',')
for drop in columns:
    _, short = manifest('short')
    narrow = root / 'narrow.csv'
    kept = [i for i, name in enumerate(columns) if name != drop]
    narrow.write_text(''.join(','.join(line.split(',')[i] for i in kept) + '\n' for line in csv.read_text().splitlines()))
    failed = subprocess.run([MANIFEST, str(short), 'close', str(narrow)], env=base_env, capture_output=True)
    assert failed.returncode != 0 and 'close complete' not in short.read_text() and verify(short, narrow) != 0, drop

# Historical rows: equivalent context only, whole-cgroup CPU only, never a relabelled version.
assert P.row_eligibility(good, good, 'flea', 'flea 0.4.0', 'flea 0.4.0')['eligible']
old = root / 'old.manifest.md'
old.write_text(good.read_text().replace('- cpu_s: complete run cgroup plus an external helper', '- cpu_s: main process ticks'))
verdict = P.row_eligibility(old, good, 'strata', 'strata 1.0.0', 'strata 1.1.0')
assert not verdict['eligible'] and any('main-process CPU' in r for r in verdict['refusals']) and any('relabelled' in r for r in verdict['refusals'])
moved = root / 'moved.manifest.md'
moved.write_text(good.read_text().replace('\n- storage: ', '\n- storage: elsewhere '))
assert any(r.startswith('storage:') for r in P.row_eligibility(moved, good, 'flea', 'v', 'v')['refusals'])
qml_run = root / 'qml.manifest.md'
qml_run.write_text(good.read_text().replace('- front end: gpui;', '- front end: qml;'))
assert any(r.startswith('front end:') for r in P.row_eligibility(qml_run, good, 'flea', 'v', 'v')['refusals'])
assert any(r.startswith('front end:') for r in P.row_eligibility(qml_run, good, 'flea-anchor', 'v', 'v')['refusals'])
assert P.row_eligibility(qml_run, good, 'strata', 'v', 'v')['eligible'], 'a rival row does not depend on the Flea front end'
engine_run = root / 'engine.manifest.md'
engine_run.write_text(good.read_text().replace('; profile: field', '; profile: engine'))
assert any(r.startswith('profile:') for r in P.row_eligibility(engine_run, good, 'strata', 'v', 'v')['refusals'])
counted = root / 'counted.manifest.md'
counted.write_text(good.read_text().replace('\n- accounting: ', '\n- accounting: main process only, '))
assert any(r.startswith('accounting:') for r in P.row_eligibility(counted, good, 'strata', 'v', 'v')['refusals'])
cli = ['python3', '-B', 'tools/flea_bench_product.py', 'eligible']
assert subprocess.run(cli + [str(good), str(good), 'flea', 'v', 'v'], capture_output=True).returncode == 0
assert subprocess.run(cli + [str(old), str(good), 'flea', 'v', 'v'], capture_output=True).returncode == 1
assert json.loads(subprocess.check_output(['python3', '-B', 'tools/flea_bench_product.py', 'build', str(cand), 'gpui']))['seams'] == []
print('field-product: all passed')
PY
