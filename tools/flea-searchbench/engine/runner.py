import argparse
import json
import os
import pathlib
import platform
import statistics
import sys
import time
import uuid

from engines import Engine, cpu_ticks, process_memory, process_cpu_ns
from contracts import identities, inventory_report, quality_reason
from live import event_probe, cancellation_probe, build_cancel_probe
from presentation import materialize
from priors import prepare
from negative_controls import run_controls
from fixtures import marked_root, source_fixture, synthetic
from measure import Measurements, record_query, update_probe

LEGACY = [('flea', 'native'), ('flea-0.3.12', 'native'), ('fff', 'bare'), ('fff', 'rich'), ('flea', 'rich'), ('fsearch', 'native'),
          ('fsearch-nd', 'linux-core')]
CONFIGURATIONS = LEGACY + [(name, profile) for profile in ['fresh', 'equal-history']
                  for name in ['flea', 'flea-0.3.12', 'fff', 'fsearch', 'fsearch-nd']]
FLEA = ('flea', 'flea-0.3.12')


class ColdGateError(RuntimeError):
    pass


def cache_gate(output, mode, context):
    if mode == 'warm':
        return
    identifier = uuid.uuid4().hex
    request = output / f'cache-request-{identifier}.json'
    response = output / f'cache-response-{identifier}.json'
    if not output.is_absolute() or not (output / '.findbench-root').is_file():
        raise RuntimeError('cold request needs an absolute marked run root')
    with request.open('x') as out:
        json.dump(dict(action='drop-page-cache', mode='cold', context=context), out)
    print(f'COLD_GATE pending: {request.name}; controller must run cold-controller.py on minipc', flush=True)
    deadline = time.monotonic() + 600
    while not response.exists():
        if time.monotonic() >= deadline:
            raise ColdGateError('cold controller acknowledgement timed out')
        time.sleep(0.02)
    receipt = json.loads(response.read_text())
    if receipt.get('host') != 'minipc' or receipt.get('exit_code') != 0:
        raise ColdGateError('cold cache clear failed: ' + json.dumps(receipt))


def main(control=False):
    parser = argparse.ArgumentParser()
    parser.add_argument('--build', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--runs', type=int, default=30)
    parser.add_argument('--fixtures', default='100000,1000000,source')
    parser.add_argument('--storage', default='local', choices=['local', 'nas', 'usb'])
    parser.add_argument('--cache-mode', choices=['warm', 'cold'], default=os.environ.get('FINDBENCH_CACHE', 'warm'))
    args = parser.parse_args()
    if (args.runs < 5 and not control) or not args.output.is_absolute() or not (args.output / '.findbench-root').is_file():
        raise SystemExit('at least five measured runs and an absolute marked output root are required')
    if not control:
        run_controls(args.output)
    allowed = sorted(os.sched_getaffinity(0))
    if len(allowed) < 2:
        raise SystemExit('two available CPUs required for the primary matched budget')
    engine_cpus = allowed[:2]
    client_cpus = allowed[2:3] or allowed[:2]
    os.sched_setaffinity(0, set(client_cpus))
    fixtures = marked_root(args.output, 'fixtures-')
    rows = Measurements(args.output)
    environment = dict(kernel=platform.release(), machine=platform.machine(), engine_cpus=engine_cpus, client_cpus=client_cpus,
                       budget=2, result_cap=350, control_only=control, noise_runs_per_batch=0 if control else 5, measured_runs=args.runs,
                       storage_label=args.storage, cold_page_cache=args.cache_mode == 'cold',
                       fixture_parent_device=fixtures.stat().st_dev, monotonic_clock=time.get_clock_info('perf_counter')._asdict()
                       if hasattr(time.get_clock_info('perf_counter'), '_asdict') else vars(time.get_clock_info('perf_counter')))
    (args.output / 'environment.json').write_text(json.dumps(environment, indent=2) + '\n')
    if args.storage != 'local':
        raise SystemExit('NAS/USB needs a marked fixture root on that mount; this local harness refuses relabeling')
    completed = False
    try:
        for label in args.fixtures.split(','):
            fixture = source_fixture(fixtures) if label == 'source' else synthetic(fixtures, int(label))
            fixture_root, tree, queries, info = fixture
            print(f'fixture={label} entries={info["entries"]} root={fixture_root}', flush=True)
            expected = identities(tree)
            folders = frozenset(os.fsdecode(path) for path, kind in expected if kind == 'd')
            frozen = [[path.hex(), kind] for path, kind in expected]
            (fixture_root / 'identities.json').write_text(json.dumps(frozen) + '\n')
            reference_paths = [os.fsdecode(path) for path, _ in expected[:350]]
            prior_seed = prepare(fixture_root, tree, expected, [q[2] for q in queries], build=args.build,
                                 native_profiles=[profile for name, profile in CONFIGURATIONS if name == 'fff'])
            noise = {}
            phases = [('measured', 1)] if control else [('noise-a', 5), ('noise-b', 5), ('measured', args.runs)]
            for phase, repetitions in phases:
                for run in range(repetitions):
                    rotation = run % len(CONFIGURATIONS)
                    order = CONFIGURATIONS[rotation:] + CONFIGURATIONS[:rotation]
                    if run % 2:
                        order = list(reversed(order))
                    for name, profile in order:
                        context = dict(phase=phase, run=run, fixture=label, engine=name, profile=profile)
                        engine = None
                        try:
                            engine = Engine(name, profile, args.build, args.output, tree, [q[2] for q in queries], engine_cpus, prior_seed=prior_seed)
                            if engine.native_priors is not None:
                                for metric in ['replay_us', 'observed_replay_us', 'copy_us']:
                                    rows.row(dict(context, work='fixture-prior-setup'), 'native_prior_' + metric,
                                             engine.native_priors[metric], 'us', note='fixture seed replay and copying precede process launch and both index clocks')
                            prior_load = engine.prior_load
                            for metric in ['xbel_us', 'zoxide_us', 'load_us'] if prior_load else []:
                                value = prior_load.get(metric)
                                rows.row(dict(context, work='prior-load-readiness'), 'prior_' + metric, value, 'us', 'ok' if value is not None else 'missing',
                                         'backend-start prior load, waited on before the cache gate and both index clocks' if value is not None else 'provider finished after the load deadline or never ran')
                            cache_gate(args.output, args.cache_mode, context)
                            ticks = cpu_ticks(engine.process.pid)
                            index_cpu = process_cpu_ns(engine.process.pid)
                            index = engine.open()
                            rows.row(context, 'index_cpu_us', (process_cpu_ns(engine.process.pid) - index_cpu) / 1000, 'us')
                            index_ticks_ms = (cpu_ticks(engine.process.pid) - ticks) * 1000 / os.sysconf('SC_CLK_TCK')
                            coverage = inventory_report(expected, engine.audit(), index['details'])
                            (engine.state / 'inventory.json').write_text(json.dumps(coverage, indent=2) + '\n')
                            if not coverage['eligible']:
                                rows.invalidate(context, '; '.join(coverage['reasons']))
                            priors_supported = name == 'fff' or (name in FLEA and index['details'].get('d10_priors') is True)
                            if profile in ('equal-history', 'rich') and not priors_supported:
                                rows.row(context, 'ranking_priors', None, 'supported', 'unsupported', 'engine has no advertised D10 provider integration')
                                rows.invalidate(context, profile + ' prior consumption unsupported')
                            if profile == 'equal-history' and not engine.priors['zoxide']['supported']:
                                rows.invalidate(context, 'zoxide replay unavailable')
                            rows.row(context, 'index_build_observed_ms', index['observed_ms'], 'ms')
                            rows.row(context, 'index_build_native_ms', index['native_ms'], 'ms')
                            rows.row(context, 'index_cpu_ticks_ms', index_ticks_ms, 'ms', 'coarse')
                            indexed = index['indexed']
                            rows.row(context, 'indexed_entries', indexed, 'entries', 'ok' if indexed == info['entries'] else 'quality-fail',
                                     f'fixture expected {info["entries"]}; no cap normalization')
                            for metric in ['rss', 'pss']:
                                rows.row(context, metric + '_after_build_bytes', index['memory'][metric], 'bytes')
                                rows.row(context, metric + '_baseline_bytes', engine.baseline_memory[metric], 'bytes')
                                rows.row(context, metric + '_delta_per_indexed_entry',
                                         (index['memory'][metric] - engine.baseline_memory[metric]) / max(1, indexed), 'bytes/entry')
                            (engine.state / 'index.json').write_text(json.dumps(index, indent=2) + '\n')
                            pings = [engine.ping() for _ in range(20)]
                            for value in pings:
                                rows.row(context, 'common_pong_ms', value, 'ms')
                            if name in FLEA:
                                for _ in range(20):
                                    empty = engine.empty_window()
                                    rows.row(context, 'empty_window_ms', empty, 'ms')
                                    rows.row(dict(context, work='empty-window'), 'zero_work_round_trip_ms', empty, 'ms', note='historical product request, distinct from common pong')
                            else:
                                for value in pings:
                                    rows.row(dict(context, work='pong'), 'zero_work_round_trip_ms', value, 'ms', note='historical pong, distinct from Flea empty-window')
                            for reuse in ['first', 'repeat']:
                                presentation = materialize(tree, reference_paths)
                                for metric, value in presentation.items():
                                    if metric == 'sha256':
                                        continue
                                    rows.row(dict(context, query_kind='common-materialization-' + reuse),
                                             'materialization_' + metric, value, 'bytes' if metric == 'bytes' else 'rows' if metric == 'rows' else 'us',
                                             note='same frozen raw identities and fields; separate from product-rich rows')
                            if phase.startswith('noise'):
                                noise.setdefault((phase, name, profile), []).extend(pings)
                            if name == 'fff':
                                rows.row(context, 'git_status_toggle', None, 'supported', 'unsupported',
                                         'public API cannot disable Git discovery/status; has_git=' + str(index['details']['has_git']))
                                if index['details']['has_git']:
                                    rows.invalidate(context, 'Git discovered in no-Git fixture')
                                    raise RuntimeError('Git discovered in no-Git primary fixture; reject changed corpus policy')
                            trace_ticks = cpu_ticks(engine.process.pid)
                            trace_cpu = process_cpu_ns(engine.process.pid)
                            traces = []
                            for kind, text, target in queries:
                                engine.query('zz_no_match_9b2d7e61')
                                for prefix in range(1, len(text) + 1):
                                    typed = text[:prefix]
                                    query_context = dict(context, query_kind=kind, prefix=prefix, query=typed)
                                    result = engine.query(typed)
                                    traces.append((query_context, result, target, kind, typed))
                            rows.row(context, 'typed_trace_cpu_ticks_ms', (cpu_ticks(engine.process.pid) - trace_ticks) * 1000 / os.sysconf('SC_CLK_TCK'), 'ms', 'coarse')
                            rows.row(context, 'typed_trace_cpu_us', (process_cpu_ns(engine.process.pid) - trace_cpu) / 1000, 'us')
                            for query_context, result, target, kind, typed in traces:
                                complete = engine.audit_rank(typed, target, info['entries'] + 16)
                                engine.verify_head(result, complete)
                                full_rank = complete['rank']
                                audit = [target] if full_rank > 0 else []
                                if target in result['paths'] and full_rank != result['paths'].index(target) + 1:
                                    rows.invalidate(context, 'head rank differs from complete audit')
                                if target not in result['paths'] and 0 < full_rank <= len(result['paths']):
                                    rows.invalidate(context, 'head omits a target ranked inside the returned head')
                                record_query(rows, query_context, result, target, audit=audit, folders=folders)
                                rows.row(query_context, 'prefix_full_target_rank', full_rank, 'rank',
                                         'ok' if full_rank else 'miss')
                            for kind, text, target in queries:
                                full = engine.query(text, limit=info['entries'] + 16)
                                complete = engine.audit_rank(text, target, info['entries'] + 16)
                                engine.verify_head(full, complete)
                                audit_context = dict(context, query_kind=kind + '-rank-audit', prefix=len(text), query=text)
                                record_query(rows, audit_context, full, target, full_rank=True, folders=folders)
                                reason = quality_reason(kind, full['paths'], target)
                                if reason:
                                    rows.invalidate(context, kind + ': ' + reason)
                                (engine.state / f'{kind}-final.json').write_text(json.dumps(dict(
                                    query=text, target=target, total_matches=full['n'], rank=full['paths'].index(target) + 1
                                    if target in full['paths'] else 0, returned=len(full['paths']), first_20=full['paths'][:20]), indent=2) + '\n')
                            for location, directory in [('root', tree), ('deep', (tree / queries[0][2]).parent)]:
                                update_probe(engine, rows, dict(context, query_kind='update-' + location), fixture_root, directory)
                                event_probe(engine, rows, dict(context, query_kind='event-only-' + location), fixture_root, directory)
                            final_memory = process_memory(engine.process.pid)
                            for metric, value in final_memory.items():
                                rows.row(context, metric + '_after_queries_updates_bytes', value, 'bytes')
                            cancellation_probe(engine, rows, dict(context, query_kind='cancellation'), queries[0][1])
                        except Exception as error:
                            rows.invalidate(context, 'execution failed: ' + str(error))
                            rows.row(context, 'execution', None, 'status', 'blocked', str(error))
                            print(f'BLOCKED {label} {phase} {run} {name}/{profile}: {error}', flush=True)
                            if isinstance(error, ColdGateError):
                                raise
                        finally:
                            if engine:
                                completion = engine.close()
                                if completion['exit_code'] != 0:
                                    rows.invalidate(context, 'nonzero steady process exit')
                                rows.row(context, 'steady_process_total_cpu_ms', completion['cpu_ms'], 'ms',
                                         'ok' if completion['exit_code'] == 0 else 'blocked', 'includes startup, indexing, queries, audits and updates')
                                rows.row(dict(context, query_kind='steady'), 'process_total_cpu_ms', completion['cpu_ms'], 'ms',
                                         'ok' if completion['exit_code'] == 0 else 'blocked', 'historical metric with an explicit workload')
                                rows.row(context, 'process_exit_code', completion['exit_code'], 'code',
                                         'ok' if completion['exit_code'] == 0 else 'blocked')
                        startup = None
                        try:
                            startup = Engine(name, profile, args.build, args.output, tree, [q[2] for q in queries], engine_cpus, prior_seed=prior_seed)
                            cache_gate(args.output, args.cache_mode, dict(context, query_kind='startup-overlap'))
                            result = startup.startup(queries[0][1])
                            startup_context = dict(context, query_kind='startup-overlap', query=queries[0][1])
                            complete = startup.audit_rank(queries[0][1], queries[0][2], info['entries'] + 16)
                            startup.verify_head(result, complete)
                            reason = quality_reason(queries[0][0], result['paths'], queries[0][2])
                            if reason:
                                rows.invalidate(context, 'startup: ' + reason)
                            coverage = inventory_report(expected, startup.audit(), startup.ready)
                            if not coverage['eligible']:
                                rows.invalidate(context, 'startup: ' + '; '.join(coverage['reasons']))
                            rows.row(startup_context, 'open_to_first_result_ms', result['first_ms'], 'ms',
                                     'ok' if result['first_ms'] is not None else 'no-result')
                            rows.row(startup_context, 'open_to_full_returned_list_ms', result['full_ms'], 'ms')
                            rows.row(startup_context, 'partial_answers', result['partial_answers'], 'answers')
                            record_query(rows, startup_context, result, queries[0][2], folders=folders)
                        except Exception as error:
                            rows.invalidate(context, 'startup execution failed: ' + str(error))
                            rows.row(dict(context, query_kind='startup-overlap'), 'execution', None, 'status', 'blocked', str(error))
                            if isinstance(error, ColdGateError):
                                raise
                        finally:
                            if startup:
                                completion = startup.close()
                                if completion['exit_code'] != 0:
                                    rows.invalidate(context, 'nonzero startup process exit')
                                rows.row(dict(context, query_kind='startup-overlap'), 'process_total_cpu_ms', completion['cpu_ms'], 'ms',
                                         'ok' if completion['exit_code'] == 0 else 'blocked', 'historical metric with an explicit workload')
                                rows.row(dict(context, query_kind='startup-overlap'), 'startup_process_total_cpu_ms', completion['cpu_ms'], 'ms',
                                         'ok' if completion['exit_code'] == 0 else 'blocked')
                        if name in FLEA:
                            cancellation = None
                            cancel_context = dict(context, query_kind='queued-build-cancellation')
                            try:
                                cancellation = Engine(name, profile, args.build, args.output, tree, [q[2] for q in queries], engine_cpus, prior_seed=prior_seed)
                                build_cancel_probe(cancellation, rows, cancel_context, queries[0][1])
                            except Exception as error:
                                rows.invalidate(context, 'queued build cancellation failed: ' + str(error))
                                rows.row(cancel_context, 'execution', None, 'status', 'blocked', str(error))
                            finally:
                                if cancellation:
                                    completion = cancellation.close()
                                    if completion['exit_code'] != 0:
                                        rows.invalidate(context, 'nonzero cancellation process exit')
                                    rows.row(cancel_context, 'cancellation_process_total_cpu_ms', completion['cpu_ms'], 'ms',
                                             'ok' if completion['exit_code'] == 0 else 'blocked')
                if phase == 'noise-b':
                    for name, profile in CONFIGURATIONS:
                        a = noise.get(('noise-a', name, profile), [])
                        b = noise.get(('noise-b', name, profile), [])
                        if a and b:
                            med_a, med_b = statistics.median(a), statistics.median(b)
                            ratio = max(med_a, med_b) / max(1e-9, min(med_a, med_b))
                            if ratio > 1.2:
                                print(f'UNSTABLE NOISE {label} {name}/{profile}: median ratio={ratio:.3f}; preserve results, repeat quiet run', flush=True)
                                rows.row(dict(phase='measured', run='noise-check', fixture=label, engine=name, profile=profile),
                                         'noise_batch_median_ratio', ratio, 'ratio', 'unstable')
            print(f'completed fixture={label}', flush=True)
        completed = True
    finally:
        rows.finish(completed)
    return 2 if rows.blocked else 0


if __name__ == '__main__':
    sys.exit(main())
