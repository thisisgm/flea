#!/usr/bin/env bash
# Checks capture bounds, app-group sampling, incomplete coverage and the full report without a display.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd -P)
mkdir -p "$repo/.superpowers/m9-integ-07"
scratch=$(mktemp -d "$repo/.superpowers/m9-integ-07/contracts.XXXXXXXX")
[[ -n "$scratch" && "$scratch" == /* ]] || exit 1
touch "$scratch/.flea-test-sandbox"
trap 'case $scratch in "$repo"/.superpowers/m9-integ-07/contracts.?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
python3 -B "$repo/tools/flea-strata-bench" selftest "$scratch"
python3 -B "$repo/tools/flea-strata-bench" init "$scratch"
python3 -B "$repo/tools/flea-strata-bench" dry-run "$scratch"
python3 -B "$repo/tools/flea-strata-bench" selftest "$scratch" > /dev/null ||
    { printf 'strata-bench: FAIL a second selftest on one ROOT\n' >&2; exit 1; }
python3 -B "$repo/tools/flea-strata-bench" dry-run "$scratch" > /dev/null ||
    { printf 'strata-bench: FAIL a second dry-run on one ROOT\n' >&2; exit 1; }
set +e
python3 -B "$repo/tools/flea-strata-bench" reprot "$scratch" 2> "$scratch/usage.stderr"
rc=$?
set -e
[[ "$rc" == 3 ]] && grep -q '^strata-bench: usage error: ' "$scratch/usage.stderr" ||
    { printf 'strata-bench: FAIL a usage error exits %s, not 3\n' "$rc" >&2; exit 1; }
printf 'strata-bench: selftest and dry-run rerun on one ROOT, and a usage error exits 3\n'
typed="$scratch/wrong-typed"
mkdir "$typed"
touch "$typed/.flea-test-sandbox"
python3 -B -c 'import json,sys; p=json.load(open(sys.argv[1])); p["visual_timeout_s"]="5"; json.dump(p, open(sys.argv[2], "w"))' \
    "$scratch/plan.json" "$typed/plan.json"
set +e
python3 -B "$repo/tools/flea-strata-bench" dry-run "$typed" 2> "$typed/stderr"
rc=$?
set -e
[[ "$rc" == 3 ]] && grep -qxF "strata-bench: plan $typed/plan.json: field visual_timeout_s must be a number from 1 to 60, not \"5\"" "$typed/stderr" ||
    { printf 'strata-bench: FAIL wrong-typed plan field rc=%s\n' "$rc" >&2; exit 1; }
printf 'strata-bench: a wrong-typed plan field exits 3 naming the field and the plan\n'
injected="$scratch/injected"
mkdir -p "$injected/tools"
sed 's/^def dry_run(root):$/&\n    raise TypeError("injected harness bug")/' "$repo/tools/flea-strata-bench" > "$injected/tools/flea-strata-bench"
grep -q 'injected harness bug' "$injected/tools/flea-strata-bench" ||
    { printf 'strata-bench: FAIL sed anchor ^def dry_run(root):$ no longer matches tools/flea-strata-bench\n' >&2; exit 1; }
set +e
python3 -B "$injected/tools/flea-strata-bench" dry-run "$scratch" 2> "$injected/stderr"
rc=$?
set -e
[[ "$rc" == 3 ]] && grep -q '^Traceback' "$injected/stderr" && grep -qxF 'TypeError: injected harness bug' "$injected/stderr" &&
    grep -qxF 'strata-bench: harness error, not a result' "$injected/stderr" ||
    { printf 'strata-bench: FAIL injected harness bug rc=%s\n' "$rc" >&2; exit 1; }
printf 'strata-bench: an unexpected harness error prints its traceback and exits 3, never a result\n'
python3 -B - "$repo" "$scratch" <<'PY'
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import sys
import types
import copy
import csv
import subprocess

repo, root = map(Path, sys.argv[1:])
loader = importlib.machinery.SourceFileLoader('strata_contracts', str(repo / 'tools/flea-strata-bench'))
spec = importlib.util.spec_from_loader(loader.name, loader)
tool = importlib.util.module_from_spec(spec)
loader.exec_module(tool)
plan = tool.read_plan(root)
for name, entrant, broken, field in (('archive', 'strata-stable', {'binary_path': '/x', 'expected_version': '1'}, 'binary_sha256'),
                                     ('package', 'dolphin', {'sha256': 7}, 'sha256')):
    nested = root / ('nested-' + name)
    nested.mkdir()
    bad = copy.deepcopy(plan)
    bad['entrants'][entrant][name] = broken
    tool.write_json(nested / 'plan.json', bad)
    try:
        tool.read_plan(nested)
    except ValueError as error:
        assert 'field entrants.%s.%s.%s must be a string' % (entrant, name, field) in str(error), error
    else:
        raise AssertionError('a broken %s key was accepted' % name)
print('strata-bench: archive and package keys the inventory reads are named on refusal')
expected_trials = len(tool.IDS) * 3 * 14
assert len(list(tool.trial_specs(plan))) == expected_trials
fixture = root / 'fixtures.json'
fixture.write_text('{}')
tool.write_json(root / 'manifest.json', {'plan': plan, 'harness_sha256': tool.sha(tool.__file__),
                                       'fixture_manifest_sha256': tool.sha(fixture)})
tool.write_json(root / 'annotations.json', {})
assert tool.report(root) == 2
results = json.loads((root / 'results.json').read_text())
assert not results['gate_passed'] and len(results['pending']) == expected_trials
assert len(results['rows']) == 33 * len(tool.IDS) and len(results['flea_not_first']) == 33
assert all(r['rank'] is None and r['score'] is None for r in results['rows'])
assert len((root / 'results.csv').read_text().splitlines()) == 33 * len(tool.IDS) + 1
unmarked = root / 'unmarked'
unmarked.mkdir()
linked = root / 'linked'
linked.symlink_to(root, target_is_directory=True)
for path in (unmarked, linked):
    try:
        tool.root_path(str(path))
    except ValueError:
        pass
    else:
        raise AssertionError('unmarked or symlinked root accepted')
print('strata-bench: all missing trials remain unranked across all 33 measures; root guards passed')

complete = root / 'synthetic-report'
complete.mkdir()
(complete / tool.MARKER).touch()
tool.write_json(complete / 'plan.json', plan)
tool.write_json(complete / 'fixtures.json', {})
products = {ident: {'binary': '/synthetic/' + ident, 'version': ident + ' 1.0', 'sha256': 'c' * 64,
                   'frontend': 'gpui' if ident == 'flea' else 'qml', 'gui_sha256': 'd' * 64}
            for ident in tool.IDS}
entrants = {ident: dict(product, gui={'sha256': 'd' * 64}, attestation={}) for ident, product in products.items()}
tool.write_json(complete / 'manifest.json', {'plan': plan, 'harness_sha256': tool.sha(tool.__file__),
                'entrants': entrants, 'product_helper_sha256': 'e' * 64,
                'fixture_manifest_sha256': tool.sha(complete / 'fixtures.json'), 'latest_verified': True,
                'raw_corpus': {'manifest_sha256': 'a' * 64, 'sets': {'twelve': [{'format': 'nef'}], 'hundred': []}}})
(complete / 'trials').mkdir()
annotations = {}
for spec in tool.trial_specs(plan):
    folder = complete / 'trials' / spec['id']
    folder.mkdir()
    score = 0.1 if spec['entrant'] == 'flea' else 2.0
    samples = [(i / 20, min(i / 20, score), 10 if spec['entrant'] == 'flea' else 20) for i in range(101)]
    trial = dict(spec, product_identity=products[spec['entrant']], product_helper_sha256='e' * 64,
                 argv=['dbus-run-session', '--', products[spec['entrant']]['binary']],
                 baseline={'start_s': -0.2, 'end_s': -0.1}, navigation_span_s=0.01,
                 frames=[{'start_s': score, 'end_s': score + 0.05},
                         {'start_s': score + 0.25, 'end_s': score + 0.3}], samples=samples, ended_s=5)
    (folder / 'frames').mkdir()
    for index, frame in enumerate([trial['baseline'], *trial['frames']]):
        filename = 'frames/synthetic-%d.jpg' % index
        (folder / filename).write_bytes(b'synthetic contract evidence')
        frame.update(file=filename, sha256=tool.sha(folder / filename))
    tool.write_json(folder / 'trial.json', trial)
    annotations[spec['id']] = {'reviewer': 'synthetic contract only', 'navigation_verified': True,
        'settings_verified': True, 'visible_total': 12, 'verified': 12, 'first': [-1, 0], 'ready': [0, 1]}
tool.write_json(complete / 'annotations.json', annotations)
assert tool.report(complete) == 0
summary = json.loads((complete / 'results.json').read_text())
assert summary['gate_passed'] and not summary['flea_not_first']
assert summary['raw_corpus']['manifest_sha256'] == 'a' * 64 and summary['raw_corpus']['sets']['twelve'][0]['format'] == 'nef'
assert all(row['runs'] == 3 for row in summary['rows'])
annotations['flea.raw.1']['verified'] = 11
annotations['flea.raw.1']['ready'] = None
tool.write_json(complete / 'annotations.json', annotations, replace=True)
assert tool.report(complete) == 1
summary = json.loads((complete / 'results.json').read_text())
assert not summary['gate_passed']
raw = next(row for row in summary['rows'] if row['entrant'] == 'flea' and row['workload'] == 'raw' and row['metric'] == 'ready_s')
assert raw['score'] is None and '11/12 verified' in raw['status']
print('strata-bench: synthetic complete matrix aggregates three runs and gates all 33 measures')
annotations['flea.raw.1'].update(verified=12, ready=[0, 1])
annotations['flea.cpu-raw.1'].update(visible_total=100, verified=50)
tool.write_json(complete / 'annotations.json', annotations, replace=True)
assert tool.report(complete) == 1
summary = json.loads((complete / 'results.json').read_text())
assert not summary['gate_passed']
assert [(loss['workload'], loss['metric']) for loss in summary['flea_not_first']] == [('cpu-raw', 'quiet_s')]
print('strata-bench: a Flea CPU run with 50 of 100 previews verified loses its quiet_s column')

early = root / 'installed-only'
early.mkdir()
(early / tool.MARKER).touch()
tool.write_json(early / 'plan.json', plan)
tool.write_json(early / 'fixtures.json', {})
originals = tool.validate_fixtures, tool.manifest, tool.load_module, tool.run_trial, tool.load_raw_corpus, tool.bind_products
captured = []
try:
    corpus_identity = {'manifest_sha256': 'b' * 64, 'sets': {'twelve': [], 'hundred': []}}
    tool.load_raw_corpus = lambda value: corpus_identity
    tool.validate_fixtures = lambda root: {'raw_corpus': corpus_identity}
    tool.manifest = lambda plan: {'host': 'synthetic', 'kernel': 'synthetic', 'build_sha': 'synthetic',
        'harness_sha256': tool.sha(tool.__file__), 'plan': plan, 'rival_inventory': None,
        'entrants': {ident: {'binary': '/synthetic/' + ident if ident != 'flea' else None,
                            'version': settings['expected_version']} for ident, settings in plan['entrants'].items()}}
    tool.load_module = lambda *args: types.SimpleNamespace(ENV=None, session_env=lambda: {}, hypr=lambda *args: '[]')
    tool.run_trial = lambda root, plan, spec, native, binaries: captured.append(spec)
    tool.bind_products = lambda plan, data, selected: {'helper_sha256': 'e' * 64,
        'identities': {ident: {'binary': data['entrants'][ident]['binary']} for ident in selected}}
    tool.run(early, only='flea-0.3.12', installed=True, raw_corpus=early / 'corpus')
    assert len(captured) == 42 and all(t['entrant'] == 'flea-0.3.12' for t in captured)
    assert json.loads((early / 'manifest.json').read_text())['latest_verified'] is False
    frozen = (early / 'manifest.json').read_bytes()
    tool.run(early, only='strata-stable', installed=True, raw_corpus=early / 'corpus')
    assert len(captured) == 84 and all(t['entrant'] == 'strata-stable' for t in captured[42:])
    assert (early / 'manifest.json').read_bytes() == frozen
    invocations = [json.loads(p.read_text()) for p in (early / 'invocations').glob('*.json')]
    assert sorted(i['selected'] for i in invocations) == [['flea-0.3.12'], ['strata-stable']]
    old_manifest = tool.manifest
    tool.manifest = lambda plan: dict(old_manifest(plan), kernel='changed')
    try:
        tool.run(early, only='krusader', installed=True, raw_corpus=early / 'corpus')
    except ValueError as error:
        assert 'resume environment differs' in str(error)
    else:
        raise AssertionError('changed kernel accepted between chunks')
finally:
    tool.validate_fixtures, tool.manifest, tool.load_module, tool.run_trial, tool.load_raw_corpus, tool.bind_products = originals
try:
    tool.monitor_geometry([{'name': 'DP-1', 'width': 1, 'height': 1, 'x': 0, 'y': 0, 'scale': 1, 'transform': 0}])
except ValueError as error:
    assert all(part in str(error) for part in ('hyprctl -j monitors', 'DP-1', 'refreshRate')), error
else:
    raise AssertionError('a monitor without refreshRate was accepted')
print('strata-bench: installed-only arm captures without any 0.4.0 or rival binary and cannot pass the latest gate')

receipts = root / 'archives-only'
receipts.mkdir()
archive_plan = copy.deepcopy(plan)
receipt = {'host': 'synthetic', 'kernel': 'synthetic', 'date_utc': '2026-10-20T12:54:00Z', 'archives': [], 'packages': {}}
versions = {'strata-stable': 'strata 1.0.0', 'strata-nightly': 'strata 1.0.1-nightly.20261018',
            'strata-nightly-channel': 'strata 1.1.0-rc.2'}
for index, (ident, version) in enumerate(versions.items()):
    binary = receipts / ident
    binary.write_text("#!/bin/sh\nprintf '%s\\n' '" + version + "'\n")
    binary.chmod(0o755)
    receipt['archives'].append({'id': ident, 'binary_path': str(binary), 'binary_sha256': tool.sha(binary),
        'expected_version': version, 'version_check': {'rc': 0, 'stdout': version}, 'source_commit': str(index) * 40,
        'tag': version.split()[-1], 'tag_commit': str(index + 1) * 40, 'sha256': str(index) * 64,
        'url': 'https://example.invalid/releases/' + ident + '.tar.gz'})
for ident in ('krusader', 'dolphin', 'nautilus', 'nemo', 'thunar'):
    binary = receipts / ident
    binary.write_text("#!/bin/sh\nprintf '%s\\n' '" + ident + " package-version'\n")
    binary.chmod(0o755)
    receipt['packages'][ident] = {'realpath': str(binary), 'sha256': tool.sha(binary),
        'package': {'rc': 0, 'stdout': ident + ' 2.9.0-2'}, 'owner': {'rc': 0, 'stdout': 'synthetic owner'}}
receipt_path = receipts / 'rival-versions.json'
tool.write_json(receipt_path, receipt)
original_output = tool.output
try:
    tool.output = lambda args: receipt['packages'][args[2]]['package']['stdout'] if len(args) == 3 and args[:2] == ['pacman', '-Q'] else original_output(args)
    tool.pin_inventory(archive_plan, receipt_path)
    installed = tool.inventory(archive_plan)
    assert set(versions) <= set(installed)
    for ident, version in versions.items():
        assert installed[ident]['version'] == version
        assert installed[ident]['attestation']['archive']['binary_sha256'] == installed[ident]['sha256']
    assert tool.rival_inventory(archive_plan) == receipt
    binary = receipts / 'strata-stable'
    original = binary.read_bytes()
    binary.write_bytes(original + b'# changed\n')
    try:
        tool.inventory(archive_plan)
    except ValueError as error:
        assert 'archive binary/version changed' in str(error)
    else:
        raise AssertionError('changed archive binary accepted')
    binary.write_bytes(original)
    receipt_path.write_text('{}')
    try:
        tool.rival_inventory(archive_plan)
    except ValueError as error:
        assert 'inventory changed' in str(error)
    else:
        raise AssertionError('changed inventory accepted')
finally:
    tool.output = original_output
print('strata-bench: three archive channels preserve version/URL/archive/source/binary identities; tampering refused without source trees')
try:
    tool.load_raw_corpus(None)
except ValueError as error:
    assert '--raw-corpus' in str(error)
else:
    raise AssertionError('RAW accepted without an explicit corpus')
print('strata-bench: RAW workloads refuse an omitted corpus; report preserves corpus manifest and per-file formats')
adapter = root / 'hermetic-corpus-adapter'
(adapter / 'tools').mkdir(parents=True)
corpus = adapter / 'corpus'
calls = adapter / 'verify-calls'
helper = adapter / 'tools/flea-raw-corpus'
helper.write_text('#!/usr/bin/env python3\nimport sys\nfrom pathlib import Path\np=Path(__file__).parent.parent\n'
                  "(p/'verify-calls').open('a').write(' '.join(sys.argv[1:])+'\\n')\n"
                  "sys.exit(7 if (p/'reject').exists() else 0)\n")
helper.chmod(0o755)
suffixes = ('cr2', 'cr3', 'nef', 'arw', 'raf', 'rw2', 'orf', 'pef', 'srw', 'dng', '3fr', 'mos')
fields = ('set', 'suffix', 'make', 'model', 'url', 'sha256', 'bytes', 'licence', 'mime')
with (adapter / 'tools/flea-raw-corpus.tsv').open('w') as f:
    writer = csv.DictWriter(f, fieldnames=fields, delimiter='\t')
    writer.writeheader()
    for name, count in (('twelve', 12), ('hundred', 100)):
        folder = corpus / name
        folder.mkdir(parents=True)
        for i in range(count):
            suffix = suffixes[i % len(suffixes)]
            path = folder / ('hermetic-%03d.' % i + suffix)
            path.write_bytes(('hermetic adapter bytes %s %d' % (name, i)).encode())
            writer.writerow(dict(set=name, suffix=suffix, make='test', model='test',
                url='https://raw.pixls.us/test-only/' + path.name, sha256=tool.sha(path),
                bytes=path.stat().st_size, licence='CC0', mime='image/x-test-raw'))
old_repo = tool.REPO
try:
    tool.REPO = adapter
    verified = tool.load_raw_corpus(corpus)
    assert len(verified['sets']['twelve']) == 12 and len(verified['sets']['hundred']) == 100
    assert len({f['format'] for f in verified['sets']['twelve']}) == 12
    assert len(calls.read_text().splitlines()) == 2 and all(line.startswith('verify ') for line in calls.read_text().splitlines())
    changed = next((corpus / 'hundred').iterdir())
    original = changed.read_bytes()
    changed.write_bytes(b'corrupt')
    try:
        tool.load_raw_corpus(corpus)
    except ValueError:
        pass
    else:
        raise AssertionError('changed real-corpus bytes accepted')
    changed.write_bytes(original)
    (adapter / 'reject').touch()
    try:
        tool.load_raw_corpus(corpus)
    except subprocess.CalledProcessError as error:
        assert error.returncode == 7
    else:
        raise AssertionError('RAW verifier failure accepted')
finally:
    tool.REPO = old_repo
print('strata-bench: hermetic adapter verifies both real-corpus sets first, maps twelve formats and refuses corruption/verifier failure')
PY
bash "$repo/tests/m9_integ_b1_missing_trials_never_pass_full_coverage.sh"
if [[ -x "$repo/target/release/flea" ]]; then
    mkdir "$scratch/empty" "$scratch/home" "$scratch/cache" "$scratch/state" "$scratch/config"
    set +e
    env -u DISPLAY -u WAYLAND_DISPLAY HOME="$scratch/home" XDG_CACHE_HOME="$scratch/cache" \
        XDG_STATE_HOME="$scratch/state" XDG_CONFIG_HOME="$scratch/config" FLEA_FRONTEND=gpui \
        timeout 10 "$repo/target/release/flea" --gui "$scratch/empty" > "$scratch/headless-product.log" 2>&1
    rc=$?
    set -e
    [[ "$rc" == 2 ]] && grep -q 'there is no graphical session' "$scratch/headless-product.log" ||
        { printf 'strata-bench: FAIL flea --gui headless rc=%s\n' "$rc" >&2; exit 1; }
    printf 'strata-bench: flea --gui headless refusal verified, rc=2, no native timing claimed\n'
fi
printf 'strata-bench: PASS capture/CPU/coverage/report contracts\n'
