#!/usr/bin/env bash
# Exercises the real report consumer with complete synthetic captures and missing, foreign or stale replies.
set -euo pipefail
if [[ ${1:-} != --isolated ]]; then
    repo=$(cd "$(dirname "$0")/.." && pwd -P)
    mkdir -p "$repo/.superpowers/m9-integ-07"
    scratch=$(mktemp -d "$repo/.superpowers/m9-integ-07/consumer.XXXXXXXX")
    [[ -n "$scratch" && "$scratch" == /* ]] || exit 1
    touch "$scratch/.flea-test-sandbox"
    mkdir "$scratch/home"
    exec env -i PATH=/usr/bin:/bin HOME="$scratch/home" LANG=C.UTF-8 \
        setsid -w unshare --user --map-root-user --pid --fork --mount-proc \
        /bin/bash "$0" --isolated "$repo" "$scratch" </dev/null
fi
repo=$2
scratch=$3
trap 'case $scratch in "$repo"/.superpowers/m9-integ-07/consumer.?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
[[ ! -t 0 && ! -t 1 && -f "$scratch/.flea-test-sandbox" ]] ||
    { echo "m9_integ_b1: refused: needs no terminal and a marked sandbox at $scratch" >&2; exit 1; }
python3 -B - "$repo" "$scratch" <<'PY'
import copy
import errno
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import types

repo, root = map(Path, sys.argv[1:])
assert os.getpid() in (1, 2) and Path('/proc/1').exists()
try:
    tty = os.open('/dev/tty', os.O_RDWR | os.O_NOCTTY)
except OSError as error:
    assert error.errno == errno.ENXIO
else:
    os.close(tty)
    raise AssertionError('CI run unexpectedly has a controlling terminal')
loader = importlib.machinery.SourceFileLoader('b1_consumer', str(repo / 'tools/flea-strata-bench'))
spec = importlib.util.spec_from_loader(loader.name, loader)
tool = importlib.util.module_from_spec(spec)
loader.exec_module(tool)

def invoke(*args, expected=0):
    result = subprocess.run([sys.executable, '-B', str(repo / 'tools/flea-strata-bench'), *map(str, args)],
                            stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=30)
    assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
    print(result.stdout.strip())
    return result

invoke('init', root)
plan = tool.read_plan(root)
products = {ident: {'binary': '/synthetic/' + ident, 'sha256': 'a' * 64, 'version': ident + ' 1.0',
                   'frontend': 'gpui' if ident == 'flea' else 'qml', 'gui_sha256': 'b' * 64} for ident in tool.IDS}
entrants = {ident: dict(value, gui={'binary': '/synthetic/flea-gui', 'sha256': 'b' * 64}, attestation={})
            for ident, value in products.items()}
tool.write_json(root / 'fixtures.json', {})
manifest = {'plan': plan, 'harness_sha256': tool.sha(tool.__file__), 'latest_verified': True,
            'fixture_manifest_sha256': tool.sha(root / 'fixtures.json'), 'entrants': entrants,
            'product_helper_sha256': 'c' * 64}
tool.write_json(root / 'manifest.json', manifest)
annotations = {}
for trial_spec in tool.trial_specs(plan):
    folder = root / 'trials' / trial_spec['id']
    (folder / 'frames').mkdir(parents=True)
    score = 0.1 if trial_spec['entrant'] == 'flea' else 2.0
    frames = [{'start_s': start, 'end_s': end, 'file': 'frames/%d.jpg' % index}
              for index, (start, end) in enumerate(((-0.2, -0.1), (score, score + 0.05), (score + 0.25, score + 0.3)))]
    for frame in frames:
        (folder / frame['file']).write_bytes(b'synthetic contract evidence, not native pixels')
        frame['sha256'] = tool.sha(folder / frame['file'])
    trial = dict(trial_spec, product_identity=products[trial_spec['entrant']], product_helper_sha256='c' * 64,
                 argv=['dbus-run-session', '--', products[trial_spec['entrant']]['binary']],
                 baseline=frames[0], frames=frames[1:], navigation_span_s=0.01, ended_s=5,
                 samples=[(i / 20, min(i / 20, score), 10 if trial_spec['entrant'] == 'flea' else 20) for i in range(101)])
    tool.write_json(folder / 'trial.json', trial)
    annotations[trial_spec['id']] = {'reviewer': 'synthetic consumer contract', 'navigation_verified': True,
        'settings_verified': True, 'visible_total': 12, 'verified': 12, 'first': [-1, 0], 'ready': [0, 1]}
tool.write_json(root / 'annotations.json', annotations)
invoke('report', root)
report = json.loads((root / 'results.json').read_text())
assert report['gate_passed'] and len(report['rows']) == 330 and not report['pending']
assert report['gate_scope'] == 'historical-b1' and report['release_scope_complete'] is False
assert report['entrants'] == entrants
print('b1-consumer: accepted CLI report consumes 420 matching trials and preserves product versions')

path = root / 'trials/strata-nightly.raw.3/trial.json'
original = json.loads(path.read_text())
for label, change in (
    ('wrong owner', lambda trial: trial.update(entrant='strata-stable')),
    ('wrong launched binary', lambda trial: trial['argv'].__setitem__(2, '/synthetic/strata-stable')),
    ('stale product version', lambda trial: trial['product_identity'].update(version='stale 0.1')),
    ('stale product helper', lambda trial: trial.update(product_helper_sha256='d' * 64)),
    ('missing product adapter reply', lambda trial: trial.pop('product_identity')),
):
    trial = copy.deepcopy(original)
    change(trial)
    tool.write_json(path, trial, replace=True)
    invoke('report', root, expected=2)
    report = json.loads((root / 'results.json').read_text())
    assert not report['gate_passed'] and report['release_scope_complete'] is False
    assert [item['trial'] for item in report['pending']] == ['strata-nightly.raw.3']
    rows = [row for row in report['rows'] if row['entrant'] == 'strata-nightly' and row['workload'] == 'raw']
    assert all(row['runs'] == 2 and row['score'] is None and row['rank'] is None for row in rows)
    print('b1-consumer: refused ' + label)
tool.write_json(path, original, replace=True)
path.rename(path.with_name('withheld.json'))
invoke('report', root, expected=2)
report = json.loads((root / 'results.json').read_text())
assert not report['gate_passed'] and len(report['pending']) == 1 and report['release_scope_complete'] is False
assert len(report['rows']) == 330
print('b1-consumer: missing rival trial remains explicit and cannot pass full coverage')
fixtures = (root / 'fixtures.json').read_bytes()
(root / 'fixtures.json').write_text('{"edited": true}')
error = invoke('report', root, expected=3)
assert 'fixture manifest changed' in error.stderr
(root / 'fixtures.json').write_bytes(fixtures)
print('b1-consumer: a harness error exits 3, apart from a measured loss')
annotations['flea.raw.1']['reviewer'] = None
tool.write_json(root / 'annotations.json', annotations, replace=True)
invoke('report', root, expected=2)
report = json.loads((root / 'results.json').read_text())
assert 'flea.raw.1' in [item['trial'] for item in report['pending']]
annotations['flea.raw.1']['reviewer'] = 'synthetic consumer contract'
tool.write_json(root / 'annotations.json', annotations, replace=True)
print('b1-consumer: a non-text reviewer leaves its trial pending instead of crashing the report')

adapter = root / 'adapter'
(adapter / 'tools').mkdir(parents=True)
helper = adapter / 'tools/flea_bench_product.py'
helper.write_text('# Synthetic ABI spy only.\n')
old_repo, old_loader = tool.REPO, tool.load_module
old_producers = tool.manifest, tool.load_raw_corpus, tool.validate_fixtures, tool.run_trial
calls = []
def identity(arm, argv):
    calls.append((arm, argv))
    return copy.deepcopy(products['flea'])
try:
    tool.REPO = adapter
    tool.load_module = lambda name, path: types.SimpleNamespace(identity=identity)
    data = {'entrants': entrants}
    bound = tool.bind_products(plan, data, ['flea'])
    assert calls == [({'entrant': 'flea', 'frontend': 'gpui'}, ['/synthetic/flea', '--gui'])]
    assert bound['identities']['flea'] == products['flea'] and bound['helper_sha256'] == tool.sha(helper)
    native_root = root / 'native-boundary'
    native_root.mkdir()
    (native_root / tool.MARKER).touch()
    native_plan = copy.deepcopy(plan)
    for ident in tool.IDS:
        native_plan['entrants'][ident]['expected_version'] = products[ident]['version']
    tool.write_json(native_root / 'plan.json', native_plan)
    tool.write_json(native_root / 'fixtures.json', {})
    tool.manifest = lambda plan: dict(copy.deepcopy(manifest), plan=plan, host='synthetic', kernel='synthetic',
                                     build_sha='synthetic', rival_inventory=None)
    tool.load_raw_corpus = lambda value: {'sets': {}}
    tool.validate_fixtures = lambda root: {'raw_corpus': {'sets': {}}}
    native = types.SimpleNamespace(ENV=None, session_env=lambda: {}, hypr=lambda *args: '[]')
    tool.load_module = lambda name, path: types.SimpleNamespace(identity=identity) if path == helper else native
    captured = []
    tool.run_trial = lambda root, plan, spec, native, binaries: captured.append((spec, copy.deepcopy(binaries['flea'])))
    tool.run(native_root, only='flea', installed=True, raw_corpus=root)
    assert len(captured) == 42 and all(spec['entrant'] == 'flea' and value['product_identity'] == products['flea']
        and value['product_helper_sha256'] == tool.sha(helper) for spec, value in captured)
    invocation = json.loads(next((native_root / 'invocations').iterdir()).read_text())
    assert invocation['products'] == bound
    print('b1-consumer: real run dispatches 42 selected trials with matching B6 replies and an invocation receipt')
    for key in ('binary', 'version', 'sha256', 'gui_sha256'):
        def stale(arm, argv, key=key):
            reply = copy.deepcopy(products['flea'])
            reply[key] = 'wrong'
            return reply
        tool.load_module = lambda name, path: types.SimpleNamespace(identity=stale)
        try:
            tool.bind_products(plan, data, ['flea'])
        except ValueError:
            pass
        else:
            raise AssertionError('B6 accepted mismatched ' + key)
    helper.rename(helper.with_suffix('.withheld'))
    try:
        tool.bind_products(plan, data, ['flea'])
    except ValueError as error:
        assert 'stage tools/flea_bench_product.py' in str(error)
    else:
        raise AssertionError('missing B6 helper accepted')
finally:
    tool.REPO, tool.load_module = old_repo, old_loader
    tool.manifest, tool.load_raw_corpus, tool.validate_fixtures, tool.run_trial = old_producers
print('b1-consumer: B6 request binds real launch argv to matching reply; missing/wrong/stale ABI controls refused')
print('m9_integ_b1_missing_trials_never_pass_full_coverage: PASS 8 consumer scenarios and 6 B6 ABI controls; synthetic only')
PY
