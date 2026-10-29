import collections
import csv
import json
import math
import os
import pathlib
import statistics
import time
import uuid

from contracts import classify_file_rank, classify_rank

from fixtures import checked_mutation

COLUMNS = ['phase', 'run', 'fixture', 'engine', 'profile', 'query_kind', 'prefix', 'query',
           'metric', 'value', 'unit', 'resolution', 'status', 'note', 'work', 'quality', 'eligible', 'ineligible_reason']


class Measurements:
    def __init__(self, output):
        self.output = pathlib.Path(output)
        if not self.output.is_absolute() or not (self.output / '.findbench-root').is_file():
            raise ValueError('measurements require an absolute marked root')
        self.file = (self.output / 'pending-measurements.tsv').open('x', newline='')
        self.writer = csv.DictWriter(self.file, fieldnames=COLUMNS, delimiter='\t', lineterminator='\n')
        self.writer.writeheader()
        self.eligibility = {}
        self.groups = collections.defaultdict(list)
        self.quality_failures = 0
        self.blocked = 0

    def row(self, context, metric, value, unit, status='ok', note=''):
        row = dict.fromkeys(COLUMNS, '')
        row.update(context, metric=metric, value='' if value is None else value, unit=unit, status=status, note=note, eligible='pending')
        if not row['work']:
            product_metrics = {'first_result_ms', 'full_returned_list_ms', 'open_to_first_result_ms',
                               'open_to_full_returned_list_ms', 'cpu_per_key_ticks_ms', 'cpu_us', 'client_cpu_us', 'decode_cpu_us',
                               'typed_trace_cpu_us', 'typed_trace_cpu_ticks_ms', 'steady_process_total_cpu_ms',
                               'startup_process_total_cpu_ms', 'process_total_cpu_ms', 'core_reply_us', 'response_bytes'}
            row['work'] = ('product-rich' if context.get('engine') in ('flea', 'flea-0.3.12') else 'names-only') if metric in product_metrics else 'common'
        if unit in {'us', 'ms'}:
            row['resolution'] = 0.001 if unit == 'us' else 0.000001
            if 'ticks' in metric:
                row['resolution'] = (1000000 if unit == 'us' else 1000) / os.sysconf('SC_CLK_TCK')
            if context.get('engine') == 'fsearch' and metric in {'match_select_paths_us', 'native_core_ms'}:
                row['resolution'] = 1 if unit == 'us' else 0.001
        self.writer.writerow(row)
        self.file.flush()
        if status == 'quality-fail':
            self.quality_failures += 1
            self.invalidate(context, metric + ': ' + note)
        if status == 'blocked':
            self.blocked += 1

    def invalidate(self, context, reason):
        key = tuple(str(context.get(k, '')) for k in ['phase', 'run', 'fixture', 'engine', 'profile'])
        self.eligibility.setdefault(key, []).append(reason)

    def finish(self, completed):
        self.file.close()
        self.groups.clear()
        with (self.output / 'pending-measurements.tsv').open() as source, (self.output / 'measurements.tsv').open('x', newline='') as out:
            writer = csv.DictWriter(out, fieldnames=COLUMNS, delimiter='\t', lineterminator='\n')
            writer.writeheader()
            for row in csv.DictReader(source, delimiter='\t'):
                key = tuple(str(row.get(k, '')) for k in ['phase', 'run', 'fixture', 'engine', 'profile'])
                reasons = self.eligibility.get(key, [])
                row['eligible'] = 'no' if reasons else 'yes'
                row['ineligible_reason'] = '; '.join(sorted(set(reasons)))
                writer.writerow(row)
                if row['phase'] == 'measured':
                    group = tuple(row[k] for k in ['fixture', 'engine', 'profile', 'query_kind', 'prefix', 'query', 'metric', 'unit', 'work', 'quality', 'status', 'eligible'])
                    self.groups[group].append(None if row['value'] in ('', None) else float(row['value']))
        fields = ['fixture', 'engine', 'profile', 'query_kind', 'prefix', 'query', 'metric', 'unit', 'work', 'quality', 'status', 'eligible',
                  'observations', 'numeric_n', 'min', 'p50', 'p95', 'max']
        for filename, eligible_only in [('summary.tsv', False), ('performance.tsv', True)]:
            with (self.output / filename).open('x', newline='') as out:
                writer = csv.writer(out, delimiter='\t', lineterminator='\n')
                writer.writerow(fields)
                for key, observations in sorted(self.groups.items(), key=lambda item: tuple(map(str, item[0]))):
                    if eligible_only and (key[-1] != 'yes' or key[-2] not in {'ok', 'coarse'} or key[-5] not in {'ms', 'us', 'bytes', 'bytes/entry'}):
                        continue
                    numbers = sorted(v for v in observations if v is not None)
                    stats = ['', '', '', ''] if not numbers else [numbers[0], statistics.median(numbers),
                            numbers[math.ceil(0.95 * len(numbers)) - 1], numbers[-1]]
                    writer.writerow([*key, len(observations), len(numbers), *stats])
        (self.output / 'eligibility.json').write_text(json.dumps([
            dict(zip(['phase', 'run', 'fixture', 'engine', 'profile'], key), reasons=reasons)
            for key, reasons in sorted(self.eligibility.items())], indent=2) + '\n')
        (self.output / 'completion.json').write_text(json.dumps(dict(
            completed=completed, blocked_samples=self.blocked, quality_failure_rows=self.quality_failures,
            ineligible_instances=len(self.eligibility),
            meaning='collection completion is not quality approval; only performance.tsv filters ineligible instances'), indent=2) + '\n')


def record_query(rows, context, result, target, full_rank=False, audit=None, *, folders):
    if result.get('n') is None and not result.get('head_verified'):
        raise ValueError('result head lacks complete match-count evidence')
    rank, rank_status = classify_rank(result['paths'], target, audit, complete=full_rank or (result.get('n') == len(result['paths'])))
    context = dict(context, quality=rank_status)
    first_status = 'ok' if result['first_ms'] is not None else 'no-result'
    rows.row(context, 'first_result_ms', result['first_ms'], 'ms', first_status)
    rows.row(context, 'full_returned_list_ms', result['full_ms'], 'ms', first_status)
    if result.get('walk_ms') is not None:
        rows.row(dict(context, work='walk-open'), 'startup_walk_ms', result['walk_ms'], 'ms')
    if result.get('core_ms') is not None:
        rows.row(dict(context, work='native-' + context['engine']), 'native_core_ms', result['core_ms'], 'ms')
    if result.get('comparable_core_ms') is not None:
        rows.row(context, 'match_select_paths_us', result['comparable_core_ms'] * 1000, 'us')
    if 'cpu_ms_ticks' in result:
        rows.row(context, 'cpu_per_key_ticks_ms', result['cpu_ms_ticks'], 'ms', 'coarse', 'kernel process tick resolution')
    paths = result['paths']
    rank, status = classify_rank(paths, target, audit, complete=full_rank or (result.get('n') == len(paths)))
    rows.row(context, 'intended_rank_full' if full_rank else 'intended_rank_top_k', rank, 'rank', status,
             'censored means found beyond head; miss means absent from full audit; unresolved awaits audit')
    file_rank, file_status = classify_file_rank(paths, folders, target, audit, complete=full_rank or (result.get('n') == len(paths)))
    rows.row(context, 'intended_file_rank_full' if full_rank else 'intended_file_rank_top_k', file_rank, 'rank', file_status,
             'rank among file rows; folders ahead of the target are not counted')
    rows.row(context, 'target_reciprocal_rank', 1 / rank if rank > 0 else 0, 'ratio', status)
    rows.row(context, 'target_recall_at_k', int(target in paths), 'ratio', status)
    for metric in ['cpu_us', 'client_cpu_us', 'decode_cpu_us', 'response_bytes', 'core_reply_us']:
        if result.get(metric) is not None:
            rows.row(context, metric, result[metric], 'bytes' if metric == 'response_bytes' else 'us')
    rows.row(context, 'returned_count', len(paths), 'entries')
    rows.row(context, 'total_matches', result['n'], 'entries', 'ok' if result['n'] is not None else 'unsupported',
             'fsearch-nd exposes only top-k results, not total-match count' if result['n'] is None else '')


def update_probe(engine, rows, context, fixture_root, directory, timeout=3.0):
    token = 'zz_findbench_' + uuid.uuid4().hex
    old = checked_mutation(fixture_root, directory / (token + '_old.txt'))
    new = checked_mutation(fixture_root, directory / (token + '_new.txt'))
    old_rel, new_rel = str(old.relative_to(engine.tree)), str(new.relative_to(engine.tree))
    recursive = ((engine.ready or {}).get('watch_coverage') == 'recursive' and (engine.ready or {}).get('watch_healthy') is True)
    supported = engine.name in {'fff', 'fsearch'} or (engine.name == 'flea' and (directory == engine.tree or recursive))
    owned = None
    try:
        for action in ['create', 'rename', 'delete']:
            before = engine.query(token)['paths']
            prior_visible = action == 'create' or (old_rel if action == 'rename' else new_rel) in before
            at = time.perf_counter_ns()
            if action == 'create':
                with checked_mutation(fixture_root, old).open('xb'):
                    pass
                owned = old
                present, absent = old_rel, new_rel
            elif action == 'rename':
                checked_mutation(fixture_root, old)
                checked_mutation(fixture_root, new)
                if new.exists():
                    raise RuntimeError('rename destination already exists')
                old.rename(new)
                owned = new
                present, absent = new_rel, old_rel
            else:
                checked_mutation(fixture_root, new).unlink()
                owned = None
                present, absent = None, new_rel
            metric = 'update_' + action + '_ms'
            if not supported:
                rows.row(context, metric, None, 'ms', 'unsupported',
                         'no recursive Flea search watch' if engine.name == 'flea' else 'FSEvents unavailable in Linux name shim')
                if engine.name == 'fsearch-nd':
                    relist_ms = engine.relist_nd(directory)
                    after = engine.query(token)['paths']
                    visible = (present is None or present in after) and absent not in after
                    rows.row(context, 'explicit_relist_' + action + '_ms', relist_ms, 'ms',
                             'ok' if visible and prior_visible else 'quality-fail', 'explicit call, not automatic watcher latency')
                continue
            if not prior_visible:
                rows.invalidate(context, 'mutation lacked prior visibility')
                rows.row(context, metric, None, 'ms', 'quality-fail', 'file was not visible before mutation; zero latency would be false')
                continue
            deadline = time.monotonic() + timeout
            observed = None
            while time.monotonic() < deadline:
                paths = engine.query(token)['paths']
                if (present is None or present in paths) and absent not in paths:
                    observed = (time.perf_counter_ns() - at) / 1e6
                    break
                time.sleep(0.001)
            if observed is None:
                rows.invalidate(context, 'mutation did not reach current index: ' + action)
            rows.row(context, metric, observed, 'ms', 'ok' if observed is not None else 'quality-fail',
                     'poll-observed upper bound; 1 ms sleeps plus query cost; timeout 3 s')
    finally:
        if owned is not None:
            checked_mutation(fixture_root, owned).unlink(missing_ok=True)
