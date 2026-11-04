"""Eligibility is applied after collection, so later failures invalidate earlier timings."""
import hashlib
import os
import stat
import json
import pathlib
import collections
import csv
import math
import random
import statistics

# R2 manifest bc984d45 plus the 0ebbcffae seam port, the 0.3.12 anchor engine and current rival pins.
MANIFEST_SHA256 = 'e654d92711a599e7588cf69ecd7accaed2cf51ceeb8229c60c5b6f01f3f91c36'
SOURCE_IDENTITY_SHA256 = 'fc20295376062c159646bbb858c03409e85fe0a489b994956b125cc9a2e038b6'
SOURCE_CONTENT_SHA256 = '040405c921f13114b582fbd266a2801cfcc8717574e3ef1c3d9a82a492d2a7f4'
SOURCE_COMMITS = {'fff': 'd978416f77aec3f9928d56e00c9e56ba39ca1fda',
                  'fsearch': 'cb6cd67f9ef12fedcf6a160b6a38faf492f7c989'}
SOURCE_QUERIES = [
    ['exact', 'fsearch_database_scan.c', 'fsearch/src/fsearch_database_scan.c'],
    ['typo', 'fsearch_databsae_scan.c', 'fsearch/src/fsearch_database_scan.c'],
    ['path', 'fff-core/src/file_picker', 'fff/crates/fff-core/src/file_picker.rs'],
    ['extension', '.rs', 'fff/crates/fff-core/src/file_picker.rs'],
    ['deep', 'fff/crates/fff-core/src/watch', 'fff/crates/fff-core/src/watcher/background_watcher.rs'],
]
ORIGINAL_COLUMNS = ['phase', 'run', 'fixture', 'engine', 'profile', 'query_kind', 'prefix',
                    'query', 'metric', 'value', 'unit', 'status', 'note']


POLICY = 'visible; no ignores; no directory-symlink descent; root excluded'


def identities(tree):
    root = os.fsencode(tree)
    pending, result = [root], []
    while pending:
        directory = pending.pop()
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.name.startswith(b'.'):
                    continue
                kind = 'd' if entry.is_dir(follow_symlinks=False) else 'f'
                result.append((os.path.relpath(entry.path, root), kind))
                if kind == 'd':
                    pending.append(entry.path)
    return sorted(result)


def digest(items):
    value = hashlib.sha256()
    for path, kind in sorted(items):
        raw = os.fsencode(path)
        value.update(len(raw).to_bytes(8, 'little'))
        value.update(raw)
        value.update(kind.encode('ascii'))
    return value.hexdigest()


def inventory_report(expected, observed, details):
    reasons = []
    if details.get('capped'):
        reasons.append('capped index')
    if details.get('partial') or details.get('complete') is False:
        reasons.append('partial index: ' + str(details.get('reason', 'unspecified')))
    if len(observed) != len(set(observed)):
        reasons.append('duplicate raw identities')
    if sorted(observed) != sorted(expected):
        reasons.append('raw identity inventory differs')
    if details.get('n') != len(expected):
        reasons.append('indexed count differs')
    if details.get('files') is not None and details['files'] != sum(kind == 'f' for _, kind in expected):
        reasons.append('indexed file/type count differs')
    return dict(eligible=not reasons, reasons=reasons, expected=len(expected), observed=len(observed),
                expected_sha256=digest(expected), observed_sha256=digest(observed), policy=POLICY,
                directories=sum(kind == 'd' for _, kind in expected),
                files=sum(kind == 'f' for _, kind in expected))


def classify_rank(paths, target, full_paths=None, complete=False):
    if target in paths:
        return paths.index(target) + 1, 'ok'
    if full_paths is not None:
        return (-1, 'censored') if target in full_paths else (0, 'miss')
    return (0, 'miss') if complete else (-1, 'unresolved')


def checked_result(result, gen, limit, complete_matches=None):
    if result.get('gen') != gen:
        raise ValueError('stale generation')
    paths = result['paths']
    if result.get('n') is not None and (not isinstance(result['n'], int) or result['n'] < len(paths)):
        raise ValueError('invalid total-match count')
    total = result.get('n') if complete_matches is None else complete_matches
    if complete_matches is not None and result.get('n') is not None and result['n'] != complete_matches:
        raise ValueError('reported total differs from complete audit')
    if total is not None and (type(total) is not int or total < 0 or len(paths) != min(limit, total)):
        raise ValueError('returned head does not cover requested matches')
    if len(paths) > limit or len(paths) != len(set(paths)):
        raise ValueError('invalid returned head')
    for name in paths:
        raw = os.fsencode(name)
        if not raw or raw.startswith(b'/') or b'..' in raw.split(b'/') or b'\0' in raw:
            raise ValueError('invalid relative identity')
    return total is not None


def quality_reason(kind, paths, target):
    if target not in paths:
        return 'final target missing'
    if kind.split('-', 1)[0] in {'exact', 'typo', 'path'} and paths[0] != target:
        return 'unambiguous final target misranked'
    return None


def observed_identities(tree, paths):
    result = []
    for path in paths:
        mode = os.lstat(os.path.join(os.fsencode(tree), os.fsencode(path))).st_mode
        result.append((os.fsencode(path), 'd' if stat.S_ISDIR(mode) else 'f'))
    return result


def marked(path):
    root = pathlib.Path(path)
    if not root.is_absolute() or root != root.resolve(strict=True):
        raise ValueError('absolute nonsymlink root required')
    marker = root / '.findbench-root'
    if marker.is_symlink() or not marker.is_file():
        raise ValueError('marked root required')
    return root


def checked_mutation(root, path):
    root, path = marked(root), pathlib.Path(path)
    if not path.is_absolute() or path.is_symlink() or path == root or path.name == '.findbench-root':
        raise ValueError('unsafe mutation path')
    path.resolve(strict=False).relative_to(root)
    return path


def read_json(path, maximum=1024 * 1024):
    path = pathlib.Path(path)
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError('regular JSON file required')
        with os.fdopen(fd, 'rb', closefd=False) as stream:
            data = stream.read(maximum + 1)
        if len(data) > maximum:
            raise ValueError('JSON size limit exceeded')
        def unique(pairs):
            result = {}
            for key, value in pairs:
                if key in result:
                    raise ValueError('duplicate JSON member')
                result[key] = value
            return result
        return json.loads(data, object_pairs_hook=unique)
    finally:
        os.close(fd)


def file_sha256(path):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError('regular identity file required')
        result = hashlib.sha256()
        with os.fdopen(fd, 'rb', closefd=False) as stream:
            while chunk := stream.read(1024 * 1024):
                result.update(chunk)
        return result.hexdigest()
    finally:
        os.close(fd)


def verify_snapshot(root, manifest):
    root, manifest = pathlib.Path(root), pathlib.Path(manifest)
    if hashlib.sha256(manifest.read_bytes()).hexdigest() != MANIFEST_SHA256:
        raise ValueError('unreviewed R2 manifest')
    entries = read_json(manifest)
    for rel, expected in entries.items():
        path = root / rel
        if path.is_symlink() or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError('R2 snapshot differs: ' + rel)
    return entries


def fixture_lock(root, label, lock):
    root = marked(root)
    tree = root / 'tree'
    if tree.is_symlink() or tree.resolve() != tree or not tree.is_dir():
        raise ValueError('fixture tree must be a real directory')
    observed = identities(tree)
    count = lock['entries'] if label == 'quality' else 682 if label == 'source' else int(label)
    if len(observed) != count or lock.get('identity_sha256') != digest(observed):
        raise ValueError('frozen fixture identities differ')
    if label == 'source':
        if lock.get('sources') != SOURCE_COMMITS:
            raise ValueError('historical source commits differ')
        if digest(observed) != SOURCE_IDENTITY_SHA256 or lock.get('content_sha256') != SOURCE_CONTENT_SHA256:
            raise ValueError('historical 682-entry source lock differs')
        content = hashlib.sha256()
        for raw, kind in observed:
            content.update(raw + b'\0' + kind.encode() + b'\0')
            if kind == 'f':
                path = tree / os.fsdecode(raw)
                if path.is_symlink():
                    raise ValueError('historical source symlink')
                content.update(bytes.fromhex(file_sha256(path)))
        if lock.get('content_sha256') != content.hexdigest():
            raise ValueError('historical source content differs')
    queries = lock['queries']
    if (label != 'quality' and len(queries) != 5) or (label == 'source' and queries != SOURCE_QUERIES):
        raise ValueError('historical query trace differs')
    queries = queries + lock.get('quality_queries', [])
    if not 1 <= len(queries) <= 256:
        raise ValueError('quality task count outside bounded manifest')
    names = {os.fsdecode(raw) for raw, _ in observed}
    if any(len(q) != 3 or q[2] not in names for q in queries):
        raise ValueError('fixture query target missing')
    return root, tree, queries, dict(entries=count, identity_sha256=digest(observed))


GROUP = ['fixture', 'profile', 'query_kind', 'prefix', 'query', 'location', 'metric', 'unit', 'work', 'quality']


def compare(samples, candidate, reference, minimum=30, iterations=2000, profiles=None):
    minimum = max(30, minimum)
    groups = collections.defaultdict(lambda: collections.defaultdict(lambda: collections.defaultdict(list)))
    resolutions = collections.defaultdict(list)
    for row in samples:
        if row['engine'] not in {candidate, reference} or row.get('eligible') != 'yes':
            continue
        if row['status'] not in {'ok', 'coarse'} or row['value'] in ('', None):
            continue
        if row.get('quality') not in {'', None, 'ok'} or row['unit'] not in {'us', 'ms', 'bytes', 'bytes/entry'}:
            continue
        if profiles is not None:
            if row['profile'] != profiles[row['engine']]:
                continue
            row = dict(row, profile=profiles[candidate] + ' vs ' + profiles[reference])
        key = tuple(row.get(field, '') for field in GROUP)
        value = float(row['value'])
        if not math.isfinite(value) or value < 0:
            raise ValueError('nonfinite or negative performance sample')
        groups[key][row['engine'], row['phase']][str(row['run'])].append(value)
        if row.get('resolution') not in ('', None):
            resolution = float(row['resolution'])
            if not math.isfinite(resolution) or resolution <= 0:
                raise ValueError(f"searchbench {row['engine']} {row['metric']} {row['phase']} run {row['run']}: invalid instrument resolution {row['resolution']!r}")
            resolutions[key].append(resolution)
    reports = []
    for key, populations in sorted(groups.items()):
        left, right = populations[candidate, 'measured'], populations[reference, 'measured']
        paired = sorted(set(left) & set(right))
        report = dict(zip(GROUP, key), candidate=candidate, reference=reference, paired_runs=len(paired))
        if len(paired) < minimum or set(left) != set(right):
            reports.append(dict(report, verdict='insufficient-pairs'))
            continue
        shifts = []
        for engine in (candidate, reference):
            pilots = [populations[engine, phase] for phase in ('noise-a', 'noise-b')]
            if any(len(batch) != 5 for batch in pilots):
                break
            shifts.append(abs(statistics.median(statistics.median(v) for v in pilots[0].values()) -
                              statistics.median(statistics.median(v) for v in pilots[1].values())))
        if len(shifts) != 2:
            reports.append(dict(report, verdict='missing-metric-pilot'))
            continue
        delta = [statistics.median(right[run]) - statistics.median(left[run]) for run in paired]
        generator = random.Random(0)
        boot = sorted(statistics.median(generator.choices(delta, k=len(delta))) for _ in range(iterations))
        lo, hi = boot[int(iterations * .025)], boot[min(iterations - 1, int(iterations * .975))]
        resolution = .001 if report['unit'] == 'us' else .000001 if report['unit'] == 'ms' else 1
        if 'ticks' in report['metric'] and report['unit'] in {'us', 'ms'}:
            resolution = (1000000 if report['unit'] == 'us' else 1000) / os.sysconf('SC_CLK_TCK')
        threshold = max(2 * max(shifts), resolution, *resolutions[key])
        verdict = 'win' if lo > threshold else 'loss' if hi < -threshold else 'tie-or-unresolved'
        reports.append(dict(report, verdict=verdict, gain=statistics.median(delta), ci95=[lo, hi], noise_threshold=threshold,
                            candidate_p50=statistics.median(statistics.median(v) for v in left.values()),
                            reference_p50=statistics.median(statistics.median(v) for v in right.values())))
    return reports


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
            row['work'] = ('product-rich' if context.get('engine') == 'flea' else 'names-only') if metric in product_metrics else 'common'
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



def measurement_type(base):
    class Measurements(base):
        def row(self, context, *args, **kwargs):
            context = dict(context)
            if context.get('engine') == 'flea' and 'work' not in context and args[0] in {
                'first_result_ms', 'full_returned_list_ms', 'open_to_first_result_ms', 'open_to_full_returned_list_ms',
                'cpu_per_key_ticks_ms', 'cpu_us', 'client_cpu_us', 'decode_cpu_us', 'typed_trace_cpu_us',
                'typed_trace_cpu_ticks_ms', 'steady_process_total_cpu_ms', 'startup_process_total_cpu_ms',
                'process_total_cpu_ms', 'core_reply_us', 'response_bytes'}:
                context['work'] = 'product-rich'
            super().row(labelled(context), *args, **kwargs)

        def invalidate(self, context, reason):
            super().invalidate(labelled(context), reason)

        def finish(self, completed):
            if not completed:
                self.file.flush()
                with (self.output / 'pending-measurements.tsv').open() as stream:
                    for row in csv.DictReader(stream, delimiter='\t'):
                        self.invalidate(row, 'collection incomplete')
            super().finish(completed)
            original = self.output / 'measurements.tsv'
            extended = self.output / 'evidence.tsv'
            original.rename(extended)
            with extended.open() as source, original.open('x', newline='') as out:
                writer = csv.DictWriter(out, fieldnames=ORIGINAL_COLUMNS, delimiter='\t', lineterminator='\n', extrasaction='ignore')
                writer.writeheader()
                writer.writerows(csv.DictReader(source, delimiter='\t'))
            with extended.open() as stream:
                reader = csv.DictReader(stream, delimiter='\t')
                fields, evidence = reader.fieldnames + ['location'], list(reader)
            with extended.open('w', newline='') as stream:
                writer = csv.DictWriter(stream, fieldnames=fields, delimiter='\t', lineterminator='\n')
                writer.writeheader()
                for row in evidence:
                    row['location'] = next((location for location in ('root', 'deep')
                                            if row['query_kind'].endswith('-' + location)), '')
                    writer.writerow(row)
            performance = self.output / 'performance.tsv'
            with performance.open() as stream:
                reader = csv.DictReader(stream, delimiter='\t')
                fields, rows = reader.fieldnames, list(reader)
            with performance.open('w', newline='') as stream:
                writer = csv.DictWriter(stream, fieldnames=fields, delimiter='\t', lineterminator='\n')
                writer.writeheader()
                writer.writerows(row for row in rows if row.get('quality') in ('', 'ok'))
    return Measurements




def labelled(context):
    context = dict(context)
    if context.get('engine') == 'flea' and '/' in context.get('profile', ''):
        role, profile = context['profile'].split('/', 1)
        context.update(engine='flea-' + role, profile=profile)
    return context


