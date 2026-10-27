import collections
import hashlib
import json
import os
import pathlib
import os
import selectors
import subprocess
import time

from fixtures import marked_root
from contracts import checked_result, identities, observed_identities
from priors import RICH_OPENS, replay, clone, clone_fff


def process_memory(pid):
    values = {}
    for line in pathlib.Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines():
        if line.startswith(('Rss:', 'Pss:')):
            name, number, _ = line.split()
            values[name[:-1].lower()] = int(number) * 1024
    if set(values) != {'rss', 'pss'}:
        raise RuntimeError('RSS/PSS unavailable')
    return values


def cpu_ticks(pid):
    text = pathlib.Path(f'/proc/{pid}/stat').read_text()
    fields = text[text.rfind(')') + 2:].split()
    return int(fields[11]) + int(fields[12])


def process_cpu_ns(pid):
    # Linux process CPUCLOCK_SCHED includes all threads, with nanosecond counters.
    return time.clock_gettime_ns((-pid - 1) * 8 + 2)


class Engine:
    def __init__(self, name, profile, build, output, tree, history, cpus, threads=2, prior_seed=None):
        # The 0.3.12 anchor speaks the same protocol as the candidate; only its binary differs.
        self.variant, name = name, 'flea' if name == 'flea-0.3.12' else name
        self.name, self.profile, self.tree = name, profile, pathlib.Path(tree)
        self.state = marked_root(output, f'state-{self.variant}-{profile}-')
        env = os.environ.copy()
        for key in ['DBUS_SESSION_BUS_ADDRESS', 'DISPLAY', 'WAYLAND_DISPLAY']:
            env.pop(key, None)
        for suffix in ['config', 'cache', 'data', 'state', 'runtime', 'tmp']:
            directory = self.state / suffix
            directory.mkdir(mode=0o700)
            key = 'TMPDIR' if suffix == 'tmp' else f'XDG_{suffix.upper()}_HOME'
            if suffix == 'runtime':
                key = 'XDG_RUNTIME_DIR'
            env[key] = str(directory)
        env['_ZO_DATA_DIR'] = str(self.state / 'data' / 'zoxide')
        if profile == 'equal-history':
            if prior_seed is None:
                raise ValueError('equal-history requires one shared replayed provider seed')
            self.priors = clone(prior_seed, self.state, env)
        else:
            self.priors = replay(self.state, self.tree, identities(self.tree), history, profile, env)
        if profile == 'rich':
            (self.state / 'history.json').write_text(json.dumps(sorted(set(history))) + '\n')
            self.priors.update(replay=sorted(set(history)), opens_per_path=RICH_OPENS, held_out=[],
                               policy='historical target-seeded profile, the same for fff and flea; not equal-history')
            (self.state / 'priors.json').write_text(json.dumps(self.priors, indent=2) + '\n')
        self.native_priors = None
        if name == 'fff' and profile in {'equal-history', 'rich'}:
            if prior_seed is None:
                raise ValueError('fff history requires a fixture seed')
            self.native_priors = clone_fff(prior_seed, profile, self.state)
        build = pathlib.Path(build)
        if name == 'flea':
            target = 'flea-anchor-target' if self.variant == 'flea-0.3.12' else 'flea-target'
            binary = os.environ.get('FLEA_SEARCH_BIN') if self.variant == 'flea' else None
            argv = [binary or str(build / target / 'release/flea-engine'), '--backend']
        elif name == 'fff':
            target = 'target-historical' if profile == 'rich' else 'target'
            argv = [str(build / target / 'release/fff-driver'), str(tree), str(self.state), profile, str(threads)]
        elif name == 'fsearch':
            argv = [str(build / 'target/release/fsearch-driver'), str(tree), str(threads)]
        elif name == 'fsearch-nd':
            argv = [str(build / 'target/release/fsearch-nd-driver'), str(tree), str(threads)]
        else:
            raise ValueError(name)
        self.stderr = (self.state / 'stderr.log').open('xb')
        argv = ['taskset', '--cpu-list', ','.join(map(str, cpus)), *argv]
        self.process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, env=env, bufsize=0)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.buffer = bytearray()
        self.queue = collections.deque()
        self.events = []
        self.decode_cpu_ns = 0
        self.response_bytes = 0
        self.stale_replies = 0
        self.capabilities = {}
        self.generation = 0
        self.ready = None
        self.closed = False
        self.log = (self.state / 'events.jsonl').open('x')
        self.query_log = (self.state / 'queries.jsonl').open('x')
        try:
            if name == 'flea':
                self.send(dict(c='window', start=0, count=0))
                self.until(lambda obj: obj.get('t') == 'rows')
                self.send(dict(c='list', path=str(tree), first=0, hidden=False))
                self.until(lambda obj: obj.get('t') == 'rows')
            else:
                boot, _ = self.until(lambda obj: obj.get('t') == 'boot')
                self.capabilities = boot.get('capabilities', {})
                if boot['cpus'] != threads:
                    raise RuntimeError('native CPU count mismatched requested budget')
            self.baseline_memory = process_memory(self.process.pid)
            # Every caller opens only after this, so no history open races the backend's prior load.
            self.prior_load = self.wait_priors() if self.variant == 'flea' and profile in ('equal-history', 'rich') else None
            self.flush_events()
        except BaseException:
            self.close()
            raise

    def send(self, value):
        if isinstance(value, dict):
            text = json.dumps(value, ensure_ascii=False, separators=(',', ':'))
        else:
            text = value
        payload = (text + '\n').encode()
        self.events.append(dict(direction='send', at_ns=time.perf_counter_ns(), bytes=len(payload),
                                sha256=hashlib.sha256(payload).hexdigest(),
                                command=value.get('c') if isinstance(value, dict) else text.split('\t', 1)[0],
                                gen=value.get('gen') if isinstance(value, dict) else None))
        self.process.stdin.write(payload)

    def receive(self, timeout=3600):
        deadline = time.monotonic() + timeout
        while not self.queue:
            left = deadline - time.monotonic()
            if left <= 0 or not self.selector.select(left):
                raise TimeoutError(f'{self.name} response deadline expired')
            chunk = os.read(self.process.stdout.fileno(), 1024 * 1024)
            if not chunk:
                raise RuntimeError(f'{self.name} exited before expected reply; see {self.state}/stderr.log')
            self.buffer.extend(chunk)
            while b'\n' in self.buffer:
                raw, _, remainder = self.buffer.partition(b'\n')
                self.buffer = bytearray(remainder)
                decode = time.process_time_ns()
                obj = json.loads(raw)
                self.decode_cpu_ns += time.process_time_ns() - decode
                self.response_bytes += len(raw) + 1
                stamp = time.perf_counter_ns()
                self.queue.append((obj, stamp))
                self.events.append(dict(at_ns=stamp, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(),
                                        t=obj.get('t'), gen=obj.get('gen'), n=obj.get('n'),
                                        walking=obj.get('walking'), scanned=obj.get('scanned')))
        obj, stamp = self.queue.popleft()
        if obj.get('t') == 'error':
            raise RuntimeError(json.dumps(obj))
        if obj.get('t') in ['ready', 'searchready']:
            self.ready = obj
        return obj, stamp

    def until(self, predicate, timeout=3600):
        deadline = time.monotonic() + timeout
        while True:
            obj, stamp = self.receive(max(0.001, deadline - time.monotonic()))
            if predicate(obj):
                return obj, stamp

    def flush_events(self):
        for item in self.events:
            self.log.write(json.dumps(item, separators=(',', ':')) + '\n')
        self.events.clear()
        self.log.flush()

    def begin_open(self):
        if self.name == 'flea':
            self.send(dict(c='searchopen', path=str(self.tree), folder=str(self.tree), hidden=False))
        else:
            self.send('O')

    def check_prior_clock(self, ready):
        if self.name == 'fff' and getattr(self, 'profile', None) in {'equal-history', 'rich'}:
            if ready.get('prior_replay_inside_open') is not False:
                raise ValueError('native prior replay is not proven outside index clocks')
            for name, expected in self.native_priors['files'].items():
                if pathlib.Path(name).name == 'lock.mdb':
                    continue
                current = self.state / 'frecency' / name
                if current.is_symlink() or hashlib.sha256(current.read_bytes()).hexdigest() != expected:
                    raise ValueError('native prior history changed during index open')

    def wait_priors(self, timeout=15):
        # Harness readiness, outside both index clocks: the backend starts its prior load and an open never waits for it.
        deadline = time.monotonic() + timeout
        while True:
            self.send(dict(c='benchpriors'))
            reply, _ = self.until(lambda obj: obj.get('t') == 'benchpriors', max(0.001, deadline - time.monotonic()))
            if reply.get('loaded') is True:
                self.flush_events()
                return reply
            if time.monotonic() >= deadline:
                raise TimeoutError('backend prior load did not finish before the history open')
            time.sleep(0.005)

    def open(self):
        at = time.perf_counter_ns()
        self.begin_open()
        ready, stamp = self.until(lambda obj: obj.get('t') in ['ready', 'searchready'])
        self.check_prior_clock(ready)
        memory = process_memory(self.process.pid)
        self.flush_events()
        return dict(observed_ms=(stamp - at) / 1e6, native_ms=ready.get('build_ms', ready.get('ms')),
                    indexed=ready['n'], memory=memory, details=ready)

    def begin_query(self, text, limit=350, offset=0, mode='native'):
        if any(c in text for c in '\t\r\n'):
            raise ValueError('query contains a driver framing character')
        self.generation += 1
        self.query_limit = limit
        if self.name == 'flea':
            self.send(dict(c='searchquery', query=text, gen=self.generation, count=limit, sizes=0))
        else:
            self.send(f'Q\t{self.generation}\t{limit}\t{offset}\t{mode}\t{text}')
        return self.generation

    def finish_query(self, gen, at, timeout=60):
        first, paths, partials, result = None, [], 0, None
        listed = False
        core_ms = core_reply_us = matcher_ms = answer_ms = None
        limit = self.query_limit
        deadline = time.monotonic() + timeout
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError('current generation response deadline expired')
            obj, stamp = self.receive(max(0.001, deadline - time.monotonic()))
            if obj.get('gen') != gen:
                if obj.get('gen') is not None:
                    self.stale_replies += 1
                    if obj['gen'] > gen:
                        raise ValueError('unexpected future generation')
                continue
            if self.name == 'flea':
                if obj['t'] == 'searchcore':
                    matcher_ms = obj.get('matcher_ms')
                    answer_ms = obj.get('answer_ms', obj.get('core_ms'))
                    core_ms = matcher_ms
                    core_reply_us = (stamp - at) / 1000
                elif obj['t'] == 'listed':
                    listed = True
                    paths = []
                    received_rows = False
                elif obj['t'] == 'rows':
                    if not listed:
                        raise ValueError('rows without paired listed generation')
                    received_rows = True
                    paths = [row['n'] for row in obj['rows']]
                    if paths and first is None:
                        first = (stamp - at) / 1e6
                elif obj['t'] == 'searched':
                    if not listed or not received_rows:
                        raise ValueError('searched without paired listed and rows')
                    if obj.get('walking'):
                        partials += 1
                    else:
                        result = dict(paths=paths, n=obj['n'], core_ms=core_ms, comparable_core_ms=matcher_ms, reported_answer_ms=answer_ms, full_ms=(stamp - at) / 1e6,
                                      epoch=obj.get('epoch'), applied_ns=obj.get('applied_ns'), pushed_ns=obj.get('pushed_ns'))
                        break
            elif obj['t'] == 'result':
                paths = obj['paths']
                if paths:
                    first = (stamp - at) / 1e6
                result = dict(paths=paths, n=obj.get('n'), core_ms=obj.get('core_ms'), comparable_core_ms=obj.get('comparable_core_ms'), full_ms=(stamp - at) / 1e6)
                break
        result.update(first_ms=first, partial_answers=partials, gen=gen, core_reply_us=core_reply_us)
        result['requested_count'] = limit
        result['head_verified'] = checked_result(result, gen, limit)
        self.flush_events()
        return result

    def query(self, text, limit=350, mode='native'):
        ticks = cpu_ticks(self.process.pid)
        cpu = process_cpu_ns(self.process.pid)
        client = time.process_time_ns()
        decode, volume = self.decode_cpu_ns, self.response_bytes
        at = time.perf_counter_ns()
        gen = self.begin_query(text, limit=limit, mode=mode)
        result = self.finish_query(gen, at)
        result.update(cpu_us=(process_cpu_ns(self.process.pid) - cpu) / 1000,
                      client_cpu_us=(time.process_time_ns() - client) / 1000,
                      decode_cpu_us=(self.decode_cpu_ns - decode) / 1000,
                      response_bytes=self.response_bytes - volume)
        result['cpu_ms_ticks'] = (cpu_ticks(self.process.pid) - ticks) * 1000 / os.sysconf('SC_CLK_TCK')
        evidence = dict(result, query=text, paths=result['paths'][:350], returned=len(result['paths']),
                        all_paths_sha256=hashlib.sha256(json.dumps(result['paths'], ensure_ascii=True).encode()).hexdigest())
        self.query_log.write(json.dumps(evidence, ensure_ascii=True) + '\n')
        self.query_log.flush()
        return result

    def startup(self, text):
        at = time.perf_counter_ns()
        self.begin_open()
        gen = self.begin_query(text)
        result = self.finish_query(gen, at)
        if self.ready is None:
            self.until(lambda obj: obj.get('t') in ['ready', 'searchready'], timeout=60)
        self.check_prior_clock(self.ready)
        result['indexed'] = self.ready['n']
        if result.get('comparable_core_ms') is None:
            result['core_ms'] = None
        result['walk_ms'] = self.ready.get('ms', self.ready.get('build_ms'))
        return result

    def audit_rank(self, query, target, count):
        if any(c in query + target for c in '\t\r\n'):
            raise ValueError('rank audit framing character')
        self.generation += 1
        gen = self.generation
        if self.name == 'flea':
            self.send(dict(c='benchaudit', query=query, target=target, gen=gen, count=count))
        else:
            self.send(f'Q\t{gen}\t{count}\t0\taudit:{target}\t{query}')
        result, _ = self.until(lambda obj: obj.get('t') == 'rank', timeout=60)
        if result.get('gen') != gen:
            raise ValueError('stale generation in rank audit')
        rank, total = result['rank'], result.get('n')
        if result.get('complete') is not True or type(total) is not int or not 0 <= total < count:
            raise ValueError('full audit lacks complete match count')
        if type(rank) is not int or not 0 <= rank <= total:
            raise ValueError('invalid full rank')
        self.flush_events()
        return dict(rank=rank, n=total)

    def verify_head(self, result, audit):
        result['head_verified'] = checked_result(result, result['gen'], result['requested_count'], audit['n'])

    def audit(self):
        if self.name == 'flea':
            self.send(dict(c='benchinventory'))
            obj, _ = self.until(lambda obj: obj.get('t') == 'inventory')
            return [(bytes.fromhex(path), kind) for path, kind in obj['identities']]
        result = self.query('', limit=int(self.ready['n']) + 16)
        return observed_identities(self.tree, result['paths'])

    def empty_window(self):
        if self.name != 'flea':
            return None
        at = time.perf_counter_ns()
        self.send(dict(c='window', start=0, count=0))
        _, stamp = self.until(lambda obj: obj.get('t') == 'rows')
        self.flush_events()
        return (stamp - at) / 1e6

    def ping(self):
        at = time.perf_counter_ns()
        self.send(dict(c='benchping') if self.name == 'flea' else 'P')
        _, stamp = self.until(lambda obj: obj.get('t') == 'pong')
        self.flush_events()
        return (stamp - at) / 1e6

    def relist_nd(self, path):
        if self.name != 'fsearch-nd':
            raise ValueError('explicit ND relist only')
        self.send('R\t' + str(path))
        obj, _ = self.until(lambda obj: obj.get('t') == 'patched')
        self.flush_events()
        return obj['ms']

    def close(self):
        if self.closed:
            return None
        self.closed = True
        try:
            self.process.stdin.close()
            # wait4 retains actual exit status and total process CPU, rather than trusting stdout alone.
            deadline = time.monotonic() + 60
            while True:
                pid, status, usage = os.wait4(self.process.pid, os.WNOHANG)
                if pid:
                    break
                if time.monotonic() >= deadline:
                    self.process.kill()
                    _, status, usage = os.wait4(self.process.pid, 0)
                    break
                time.sleep(0.02)
            self.process.returncode = os.waitstatus_to_exitcode(status)
            return dict(exit_code=self.process.returncode, cpu_ms=(usage.ru_utime + usage.ru_stime) * 1000)
        finally:
            self.selector.close()
            self.process.stdout.close()
            self.stderr.close()
            self.flush_events()
            self.log.close()
            self.query_log.close()
