"""Run reviewer defects through Engine, the real runner, and final table filtering."""
import argparse
import csv
import json
import os
import pathlib
import subprocess
import sys

from fixtures import marked_root


CASES = ['valid', 'valid-no-total', 'short-head', 'short-head-no-total', 'short-final-head', 'startup-miss', 'startup-misrank',
         'startup-clock', 'prior-valid', 'prior-replay-inside-open']


def run_case(parent, case):
    import runner

    root = marked_root(parent, case + '-')
    build = root / 'build'
    adapter = pathlib.Path(__file__).with_name('production_adapter.py').resolve()
    for binary in ['flea-target/release/flea-engine', 'target/release/fff-driver']:
        path = build / binary
        path.parent.mkdir(parents=True, exist_ok=True)
        path.symlink_to(adapter)
    provider = root / 'bin'
    provider.mkdir()
    (provider / 'zoxide').symlink_to(adapter)
    os.environ['PATH'] = str(provider) + os.pathsep + os.environ['PATH']
    os.environ['FB0_REVIEW_MODE'] = case
    output = marked_root(root, 'run-')
    runner.CONFIGURATIONS = [('fff', 'equal-history')] if case in {'prior-valid', 'prior-replay-inside-open'} else [('flea', 'fresh'), ('fff', 'fresh')]
    sys.argv = ['runner.py', '--build', str(build), '--output', str(output), '--runs', '1', '--fixtures', '400']
    status = runner.main(control=True)
    with (output / 'measurements.tsv').open() as stream:
        samples = list(csv.DictReader(stream, delimiter='\t'))
    with (output / 'performance.tsv').open() as stream:
        performance = list(csv.DictReader(stream, delimiter='\t'))
    engine = 'fff' if case in {'short-head-no-total', 'prior-replay-inside-open'} else 'flea'
    rejected = [row for row in samples if row['engine'] == engine and row['eligible'] == 'no']
    if case in {'valid', 'valid-no-total', 'prior-valid'}:
        if status or rejected or not performance:
            raise AssertionError('valid production adapter failed')
        if case == 'prior-valid':
            ledgers = list(output.glob('state-fff-*/fff-prior-seed.json'))
            if len(ledgers) != 2:
                raise AssertionError('both measured process workloads must copy the fixture prior seed')
            reports = [json.loads(path.read_text()) for path in ledgers]
            if reports[0]['events'] != reports[1]['events'] or reports[0]['files'] != reports[1]['files']:
                raise AssertionError('per-instance prior event times or bytes differ')
            for path, seed in zip(ledgers, reports):
                from priors import database_hashes
                if database_hashes(path.parent / 'frecency') != seed['files']:
                    raise AssertionError('normal index open replayed or changed the native history seed')
    elif case == 'startup-clock':
        leaked = [row for row in samples if row['engine'] == engine and row['query_kind'] == 'startup-overlap'
                  and row['metric'] == 'match_select_paths_us']
        walk = [row for row in samples if row['engine'] == engine and row['query_kind'] == 'startup-overlap'
                and row['metric'] == 'startup_walk_ms']
        if status or leaked or not walk or rejected:
            raise AssertionError('legacy startup walk clock was exported as a matcher or unrelated work failed')
    else:
        if not rejected or any(row['engine'] == engine for row in performance):
            raise AssertionError('invalid production adapter remained eligible')
        reasons = '; '.join(row['ineligible_reason'] for row in rejected)
        wanted = {'short-head': 'returned head', 'short-head-no-total': 'returned head', 'short-final-head': 'returned head',
                  'startup-miss': 'startup: final target missing',
                  'startup-misrank': 'startup: unambiguous final target misranked',
                  'prior-replay-inside-open': 'native prior history changed during index open'}[case]
        if wanted not in reasons:
            raise AssertionError('control failed for an unrelated reason: ' + reasons)
    result = dict(case=case, accepted=case in {'valid', 'valid-no-total', 'prior-valid'}, runner_exit=status, output=str(output),
                  reasons=sorted({row['ineligible_reason'] for row in rejected}),
                  excluded_clock=case == 'startup-clock')
    (root / 'control.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def run_production_controls(parent):
    report = {}
    for case in CASES:
        process = subprocess.run([sys.executable, '-B', __file__, '--case', case, '--parent', str(parent)],
                                 text=True, capture_output=True, timeout=45)
        (parent / (case + '.log')).write_text(process.stdout + process.stderr)
        if process.returncode:
            raise AssertionError(f'production control {case} failed; see {parent / (case + ".log")}')
        report['production-' + case] = json.loads(process.stdout.splitlines()[-1])
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--parent', type=pathlib.Path, required=True)
    args = parser.parse_args()
    print(json.dumps(run_case(args.parent, args.case)))
