"""Paired engine and stage comparisons with explicit unsupported metric boundaries."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE / 'engine'))
from corpus import COMMIT, DEFAULT_ROOT, prepare, load
from cold import cache_gate, clocks

ENGINE_ROSTER = ['flea/native', 'flea-0.3.12/native', 'fff/bare', 'fff/rich', 'fsearch/native', 'fsearch-nd/linux-core'] + [name + '/' + profile for profile in ['fresh', 'equal-history'] for name in ['flea', 'flea-0.3.12', 'fff', 'fsearch', 'fsearch-nd']]
WALK_ROSTER = ['flea-search', 'std-serial', 'ignore-parallel', 'ignore-serial', 'jwalk', 'dua-core', 'fd', 'ripgrep']


def pairs(entrants, reference, run):
    for rival in entrants:
        if rival != reference:
            yield rival, [reference, rival] if run % 2 == 0 else [rival, reference]


def stage_pairs(stage, entrants, run):
    if stage != 'engine':
        for rival, order in pairs(entrants, entrants[0], run):
            yield entrants[0], rival, order
        return
    for rival in entrants:
        name, profile = rival.split('/')
        if name == 'flea':
            continue
        reference = 'flea/' + (profile if profile in {'fresh', 'equal-history'} else 'native')
        if reference not in entrants:
            raise ValueError('missing matched candidate profile: ' + reference)
        yield reference, rival, [reference, rival] if run % 2 == 0 else [rival, reference]


def host_load():
    values = os.getloadavg()
    return dict(load_1=values[0], load_5=values[1], load_15=values[2], above_4=values[0] > 4,
                at_ns=time.monotonic_ns())


# Sample input: fzf "0.74.5 (hash)", fd "fd 10.5.0", rg "ripgrep 15.2.0".
def pinned_version(name, value):
    expected = {'fzf':['0.74.5'], 'fd':['fd','10.5.0'], 'rg':['ripgrep','15.2.0']}[name]
    first_line = value.splitlines()[0].split() if value else []
    return first_line[:len(expected)] == expected


def capabilities(stage):
    return dict(cancel_to_idle=dict(supported=False, reason='no worker-idle acknowledgement or native idle counter; close acknowledgement is a separate clock'),
                watch_freshness=dict(supported=False, reason=(
                    'engine adapters do not export recursive watch health and mutation epochs; relist clocks cannot establish passive freshness'
                    if stage == 'engine' else 'stage adapters own no persistent watched product session')))


def imported(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def marked(parent, prefix):
    parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix=prefix, dir=parent)).resolve()
    (root / '.findbench-root').write_text('flea-searchbench\n')
    return root


def preflight(args, output):
    result = dict(host=socket.gethostname(), candidate=COMMIT, load=host_load(), corpus_root=str(args.corpus_root),
                  capabilities={s: capabilities(s) for s in ['engine', 'match', 'walk']}, blockers=[])
    if result['host'] not in {'ultra-36', 'ultra-27'}:
        result['blockers'].append('timing restricted to East2 and West')
    from artifacts import candidate, engine, roster
    stages = getattr(args, 'stages', 'engine,match,walk').split(',')
    if not set(stages) <= {'engine','match','walk'}:
        result['blockers'].append('unknown stages')
    for name, path, check in [('candidate', args.flea_bin, candidate),
                              ('engine', args.engine_build, engine),
                              ('matcher', args.matcher_bin, roster),
                              ('walker', args.walker_bin, roster)]:
        required = name == 'candidate' or {'engine':'engine','matcher':'match','walker':'walk'}[name] in stages
        if not required:
            continue
        if path is None:
            result['blockers'].append('missing build input: ' + name)
            continue
        try:
            result[name] = check(path)
        except (OSError, ValueError, KeyError) as error:
            result['blockers'].append(name + ': ' + str(error))
    for name in ['fzf', 'fd', 'rg']:
        if ('match' if name == 'fzf' else 'walk') not in stages:
            continue
        path = getattr(args, name, None)
        if path is None or not path.is_file() or not os.access(path, os.X_OK):
            result['blockers'].append('missing executable roster tool: ' + name)
        else:
            from artifacts import digest
            try:
                version = subprocess.run([str(path), '--version'], check=True, capture_output=True, text=True, timeout=5).stdout.strip()
                expected = {'fzf':'0.74.5', 'fd':'fd 10.5.0', 'rg':'ripgrep 15.2.0'}[name]
                if not pinned_version(name, version):
                    raise ValueError('expected ' + expected + ', observed ' + version)
                result[name] = dict(path=str(path), sha256=digest(path), version=version)
            except (OSError, ValueError, subprocess.SubprocessError) as error:
                result['blockers'].append(name + ': ' + str(error))
    if args.engine_build is not None and 'engine' in result:
        compatibility = {}
        for rel in result['engine']['binaries']:
            binary = args.engine_build / rel
            checked = subprocess.run(['ldd', str(binary)], capture_output=True, text=True, timeout=10)
            compatibility[rel] = dict(exit_code=checked.returncode, stdout=checked.stdout, stderr=checked.stderr)
            if 'not found' in checked.stdout or (checked.returncode and 'not a dynamic executable' not in checked.stderr):
                result['blockers'].append('native runtime libraries unavailable: ' + rel)
        result['runtime_libraries'] = compatibility
    try:
        from corpus import SOURCE_DIGEST, SOURCE_QUERIES
        from fixtures import TARGETS
        for label in getattr(args, 'fixtures', '100000,1000000,flea-source').split(','):
            fixture = load(args.corpus_root, label)
            expected_count = {'100000':100000, '1000000':1000000, 'flea-source':4902}[label]
            expected_queries = SOURCE_QUERIES if label == 'flea-source' else TARGETS
            if fixture['info']['entries'] != expected_count or fixture['queries'] != [list(q) for q in expected_queries]:
                raise ValueError('corpus count or query contract differs: ' + label)
            if label == 'flea-source' and fixture['identity_sha256'] != SOURCE_DIGEST:
                raise ValueError('Flea source identity differs')
            result.setdefault('corpora', {})[label] = dict(entries=expected_count, identity_sha256=fixture['identity_sha256'])
    except (OSError, ValueError, KeyError) as error:
        result['blockers'].append('corpus: ' + str(error))
    if args.cache == 'cold':
        result['cold_helper'] = str(HERE / 'cold-controller.py')
    (output / 'preflight.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    return result


def run(args, output):
    from engines import Engine, process_memory
    from contracts import identities, inventory_report, quality_reason
    from priors import prepare as prior_prepare
    from metrics import collect_engine, collect_match, collect_walk, summarize
    matcher = imported('matchbench_runner', HERE.parent / 'flea-matchbench/runner.py')
    allowed = sorted(os.sched_getaffinity(0))
    if len(allowed) < 3:
        raise ValueError('two engine CPUs and one client CPU required')
    cpus = allowed[:2]
    os.sched_setaffinity(0, {allowed[2]})
    os.environ['FLEA_SEARCH_BIN'] = str(args.flea_bin)
    args.run_output = output
    helper = subprocess.Popen([sys.executable, '-B', str(HERE / 'cold-controller.py'), str(output)]) if args.cache == 'cold' and not getattr(args, 'external_cold_helper', False) else None
    records = (output / 'runs.jsonl').open('x')
    samples = []
    finished = False
    try:
        for label in args.fixtures.split(','):
            fixture = load(args.corpus_root, label)
            tree = Path(fixture['tree'])
            queries = fixture['queries']
            expected = identities(tree)
            prior_seed = prior_prepare(Path(fixture['fixture']), tree, expected, [q[2] for q in queries],
                                       build=args.engine_build, native_profiles=['bare', 'rich', 'equal-history']) if 'engine' in args.stages else None
            phases = [('pilot-a', 5), ('pilot-b', 5), ('measured', args.runs)]
            for stage in args.stages.split(','):
                entrants = ENGINE_ROSTER if stage == 'engine' else [f'{n}/{p}' for n, p in matcher.ENTRANTS] if stage == 'match' else WALK_ROSTER
                reference = entrants[0]
                for phase, count in phases:
                    for number in range(count):
                        for reference, rival, order in stage_pairs(stage, entrants, number):
                            batch = dict(stage=stage, fixture=label, phase=phase, run=number, rival=rival,
                                         cache=args.cache, reference=reference, before=host_load())
                            pair_samples = []
                            for entrant in order:
                                context = dict(stage=stage, fixture=label, phase=phase, run=number, rival=rival, entrant=entrant, reference=reference, cache=args.cache)
                                state = marked(output, stage + '-')
                                try:
                                    if stage == 'engine':
                                        sample = collect_engine(args, state, fixture, cpus, entrant, prior_seed)
                                    elif stage == 'match':
                                        sample = collect_match(args, state, fixture, cpus, entrant, matcher)
                                    else:
                                        sample = collect_walk(args, state, fixture, cpus, entrant)
                                    sample.update(context, status='ok')
                                except Exception as error:
                                    sample = dict(context, status='blocked', reason=str(error))
                                    if isinstance(error, clocks.ColdGateError):
                                        batch['above_4'] = batch['before']['above_4']
                                        for failed in pair_samples + [sample]:
                                            failed['batch'] = batch
                                            samples.append(failed)
                                            records.write(json.dumps(failed) + '\n')
                                        records.flush()
                                        raise
                                pair_samples.append(sample)
                            batch['after'] = host_load()
                            batch['above_4'] = batch['before']['above_4'] or batch['after']['above_4']
                            batch['order'] = order
                            for sample in pair_samples:
                                sample['batch'] = batch
                                samples.append(sample)
                                records.write(json.dumps(sample) + '\n')
                            records.flush()
                            print(json.dumps(dict(batch=batch, status=[s['status'] for s in pair_samples])), flush=True)
        finished = True
    finally:
        records.close()
        os.sched_setaffinity(0, set(allowed))
        completed = finished and bool(samples) and all(s['status'] == 'ok' and s.get('eligible') for s in samples)
        (output / 'completion.json').write_text(json.dumps(dict(complete=completed)) + '\n')
        if helper is not None:
            helper.wait(timeout=130)
        summarize(output, samples, args.runs)
    return 0 if completed else 2


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--corpus-root', type=Path, default=DEFAULT_ROOT)
    parser.add_argument('--output-parent', type=Path, default=REPO / '.superpowers/m9-fs-instrument')
    parser.add_argument('--fixtures', default='100000,1000000,flea-source')
    parser.add_argument('--runs', type=int, default=30)
    parser.add_argument('--stages', default='engine,match,walk')
    parser.add_argument('--cache', choices=['warm', 'cold'], default='warm')
    parser.add_argument('--external-cold-helper', action='store_true')
    parser.add_argument('--engine-build', type=Path)
    parser.add_argument('--flea-bin', type=Path)
    parser.add_argument('--matcher-bin', type=Path)
    parser.add_argument('--walker-bin', type=Path)
    parser.add_argument('--fzf', type=Path)
    parser.add_argument('--fd', type=Path)
    parser.add_argument('--rg', type=Path)
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--preflight', action='store_true')
    parser.add_argument('--build-stage', action='store_true')
    parser.add_argument('--build-rosters', action='store_true')
    parser.add_argument('--resolve-locks', action='store_true')
    parser.add_argument('--smoke', action='store_true')
    args = parser.parse_args()
    output = marked(args.output_parent.resolve(), 'run-')
    print('RUN_ROOT=' + str(output), flush=True)
    if args.resolve_locks:
        locks = {}
        for name in ['flea-matchbench', 'flea-walkbench']:
            manifest = HERE.parent / name / 'Cargo.toml'
            subprocess.run(['cargo', 'generate-lockfile', '--offline', '--manifest-path', str(manifest)], check=True)
            locks[name] = manifest.with_name('Cargo.lock').read_text()
        print('RESOLVED_LOCKS=' + json.dumps(locks), flush=True)
        if not args.build_rosters:
            return 0
    if args.smoke:
        from smoke import run as smoke
        smoke(args.flea_bin, output, sorted(os.sched_getaffinity(0))[:2])
        return 0
    if args.build_rosters:
        receipt = {}
        from artifacts import digest
        for name in ['flea-matchbench', 'flea-walkbench']:
            source = HERE.parent / name
            subprocess.run(['cargo', 'build', '--locked', '--release', '--manifest-path', str(source / 'Cargo.toml'), '--target-dir', str(output / 'roster-target')], check=True)
            binary = output / 'roster-target/release' / name
            hashes = {str(p.relative_to(source)): digest(p) for p in sorted(source.rglob('*')) if p.is_file() and p.suffix in {'.rs', '.toml'} and 'target' not in p.parts}
            receipt[name] = dict(binary=str(binary), sha256=digest(binary), sources=hashes, lock=digest(source / 'Cargo.lock'), lock_text=(source / 'Cargo.lock').read_text())
        (output / 'rosters.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt), flush=True)
        if args.prepare:
            print(json.dumps(prepare(args.corpus_root, REPO, args.fixtures.split(','))), flush=True)
        return 0
    if args.build_stage:
        builder = imported('build_stage', HERE / 'build-stage.py')
        print(json.dumps(builder.build(REPO, output)), flush=True)
        return 0
    if args.prepare:
        print(json.dumps(prepare(args.corpus_root, REPO, args.fixtures.split(','))), flush=True)
        return 0
    result = preflight(args, output)
    if args.preflight or result['blockers']:
        return 2 if result['blockers'] else 0
    if args.runs < 30:
        raise ValueError('accepted collection requires N >= 30')
    return run(args, output)


if __name__ == '__main__':
    sys.exit(main())
