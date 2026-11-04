"""Process-run reductions and paired, run-clustered bootstrap summaries."""
import hashlib
import json
import math
import os
from pathlib import Path
import random
import statistics
import subprocess
import time

from cold import cache_gate
from fair.eligibility import compare


def p95(values):
    return sorted(values)[math.ceil(.95 * len(values)) - 1]


def descendants(pid):
    result, pending = [], [pid]
    while pending:
        current = pending.pop()
        if current in result:
            continue
        result.append(current)
        for task in Path(f'/proc/{current}/task').iterdir():
            pending.extend(map(int, (task / 'children').read_text().split()))
    return result


def process_tree_memory(pid):
    from engines import process_memory
    total = dict(pss=0, rss=0)
    for child in descendants(pid):
        for key, value in process_memory(child).items():
            total[key] += value
    return total


def validate_match_head(result, limit):
    from contracts import checked_result
    checked_result(result, result['gen'], limit)


def collect_engine(args, state, fixture, cpus, entrant, prior_seed):
    from engines import Engine
    from contracts import identities, inventory_report, quality_reason
    name, profile = entrant.split('/')
    engine = Engine(name, profile, args.engine_build, state, Path(fixture['tree']), [q[2] for q in fixture['queries']], cpus, prior_seed=prior_seed)
    try:
        cache_gate(args.run_output, args.cache, dict(stage='engine', state=str(state), entrant=entrant))
        index = engine.open()
        coverage = inventory_report(identities(Path(fixture['tree'])), engine.audit(), index['details'])
        timings, native, quality, audits = [], [], [], []
        for kind, text, target in fixture['queries']:
            engine.query('zz_no_match_9b2d7e61')
            final = None
            for prefix in range(1, len(text) + 1):
                result = engine.query(text[:prefix])
                timings.append(result['full_ms'])
                if result.get('comparable_core_ms') is not None:
                    native.append(result['comparable_core_ms'])
                final = result
            complete = engine.audit_rank(text, target, fixture['info']['entries'] + 16)
            engine.verify_head(final, complete)
            rank = final['paths'].index(target) + 1 if target in final['paths'] else 0
            consistent = (not rank or rank == complete['rank']) and not (0 < complete['rank'] <= len(final['paths']) and not rank)
            audits.append(dict(kind=kind, rank=rank, full_rank=complete['rank'], head_consistent=consistent, quality_error=quality_reason(kind, final['paths'], target)))
            quality.append(1 / rank if rank else 0)
        sample = dict(index_ms=index['observed_ms'], native_index_ms=index['native_ms'],
                      query_p50_ms=statistics.median(timings), query_p95_ms=p95(timings),
                      memory_bytes=index['memory']['pss'], memory_boundary='process PSS at ready',
                      coverage=coverage, quality_mrr=statistics.mean(quality), quality_audits=audits,
                      coverage_exact=int(coverage['eligible']), quality_pass=all(not a['quality_error'] for a in audits),
                      eligible=coverage['eligible'] and all(a['head_consistent'] for a in audits))
        if native:
            sample['native_query_p50_ms'] = statistics.median(native)
        return sample
    finally:
        completion = engine.close()
        if completion['exit_code']:
            raise RuntimeError('engine exited ' + str(completion))


def collect_match(args, state, fixture, cpus, entrant, matcher):
    from engines import process_memory
    name, profile = entrant.split('/')
    engine = matcher.Entrant(name, profile, args.matcher_bin, args.fzf, cpus, 2, state)
    try:
        cache_gate(args.run_output, args.cache, dict(stage='match', state=str(state), entrant=entrant))
        index = engine.open(Path(fixture['paths']))
        memory = process_tree_memory(engine.process.pid)
        timings, native, quality, audits = [], [], [], []
        for kind, text, target in fixture['queries']:
            engine.query(matcher.NO_MATCH)
            for prefix in range(1, len(text) + 1):
                result = engine.query(text[:prefix])
                timings.append(result['observed_ms'])
                validate_match_head(result, 350)
                native.append(result['core_ms'])
            audit = engine.audit(text, target)
            rank = result['paths'].index(target) + 1 if target in result['paths'] else 0
            consistent = (not rank or rank == audit['rank']) and not (0 < audit['rank'] <= len(result['paths']) and not rank)
            quality.append(1 / rank if rank else 0)
            audits.append(dict(kind=kind, rank=rank, full_rank=audit['rank'], head_consistent=consistent))
        return dict(index_ms=index['observed_ms'], native_index_ms=index['build_ms'],
                    query_p50_ms=statistics.median(timings), query_p95_ms=p95(timings),
                    native_query_p50_ms=statistics.median(native), memory_bytes=memory['pss'],
                    memory_boundary='process tree PSS at ready, includes driver, matcher and path corpus',
                    coverage=dict(eligible=index['n'] == fixture['info']['entries'], expected=fixture['info']['entries'], observed=index['n'], boundary='matcher path count; no filesystem census'),
                    quality_mrr=statistics.mean(quality), quality_audits=audits,
                    coverage_exact=int(index['n'] == fixture['info']['entries']),
                    eligible=index['n'] == fixture['info']['entries'] and all(a['head_consistent'] for a in audits))
    finally:
        code, _ = engine.close()
        if code:
            raise RuntimeError('matcher exited ' + str(code))


def collect_walk(args, state, fixture, cpus, entrant):
    cache_gate(args.run_output, args.cache, dict(stage='walk', state=str(state), entrant=entrant))
    argv = ['taskset', '--cpu-list', ','.join(map(str, cpus))]
    if entrant == 'flea-search':
        done = subprocess.run(argv + [str(args.flea_bin), '--search-stage', 'walk', fixture['tree']], check=True, capture_output=True, timeout=3600)
        result = json.loads(done.stdout)
        from corpus import identity_digest
        digest = identity_digest([(bytes.fromhex(raw), kind) for raw, kind in result['identities']])
        elapsed = result['elapsed_ns'] / 1e6
        memory, memory_boundary = result['pss'], 'product PSS after compressed index build'
        files, dirs, errors = result['files'], result['dirs'], result['errors']
    else:
        argv += [str(args.walker_bin), 'adapter', entrant, '--tree', fixture['tree'], '--mode', 'index', '--threads', '2']
        tool = args.fd if entrant == 'fd' else args.rg if entrant == 'ripgrep' else None
        if tool:
            argv += ['--tool', str(tool)]
        done = subprocess.run(argv, check=True, capture_output=True, text=True, timeout=3600)
        fields = done.stdout.strip().split('\t')
        if fields[0] != 'RESULT' or len(fields) != 10:
            raise ValueError('walker result schema differs')
        _, elapsed, files, dirs, errors, digest, cpu, hwm, own_peak, tool_peak = fields
        elapsed = int(elapsed) / 1e6
        files, dirs, errors = int(files), int(dirs), int(errors)
        memory = None
        memory_boundary = 'rival exposes peak RSS only; steady PSS unavailable after one-shot exit'
    from corpus import identity_digest
    manifest = json.loads((Path(fixture['fixture']) / 'manifest.json').read_text())
    if entrant == 'ripgrep':
        manifest = [(path, kind) for path, kind in manifest if kind == 'f']
    expected_digest = identity_digest(manifest)
    coverage = dict(eligible=digest == expected_digest and errors == 0, expected=len(manifest), observed=files + dirs,
                    identity_sha256=digest, expected_sha256=expected_digest, errors=errors,
                    boundary='files-only census' if entrant == 'ripgrep' else 'files and directories')
    return dict(index_ms=elapsed, memory_bytes=memory, memory_boundary=memory_boundary, coverage=coverage,
                comparable=entrant != 'ripgrep', coverage_exact=int(coverage['eligible'] and entrant != 'ripgrep'),
                eligible=coverage['eligible'])


def interval(values, seed=20261025, resamples=10000):
    if not values:
        return None
    rng = random.Random(seed)
    estimates = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(resamples))
    return dict(n=len(values), median=statistics.median(values), low=estimates[int(.025 * resamples)], high=estimates[int(.975 * resamples) - 1])


def summarize(output, samples, required):
    required = max(30, required)
    columns = ['index_ms', 'query_p50_ms', 'query_p95_ms', 'quality_mrr', 'coverage_exact', 'memory_bytes']
    summary = []
    keys = sorted({(s['stage'], s['fixture'], s['cache'], s['rival']) for s in samples})
    for stage, fixture, cache, rival in keys:
        population = [s for s in samples if (s['stage'], s['fixture'], s['cache'], s['rival']) == (stage, fixture, cache, rival)]
        group = [s for s in population if s['phase'] == 'measured']
        if not group:
            continue
        reference = group[0].get('reference') or ('flea/native' if stage == 'engine' else 'flea-search/native' if stage == 'match' else 'flea-search')
        for metric in columns:
            by_run = {}
            performance = metric not in {'quality_mrr', 'coverage_exact'}
            unit = 'bytes' if metric == 'memory_bytes' else 'ms'
            rows = []
            for sample in population:
                if (sample['status'] == 'ok' and sample.get('eligible') and isinstance(sample.get(metric), (int, float))
                        and (not performance or (sample.get('quality_pass', True) and sample.get('comparable', True)))):
                    if sample['phase'] == 'measured':
                        by_run.setdefault(sample['run'], {}).setdefault(sample['entrant'], []).append(sample[metric])
                    floor = 1024 if unit == 'bytes' else .000001
                    resolution = sample.get('resolutions', {}).get(metric)
                    if resolution is not None and (not math.isfinite(resolution) or resolution <= 0):
                        raise ValueError(f"searchbench {sample['entrant']} {metric} {sample['phase']} run {sample['run']}: invalid instrument resolution {resolution!r}")
                    rows.append(dict(engine=sample['entrant'], phase={'pilot-a': 'noise-a', 'pilot-b': 'noise-b'}.get(sample['phase'], sample['phase']),
                                     run=sample['run'], fixture=fixture, profile='', metric=metric, unit=unit,
                                     value=sample[metric], status='ok', eligible='yes', quality='ok',
                                     resolution=max(floor, resolution) if resolution is not None else floor))
            by_run = {run: {entrant: statistics.median(values) for entrant, values in entrants.items()} for run, entrants in by_run.items()}
            pairs = [v for v in by_run.values() if reference in v and rival in v]
            a, b = [v[reference] for v in pairs], [v[rival] for v in pairs]
            delta = [v[reference] - v[rival] for v in pairs]
            ci = interval(delta)
            winner = 'unproven'
            admitted = compare(rows, reference, rival, minimum=required) if performance else []
            admission = admitted[0] if admitted else dict(verdict='non-performance-metric' if not performance else 'insufficient-pairs')
            if admission['verdict'] in {'win', 'loss', 'tie-or-unresolved'}:
                winner = {'win': reference, 'loss': rival, 'tie-or-unresolved': 'overlap'}[admission['verdict']]
            summary.append(dict(stage=stage, fixture=fixture, cache=cache, rival=rival, metric=metric,
                                reference=reference, reference_ci=interval(a), rival_ci=interval(b), difference_ci=ci,
                                winner=winner, admission=admission['verdict'], noise_threshold=admission.get('noise_threshold'),
                                above_4_pairs=sum(s['batch']['above_4'] for s in group if s['entrant'] == reference)))
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    return summary
