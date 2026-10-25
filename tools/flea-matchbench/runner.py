# Engine-stage matching bench: every entrant gets the same corpus, the same typed query trace and the same clocks.
import argparse
import csv
import hashlib
import json
import math
import os
import pathlib
import platform
import random
import selectors
import socket
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
# West (ultra-27) and East2 (ultra-36) only: never East's shared host, never minipc.
ALLOWED_HOSTS = {'ultra-27', 'ultra-36'}
COLUMNS = ['phase', 'run', 'fixture', 'engine', 'profile', 'query_kind', 'prefix', 'query',
           'metric', 'value', 'unit', 'status', 'note']
ENTRANTS = [('flea-search', 'native'), ('flea-current', 'native'), ('frizbee', 'native'), ('frizbee', 'typo'), ('neo_frizbee', 'native'),
            ('neo_frizbee', 'typo'), ('nucleo', 'native'), ('fuzzy-matcher', 'native'), ('sublime_fuzzy', 'native'),
            ('fzf', 'native')]
REFERENCE = ('flea-search', 'native')
FZF_VERSION = '0.74.5'
FZF_URL = f'https://github.com/junegunn/fzf/releases/download/v{FZF_VERSION}/fzf-{FZF_VERSION}-linux_amd64.tar.gz'
FZF_SHA256 = 'd47d7f882972c7d674dbbba998ffa4116e68379616ffac7695d0671e9a722c5f'
NO_MATCH = 'zz_no_match_9b2d7e61'
RESULT_CAP = 350
BOOTSTRAP = 10000

# The B3 synthetic fixture's formula (findbench1 fixtures.py), replayed on paths so no file is created.
TOPS = ['Documents', 'Downloads', 'Pictures', 'Music', 'Videos', 'projects', 'archive', 'shared']
EXTENSIONS = ['rs', 'c', 'h', 'py', 'js', 'qml', 'md', 'txt', 'pdf', 'jpg', 'mp4', 'flac', 'svg', 'json', 'zip']
WORDS = ['report', 'notes', 'render', 'window', 'invoice', 'meeting', 'holiday', 'budget', 'parser', 'model', 'test', 'draft']
TARGETS = [
    ('exact', 'render_frame.rs', 'projects/editor/src/components/render_frame.rs'),
    ('typo', 'redner_frame.rs', 'projects/editor/src/components/render_frame.rs'),
    ('path', 'editor/src/components/render', 'projects/editor/src/components/render_frame.rs'),
    ('extension', '.svg', 'Documents/reference/architecture_diagram.svg'),
    ('deep', 'archive/2024/client/reports/quarterly', 'archive/2024/client/reports/quarterly_review.pdf'),
]
SOURCE_TARGETS = [
    ('exact', 'fuzzy.rs', 'src/backend/fuzzy.rs'),
    ('typo', 'fzuzy.rs', 'src/backend/fuzzy.rs'),
    ('path', 'backend/searchindex', 'src/backend/searchindex.rs'),
    ('extension', '.qml', 'ui/SearchStrip.qml'),
    ('deep', 'src/backend/searchw', 'src/backend/searchwalk.rs'),
]


def marked_root(parent, prefix):
    parent = pathlib.Path(parent).resolve(strict=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix=prefix, dir=parent))
    (root / '.flea-matchbench-root').write_text('flea-matchbench\n')
    return root


def synthetic_paths(entries):
    dirs, files = set(), set()

    def mkdirs(rel):
        parts = rel.split('/')
        for i in range(1, len(parts) + 1):
            dirs.add('/'.join(parts[:i]))

    for _, _, rel in TARGETS:
        mkdirs(rel.rsplit('/', 1)[0])
        files.add(rel)
    leaves = []
    for i in range(max(8, entries // 32)):
        digits, value = [], i // len(TOPS)
        while value:
            digits.append(value % 8)
            value //= 8
        leaf = '/'.join([TOPS[i % len(TOPS)]] + [f'folder_{d:02d}' for d in reversed(digits or [0])])
        mkdirs(leaf)
        leaves.append(leaf)
    current = len(dirs) + len(files)
    for i in range(entries - current):
        stem = WORDS[i % len(WORDS)] + '_' + WORDS[(i // len(WORDS)) % len(WORDS)]
        stem += f'_{i:08d}' if i % 3 else f'_{i % 97:02d}'
        stem += '_detail' * (i % 5)
        name = stem + '.' + EXTENSIONS[i % len(EXTENSIONS)]
        leaf = leaves[(i * 37) % len(leaves)]
        path = f'{leaf}/{name}'
        if path in files or path in dirs:
            path = f'{leaf}/{i:08d}_{name}'
        if path in files or path in dirs:
            raise RuntimeError('fixture formula produced a second collision: ' + path)
        files.add(path)
    paths = sorted(dirs | files)
    if len(paths) != entries:
        raise RuntimeError(f'exact fixture count failed: {len(paths)} != {entries}')
    return paths


def source_paths(repo, commit):
    # Tracked blobs and trees at the pinned commit; hidden components, symlinks and submodules are excluded.
    out = subprocess.run(['git', '-C', str(repo), 'ls-tree', '-r', '-t', '-z', commit], check=True,
                         capture_output=True).stdout
    paths = []
    for record in out.split(b'\0'):
        if not record:
            continue
        meta, _, name = record.partition(b'\t')
        mode = meta.split()[0]
        path = name.decode()
        if mode in (b'120000', b'160000') or any(part.startswith('.') for part in path.split('/')):
            continue
        paths.append(path)
    return sorted(paths)


def fixture(label, repo, commit):
    if label == 'source':
        return source_paths(repo, commit), SOURCE_TARGETS, dict(kind='flea-source', commit=commit)
    entries = int(label)
    return synthetic_paths(entries), TARGETS, dict(kind='synthetic', formula='findbench1 fixtures.py synthetic, paths only')


def provision_fzf(output, explicit):
    if explicit:
        return pathlib.Path(explicit), 'operator binary ' + explicit
    root = marked_root(output, 'fzf-')
    tarball = root / 'fzf.tar.gz'
    with urllib.request.urlopen(FZF_URL, timeout=120) as response:
        tarball.write_bytes(response.read())
    digest = hashlib.sha256(tarball.read_bytes()).hexdigest()
    if digest != FZF_SHA256:
        raise RuntimeError(f'fzf release checksum mismatch: {digest}')
    with tarfile.open(tarball) as archive:
        member = archive.getmember('fzf')
        if not member.isfile():
            raise RuntimeError('fzf tarball member is not a regular file')
        try:
            archive.extract(member, root, filter='data')
        except TypeError:
            archive.extract(member, root)
    binary = root / 'fzf'
    binary.chmod(0o755)
    return binary, f'fzf {FZF_VERSION} release tarball sha256 {FZF_SHA256}'


class Entrant:
    def __init__(self, name, profile, adapter_bin, fzf, cpus, threads, logdir):
        if name == 'flea-search':
            argv = [os.environ['FLEA_SEARCH_BIN'], '--search-stage', 'match', str(threads)]
        elif name == 'fzf':
            if fzf is None:
                raise RuntimeError('fzf binary unavailable')
            argv = [sys.executable, '-I', str(HERE / 'fzf_driver.py'), str(fzf), str(threads)]
        else:
            argv = [str(adapter_bin), 'adapter', name, profile, str(threads)]
        argv = ['taskset', '--cpu-list', ','.join(map(str, cpus)), *argv]
        self.name, self.profile = name, profile
        self.stderr = open(logdir / f'stderr-{name}-{profile}-{time.monotonic_ns()}.log', 'xb')
        env = {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'LC_ALL': 'C.UTF-8', 'FZF_VERSION': FZF_VERSION}
        if name == 'flea-search':
            stage_root = pathlib.Path(tempfile.mkdtemp(prefix='flea-stage-', dir=logdir))
            (stage_root / '.findbench-root').write_text('flea-searchbench\n')
            env['FLEA_STAGE_ROOT'] = str(stage_root)
        self.process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
                                        env=env, bufsize=0)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.buffer = bytearray()
        self.gen = 0
        self.boot = self.receive()
        if self.boot.get('t') != 'boot' or self.boot.get('cpus') != threads:
            raise RuntimeError('adapter boot did not confirm the thread budget: ' + json.dumps(self.boot))

    def send(self, text):
        self.process.stdin.write((text + '\n').encode())

    def receive(self, timeout=3600):
        deadline = time.monotonic() + timeout
        while b'\n' not in self.buffer:
            left = deadline - time.monotonic()
            if left <= 0 or not self.selector.select(left):
                raise TimeoutError(f'{self.name} response deadline expired')
            chunk = os.read(self.process.stdout.fileno(), 1 << 20)
            if not chunk:
                raise RuntimeError(f'{self.name} exited before replying')
            self.buffer.extend(chunk)
        raw, _, rest = self.buffer.partition(b'\n')
        self.buffer = bytearray(rest)
        return json.loads(raw)

    def open(self, corpus):
        at = time.perf_counter_ns()
        self.send('O\t' + str(corpus))
        ready = self.receive()
        return dict(ready, observed_ms=(time.perf_counter_ns() - at) / 1e6)

    def query(self, text, limit=RESULT_CAP):
        self.gen += 1
        at = time.perf_counter_ns()
        self.send(f'Q\t{self.gen}\t{limit}\t0\tnative\t{text}')
        result = self.receive()
        if result.get('t') != 'result' or result.get('gen') != self.gen:
            raise RuntimeError('mismatched result: ' + json.dumps(result)[:300])
        result['observed_ms'] = (time.perf_counter_ns() - at) / 1e6
        return result

    def audit(self, text, target):
        self.gen += 1
        self.send(f'A\t{self.gen}\t{target}\t{text}')
        result = self.receive()
        if result.get('t') != 'audit' or result.get('gen') != self.gen:
            raise RuntimeError('mismatched audit: ' + json.dumps(result)[:300])
        return result

    def ping(self):
        at = time.perf_counter_ns()
        self.send('P')
        if self.receive().get('t') != 'pong':
            raise RuntimeError('ping answered with something else')
        return (time.perf_counter_ns() - at) / 1e6

    def close(self):
        try:
            self.send('X')
            self.process.stdin.close()
        except OSError:
            pass
        _, status, usage = os.wait4(self.process.pid, 0)
        self.selector.close()
        self.process.stdout.close()
        self.stderr.close()
        return os.waitstatus_to_exitcode(status), (usage.ru_utime + usage.ru_stime) * 1000


class Rows:
    def __init__(self, output):
        self.file = (output / 'measurements.tsv').open('x', newline='')
        self.writer = csv.DictWriter(self.file, fieldnames=COLUMNS, delimiter='\t', lineterminator='\n')
        self.writer.writeheader()
        self.blocked = 0
        self.kept = []

    def row(self, context, metric, value, unit, status='ok', note=''):
        row = dict.fromkeys(COLUMNS, '')
        row.update(context, metric=metric, value='' if value is None else value, unit=unit, status=status, note=note)
        self.writer.writerow(row)
        self.blocked += status == 'blocked'
        self.kept.append(row)

    def close(self):
        self.file.close()


def run_one(rows, context, entrant, corpus, entries, queries):
    ready = entrant.open(corpus)
    rows.row(context, 'index_build_ms', ready['build_ms'], 'ms')
    rows.row(context, 'index_build_observed_ms', ready['observed_ms'], 'ms')
    rows.row(context, 'index_build_cpu_ms', ready['build_cpu_ns'] / 1e6, 'ms')
    rows.row(context, 'indexed_entries', ready['n'], 'entries', 'ok' if ready['n'] == entries else 'quality-fail',
             f'corpus holds {entries}')
    for value in [entrant.ping() for _ in range(20)]:
        rows.row(context, 'zero_work_round_trip_ms', value, 'ms')
    trace_cpu = 0
    for kind, text, target in queries:
        trace_cpu += entrant.query(NO_MATCH)['cpu_ns']
        for prefix in range(1, len(text) + 1):
            typed = text[:prefix]
            result = entrant.query(typed)
            trace_cpu += result['cpu_ns']
            q = dict(context, query_kind=kind, prefix=prefix, query=typed)
            rows.row(q, 'match_core_ms', result['core_ms'], 'ms', 'ok', result.get('clock', 'in-process monotonic'))
            rows.row(q, 'match_observed_ms', result['observed_ms'], 'ms')
            rows.row(q, 'match_cpu_ms', result['cpu_ns'] / 1e6, 'ms')
            rank = result['paths'].index(target) + 1 if target in result['paths'] else -1
            rows.row(q, 'intended_rank_top_k', rank, 'rank', 'ok' if rank > 0 else 'censored',
                     '-1 means absent from the returned top-k')
            if prefix == len(text):
                rows.row(q, 'final_rank_top_k', rank, 'rank', 'ok' if rank > 0 else 'censored',
                         'rank of the target for the whole query; -1 means outside the returned top-k')
            rows.row(q, 'returned_count', len(result['paths']), 'entries')
            rows.row(q, 'total_matches', result['n'], 'entries')
    rows.row(context, 'trace_cpu_ms', trace_cpu / 1e6, 'ms')
    for kind, text, target in queries:
        audit = entrant.audit(text, target)
        q = dict(context, query_kind=kind + '-rank-audit', prefix=len(text), query=text)
        rows.row(q, 'intended_rank_full', audit['rank'], 'rank', 'ok' if audit['rank'] > 0 else 'quality-fail',
                 '0 means absent from every match')
        rows.row(q, 'total_matches_full', audit['n'], 'entries')


def bootstrap_median(values, rng):
    if not values:
        return None, None, None
    n = len(values)
    medians = sorted(statistics.median(rng.choices(values, k=n)) for _ in range(BOOTSTRAP))
    return statistics.median(values), medians[int(0.025 * BOOTSTRAP)], medians[int(0.975 * BOOTSTRAP) - 1]


def per_run(rows, metric, reducer):
    groups = {}
    for row in rows:
        if row['phase'] == 'measured' and row['metric'] == metric and row['status'] == 'ok' and row['value'] != '':
            key = (row['fixture'], row['engine'], row['profile'])
            groups.setdefault(key, {}).setdefault(row['run'], []).append(float(row['value']))
    return {key: {run: reducer(values) for run, values in runs.items()} for key, runs in groups.items()}


def p95(values):
    values = sorted(values)
    return values[math.ceil(0.95 * len(values)) - 1]


def summarize(output, rows, dry):
    rng = random.Random(20261025)
    metrics = [('match_core_ms', 'median', statistics.median), ('match_core_ms', 'p95', p95),
               ('match_observed_ms', 'median', statistics.median), ('match_cpu_ms', 'median', statistics.median),
               ('trace_cpu_ms', 'total', sum), ('index_build_ms', 'value', statistics.median),
               ('index_build_cpu_ms', 'value', statistics.median), ('zero_work_round_trip_ms', 'median', statistics.median)]
    summary, paired = [], []
    for metric, label, reducer in metrics:
        table = per_run(rows, metric, reducer)
        for key, runs in sorted(table.items()):
            median, low, high = bootstrap_median(list(runs.values()), rng)
            summary.append([*key, f'{metric}:{label}', len(runs), median, low, high])
            reference = table.get((key[0], *REFERENCE))
            if reference and key[1:] != REFERENCE:
                common = sorted(set(runs) & set(reference))
                ratios = [runs[r] / reference[r] for r in common if reference[r] > 0]
                median, low, high = bootstrap_median(ratios, rng)
                paired.append([*key, f'{metric}:{label}', '/'.join(REFERENCE), len(ratios), median, low, high])
    quality = {}
    for row in rows:
        if row['phase'] == 'measured' and row['metric'] in ('intended_rank_full', 'final_rank_top_k'):
            key = (row['fixture'], row['engine'], row['profile'], row['query_kind'].removesuffix('-rank-audit'), row['metric'])
            quality.setdefault(key, set()).add(int(row['value']))
    with (output / 'summary.tsv').open('x') as out:
        out.write('fixture\tengine\tprofile\tmetric\truns\tmedian\tci95_low\tci95_high\n')
        for line in summary:
            out.write('\t'.join(map(str, line)) + '\n')
    with (output / 'paired.tsv').open('x') as out:
        out.write('fixture\tengine\tprofile\tmetric\treference\tpaired_runs\tmedian_ratio\tci95_low\tci95_high\n')
        for line in paired:
            out.write('\t'.join(map(str, line)) + '\n')
    with (output / 'quality.tsv').open('x') as out:
        out.write('fixture\tengine\tprofile\tquery_kind\tmetric\tranks_seen\tstable\n')
        for key, ranks in sorted(quality.items()):
            out.write('\t'.join([*key, ','.join(map(str, sorted(ranks))), str(len(ranks) == 1)]) + '\n')
    mrr = {}
    for key, ranks in quality.items():
        if key[4] == 'final_rank_top_k':
            rank = min(ranks)
            mrr.setdefault(key[:3], []).append(1 / rank if rank > 0 else 0)
    with (output / 'mrr.tsv').open('x') as out:
        out.write('fixture\tengine\tprofile\tqueries\tmrr_top_k\n')
        for key, values in sorted(mrr.items()):
            out.write('\t'.join([*key, str(len(values)), f'{sum(values) / len(values):.4f}']) + '\n')
    for name in ['summary.tsv', 'paired.tsv', 'quality.tsv', 'mrr.tsv']:
        print(f'== {name}{" (DRY RUN, not evidence)" if dry else ""}')
        print((output / name).read_text(), end='', flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--adapter-bin', required=True, type=pathlib.Path)
    parser.add_argument('--output-parent', type=pathlib.Path, default=REPO / '.superpowers' / 'flea-matchbench')
    parser.add_argument('--fixtures', default='100000,1000000,source')
    parser.add_argument('--runs', type=int, default=30)
    parser.add_argument('--noise-runs', type=int, default=5)
    parser.add_argument('--threads', type=int, default=2)
    parser.add_argument('--entrants', default=','.join(f'{n}/{p}' for n, p in ENTRANTS))
    parser.add_argument('--source-commit', default='0ebbcffae')
    parser.add_argument('--fzf', default='')
    parser.add_argument('--dry', action='store_true', help='one measured run, no noise batches; output is not evidence')
    args = parser.parse_args()
    host = socket.gethostname()
    if host not in ALLOWED_HOSTS:
        raise SystemExit(f'flea-matchbench runs on West or East2 only ({sorted(ALLOWED_HOSTS)}); this host is {host}')
    if not args.dry and (args.runs < 30 or args.noise_runs < 5):
        raise SystemExit('evidence runs need N >= 30 measured runs and two noise batches of at least five')
    entrants = [tuple(e.split('/')) for e in args.entrants.split(',')]
    unknown = set(entrants) - set(ENTRANTS)
    if unknown:
        raise SystemExit(f'unknown entrants: {sorted(unknown)}')
    allowed = sorted(os.sched_getaffinity(0))
    if len(allowed) < args.threads + 1:
        raise SystemExit('need the entrant budget plus one client CPU')
    engine_cpus, client_cpus = allowed[:args.threads], allowed[args.threads:args.threads + 1]
    os.sched_setaffinity(0, set(client_cpus))
    args.output_parent.mkdir(parents=True, exist_ok=True)
    output = marked_root(args.output_parent, 'run-')
    logs = output / 'logs'
    logs.mkdir()
    rows = Rows(output)
    fzf, fzf_identity = None, 'not requested'
    if ('fzf', 'native') in entrants:
        try:
            fzf, fzf_identity = provision_fzf(output, args.fzf)
        except Exception as error:
            fzf_identity = f'BLOCKED: {error}'
    lock = subprocess.run([str(args.adapter_bin), 'lock'], capture_output=True, text=True).stdout
    environment = dict(host=host, kernel=platform.release(), machine=platform.machine(), python=platform.python_version(),
                       engine_cpus=engine_cpus, client_cpus=client_cpus, threads=args.threads, result_cap=RESULT_CAP,
                       runs=args.runs, noise_runs=args.noise_runs, dry=args.dry, entrants=entrants, fzf=fzf_identity,
                       source_commit=args.source_commit, lock_sha256=hashlib.sha256(lock.encode()).hexdigest(),
                       repo_head=subprocess.run(['git', '-C', str(REPO), 'rev-parse', 'HEAD'], capture_output=True,
                                                text=True).stdout.strip())
    (output / 'environment.json').write_text(json.dumps(environment, indent=2) + '\n')
    (output / 'Cargo.lock.resolved').write_text(lock)
    print('RUN_ROOT=' + str(output))
    print('ENVIRONMENT ' + json.dumps(environment), flush=True)
    phases = [('measured', 1)] if args.dry else [('noise-a', args.noise_runs), ('noise-b', args.noise_runs), ('measured', args.runs)]
    try:
        for label in args.fixtures.split(','):
            paths, queries, info = fixture(label, REPO, args.source_commit)
            for _, _, target in queries:
                if target not in paths:
                    raise SystemExit(f'fixture {label} lacks target {target}')
            corpus = output / f'corpus-{label}.txt'
            corpus.write_text('\n'.join(paths) + '\n')
            info.update(entries=len(paths), sha256=hashlib.sha256(corpus.read_bytes()).hexdigest())
            print(f'fixture={label} ' + json.dumps(info), flush=True)
            for phase, repetitions in phases:
                for run in range(repetitions):
                    # Balanced interleave: the order rotates every run and reverses every other run.
                    rotation = run % len(entrants)
                    order = entrants[rotation:] + entrants[:rotation]
                    if run % 2:
                        order.reverse()
                    for name, profile in order:
                        context = dict(phase=phase, run=run, fixture=label, engine=name, profile=profile)
                        entrant = None
                        started = time.monotonic()
                        try:
                            entrant = Entrant(name, profile, args.adapter_bin, fzf, engine_cpus, args.threads, logs)
                            if run == 0 and phase == phases[0][0]:
                                rows.row(context, 'identity', None, 'text', 'ok', json.dumps(entrant.boot))
                            run_one(rows, context, entrant, corpus, len(paths), queries)
                        except Exception as error:
                            rows.row(context, 'execution', None, 'status', 'blocked', repr(error)[:500])
                            print(f'BLOCKED {label} {phase} {run} {name}/{profile}: {error!r}', flush=True)
                        finally:
                            if entrant:
                                code, cpu = entrant.close()
                                rows.row(context, 'process_total_cpu_ms', cpu, 'ms', 'ok' if code == 0 else 'blocked')
                                rows.row(context, 'process_exit_code', code, 'code', 'ok' if code == 0 else 'blocked')
                        print(f'done {label} {phase} {run} {name}/{profile} {time.monotonic() - started:.1f}s', flush=True)
            if not args.dry:
                for name, profile in entrants:
                    batches = []
                    for batch in ['noise-a', 'noise-b']:
                        values = [float(r['value']) for r in rows.kept if r['phase'] == batch and r['fixture'] == label
                                  and r['engine'] == name and r['profile'] == profile and r['metric'] == 'match_core_ms']
                        batches.append(statistics.median(values) if values else None)
                    if None not in batches:
                        ratio = max(batches) / max(1e-9, min(batches))
                        rows.row(dict(phase='measured', run='noise-check', fixture=label, engine=name, profile=profile),
                                 'noise_batch_median_ratio', ratio, 'ratio', 'ok' if ratio <= 1.2 else 'unstable')
    finally:
        rows.close()
    summarize(output, rows.kept, args.dry)
    print(f'BLOCKED_ROWS={rows.blocked}')
    return 2 if rows.blocked else 0


if __name__ == '__main__':
    sys.exit(main())
