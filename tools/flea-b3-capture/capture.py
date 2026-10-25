#!/usr/bin/env python3
"""B3 installed capture: open to first presented result and indexed PSS for one million-file search.

One run launches the exact `flea --gui` launcher on a census-bound corpus, types the search key and
query through real Wayland input as soon as the window maps, and reads the same process's
flea.searchStamps. The first result time is the compositor's presentation time of the first
submitted frame that painted the query's own rows (CLOCK_MONOTONIC, wp_presentation), minus the
launch time on the same clock. PSS is the sum of smaps_rollup over every live process of the run's
cgroup except the driver itself, sampled once the answer settled on a complete index. A missing
seam, a partial or mismatched index, a foreign process or a failed close blocks the run; nothing
is estimated. Exit 0 validated capture, 3 blocked, 2 bad input. See README.md.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import shutil
import sys
import tempfile
import time

SCHEMA = 'flea.b3.capture.v1'
CELLS = ('B3:million-file-search:open_to_first_result_ms', 'B3:million-file-search:indexed_pss_mib')
CLOCK_MONOTONIC = 1
QUERY = re.compile(r'[a-z0-9]{1,32}')
FIELDS = ('gen', 'keyAt', 'firstRowsAt', 'settledAt', 'query', 'indexed', 'total', 'walking',
          'cancelled', 'partial', 'firstRow', 'firstName', 'presentedFirst', 'presentedSettled', 'coverage')


class Blocked(Exception):
    def __init__(self, code, detail):
        super().__init__(f'{code}: {detail}')
        self.code, self.detail = code, detail


def need(value, code, detail):
    if not value:
        raise Blocked(code, detail)


def sha256(path):
    with open(path, 'rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write_new(path, value):
    with open(path, 'x') as output:
        output.write(json.dumps(value, indent=2, sort_keys=True) + '\n')


def census(root, out):
    """Count every non-hidden entry below root the way the search walk does: no root, no symlink descent."""
    root = Path(root)
    need(root.is_absolute() and root.resolve(strict=True) == root and root.is_dir(), 'corpus_root', str(root))
    files = directories = 0
    digest = hashlib.sha256()
    stack = [root]
    while stack:
        here = stack.pop()
        with os.scandir(here) as entries:
            names = sorted((entry.name, entry.is_dir(follow_symlinks=False)) for entry in entries
                           if not entry.name.startswith('.'))
        for name, is_dir in names:
            rel = (here / name).relative_to(root).as_posix()
            digest.update(f'{"d" if is_dir else "f"} {rel}\0'.encode('utf-8', 'surrogateescape'))
            if is_dir:
                directories += 1
                stack.append(here / name)
            else:
                files += 1
    manifest = dict(root=str(root), files=files, directories=directories, hidden=False,
                    names_sha256=digest.hexdigest(), census='flea-b3-capture walk, non-hidden, root excluded')
    write_new(out, manifest)
    return manifest


def load_corpus(path):
    corpus = json.loads(Path(path).read_text())
    for field in ('root', 'files', 'directories', 'names_sha256'):
        need(field in corpus, 'corpus_fields', field)
    root = Path(corpus['root'])
    need(root.is_absolute() and root.resolve(strict=True) == root and root.is_dir(), 'corpus_root', str(root))
    need(type(corpus['files']) is int and type(corpus['directories']) is int
         and corpus['files'] >= 1 and corpus['directories'] >= 0, 'corpus_counts', 'integer populations')
    need(corpus.get('hidden', False) is False, 'corpus_hidden', 'the default search skips hidden names')
    need(re.fullmatch(r'[0-9a-f]{64}', str(corpus['names_sha256'])) is not None, 'corpus_census', 'names_sha256')
    return corpus


def proc_rows():
    rows = {}
    for entry in Path('/proc').iterdir():
        if not entry.name.isdecimal():
            continue
        try:
            words = (entry / 'stat').read_text().rsplit(') ', 1)[1].split()
            rows[int(entry.name)] = dict(ppid=int(words[1]), state=words[0], session=int(words[3]),
                                         start=int(words[19]), cgroup=(entry / 'cgroup').read_text().strip())
        except (FileNotFoundError, ProcessLookupError, IndexError):
            continue
    return rows


def descendants(root, rows):
    found, frontier = {root}, [root]
    while frontier:
        parent = frontier.pop()
        for pid, row in rows.items():
            if row['ppid'] == parent and pid not in found:
                found.add(pid)
                frontier.append(pid)
    return found


def pss_kb(pid):
    try:
        text = (Path('/proc') / str(pid) / 'smaps_rollup').read_text()
    except (FileNotFoundError, ProcessLookupError) as error:
        raise Blocked('member_set_changed', f'{pid} left during the sample: {error}') from error
    for line in text.splitlines():
        if line.startswith('Pss:'):
            return int(line.split()[1])
    raise Blocked('pss_unreadable', str(pid))


class Run:
    def __init__(self, args):
        self.args = args
        self.out = Path(args.output)
        self.evidence = dict(schema=SCHEMA, cells=dict.fromkeys(CELLS), status='BLOCKED')
        self.proc = None
        self.launched = False
        self.owns_output = False
        self.deadline = time.monotonic() + args.timeout
        self.tools = dict(os.environ)
        self.me = os.getpid()

    def tool(self, argv, seconds=None, env=None, cleanup=False):
        seconds = self.args.tool_timeout if seconds is None else seconds
        # Cleanup keeps its own short bound, so a run that timed out still closes through its IPC.
        remaining = seconds if cleanup else min(seconds, self.deadline - time.monotonic())
        need(remaining > 0, 'timeout', 'run deadline passed before ' + argv[0])
        try:
            done = subprocess.run(argv, env=self.tools if env is None else env, stdin=subprocess.DEVNULL,
                                  capture_output=True, text=True, timeout=remaining)
        except (subprocess.TimeoutExpired, OSError) as error:
            raise Blocked('tool_timeout' if isinstance(error, subprocess.TimeoutExpired) else 'tool_failed',
                          f'{argv[0]} {argv[1:3]}: {error}') from error
        need(done.returncode == 0, 'tool_failed', f'{argv[0]} {argv[1:3]} exit {done.returncode}: {done.stderr.strip()[:300]}')
        return done.stdout.strip()

    def ipc(self, function, cleanup=False):
        argv = [self.launcher, 'ipc', '--pid', str(self.gui_pid), 'call', 'flea', function]
        return self.tool(argv, env=self.appenv, cleanup=cleanup)

    def prepare(self):
        args = self.args
        need(QUERY.fullmatch(args.query) is not None, 'query_text', 'lowercase ASCII letters and digits, 1 to 32')
        self.launcher = str(Path(args.flea))
        need(Path(self.launcher).is_absolute() and os.access(self.launcher, os.X_OK), 'launcher', self.launcher)
        self.gui = str(Path(self.launcher).resolve(strict=True).with_name('flea-gui'))
        need(os.access(self.gui, os.X_OK), 'wrong_sibling', 'launcher has no executable flea-gui sibling')
        try:
            self.corpus = load_corpus(args.corpus)
        except (OSError, ValueError) as error:
            raise Blocked('corpus_fields', f'{args.corpus}: {error}') from error
        try:
            self.out.mkdir(mode=0o700)
        except OSError as error:
            raise Blocked('output_exists', f'{self.out}: {error}') from error
        self.owns_output = True
        (self.out / '.flea-test-sandbox').touch()
        self.home = self.out / 'home'
        for part in ('.config', '.cache', '.local/state', '.local/share'):
            (self.home / part).mkdir(parents=True, mode=0o700)
        runtime = os.environ.get('XDG_RUNTIME_DIR', '')
        display = os.environ.get('WAYLAND_DISPLAY', '')
        need(re.fullmatch(r'[A-Za-z0-9._-]+', display) is not None, 'session', 'WAYLAND_DISPLAY name')
        socket = Path(runtime) / display
        need(socket.exists() and stat.S_ISSOCK(socket.stat().st_mode), 'session', 'Wayland socket')
        # A short private runtime keeps Hyprland's event socket path under the 108-byte socket limit.
        self.runtime = Path(tempfile.mkdtemp(prefix='b3r.', dir='/tmp'))
        if (Path(runtime) / 'hypr').is_dir():
            (self.runtime / 'hypr').symlink_to(Path(runtime) / 'hypr', target_is_directory=True)
        self.appenv = {k: v for k, v in os.environ.items() if not k.startswith('FLEA_')}
        self.appenv.update(HOME=str(self.home), XDG_CONFIG_HOME=str(self.home / '.config'),
                           XDG_CACHE_HOME=str(self.home / '.cache'), XDG_STATE_HOME=str(self.home / '.local/state'),
                           XDG_DATA_HOME=str(self.home / '.local/share'), XDG_RUNTIME_DIR=str(self.runtime),
                           WAYLAND_DISPLAY=str(socket), FLEA_FRONTEND='gpui')
        # Artifact hashes are read before the launch clock starts, never inside the timed interval.
        self.evidence['identity'] = dict(launcher=self.launcher, launcher_sha256=sha256(self.launcher),
                                         gui=self.gui, gui_sha256=sha256(self.gui),
                                         version=self.tool([self.launcher, '--version'], env=self.appenv))
        self.evidence['corpus'] = self.corpus
        self.evidence['query'] = args.query
        self.evidence['cache'] = 'as found; this driver never drops caches'
        # Clients already on the display stay untouched; only a new window of the owned PID counts.
        self.before = {row['address'] for row in self.clients()}

    def clients(self):
        return [row for row in json.loads(self.tool(['hyprctl', 'clients', '-j'])) if row.get('mapped', True)]

    def launch(self):
        log = (self.out / 'gui.log').open('x')
        self.open_ns = time.monotonic_ns()
        self.proc = subprocess.Popen([self.launcher, '--gui', self.corpus['root']], env=self.appenv, cwd=self.out,
                                     stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                                     start_new_session=True)
        self.gui_pid = self.proc.pid
        self.launched = True
        self.seen = {}
        while True:
            self.track()
            need(self.proc.poll() is None, 'gui_exited', f'exit {self.proc.returncode} during startup')
            mapped = [row for row in self.clients() if row['address'] not in self.before]
            need(len(mapped) <= 1, 'foreign_client', 'a second client mapped during launch')
            if mapped:
                break
            need(time.monotonic() < self.deadline, 'timeout', 'window never mapped')
            time.sleep(0.005)
        self.mapped_ns = time.monotonic_ns()
        window = mapped[0]
        need(window.get('pid') == self.gui_pid, 'foreign_client', f'mapped pid {window.get("pid")} is not {self.gui_pid}')
        exe = Path(f'/proc/{self.gui_pid}/exe').resolve()
        script = (Path(f'/proc/{self.gui_pid}/cmdline').read_bytes().split(b'\0') + [b'', b''])[1]
        need(exe == Path(self.gui).resolve() or (self.args.offline and os.fsdecode(script) == self.gui),
             'wrong_sibling', f'launcher did not exec its sibling: {exe}')
        self.address = window['address']
        self.evidence['window'] = dict(pid=self.gui_pid, address=self.address, cls=window.get('class'))

    def type_query(self):
        # Keys go in once the window says it is usable, as a person would type; that wait stays inside the interval.
        while self.ipc('ready') != 'true':
            need(self.proc.poll() is None, 'gui_exited', f'exit {self.proc.returncode} before ready')
            need(time.monotonic() < self.deadline, 'timeout', 'window never ready')
            time.sleep(0.005)
        self.ready_ns = time.monotonic_ns()
        focus = 'hl.dsp.focus({ window = "address:' + self.address + '" })'
        need(self.tool(['hyprctl', 'dispatch', focus]) == 'ok', 'focus_refused', self.address)
        while json.loads(self.tool(['hyprctl', 'activewindow', '-j'])).get('address') != self.address:
            need(time.monotonic() < self.deadline, 'timeout', 'owned window never took focus')
            time.sleep(0.005)
        self.evidence['active_at_key'] = self.tool(['hyprctl', 'activewindow', '-j'])
        self.key_ns = time.monotonic_ns()
        self.tool(['wtype', 'f' + self.args.query])
        self.typed_ns = time.monotonic_ns()
        stop = time.monotonic() + 3
        while True:
            delivered = parse_object(self.ipc('keyDeliveryState'), 'keyDeliveryState')
            if delivered.get('searchQuery') == self.args.query:
                break
            need(time.monotonic() < stop, 'query_delivery', 'typed keys did not reach the search: ' + json.dumps(delivered))
            time.sleep(0.02)

    def track(self):
        # Every member ever seen, by pid and start time, so a child reparented after a crash is still the run's.
        rows = proc_rows()
        for pid in descendants(self.gui_pid, rows) | {p for p, row in rows.items() if row['session'] == self.gui_pid}:
            if pid in rows:
                self.seen.setdefault(pid, rows[pid]['start'])

    def live_members(self):
        rows = proc_rows()
        return {pid for pid, start in self.seen.items() if pid in rows and rows[pid]['start'] == start
                and rows[pid]['state'] != 'Z'} | {pid for pid, row in rows.items()
                                                   if row['session'] == self.gui_pid and row['state'] != 'Z'}

    def settle(self):
        # Polled sparsely: every reported time is the compositor's, so the poll rate never moves a result.
        while True:
            self.track()
            need(self.proc.poll() is None, 'gui_exited', f'exit {self.proc.returncode} before the answer settled')
            try:
                raw = self.ipc('searchStamps')
            except Blocked as error:
                # One slow answer while a large index builds is retried until the run deadline.
                need(time.monotonic() < self.deadline, 'timeout', 'answer never settled: ' + error.detail)
                need(error.code == 'tool_timeout', error.code, error.detail)
                continue
            self.evidence['last_stamps'] = raw
            stamps = parse_stamps(raw)
            if (stamps['query'] == self.args.query and stamps['settledAt'] > 0 and stamps['walking'] is False
                    and stamps['presentedSettled'] is not None and stamps['coverage'] != 'pending'):
                self.raw_stamps = raw
                return stamps
            need(time.monotonic() < self.deadline, 'timeout', 'answer never settled: ' + raw[:400])
            time.sleep(self.args.poll)

    def validate(self, stamps):
        corpus = self.corpus
        delivered = parse_object(self.ipc('keyDeliveryState'), 'keyDeliveryState')
        need(delivered.get('searchQuery') == self.args.query, 'query_delivery', json.dumps(delivered))
        need(type(stamps['gen']) is int and stamps['gen'] >= 1, 'stale_generation', str(stamps['gen']))
        need(stamps['cancelled'] is False and stamps['partial'] is False, 'partial_index',
             f'cancelled={stamps["cancelled"]} partial={stamps["partial"]}')
        entries = corpus['files'] + corpus['directories']
        need(stamps['indexed'] == entries, 'index_census', f'index holds {stamps["indexed"]}, census {entries}')
        need(type(stamps['total']) is int and stamps['total'] >= 1, 'empty_result', 'no match')
        first = stamps['presentedFirst']
        need(first is not None, 'missing_presented_clock', 'paint is not presentation')
        need(stamps['coverage'] == 'complete', 'feedback_lost', f'coverage {stamps["coverage"]}: a lost submission may have been first')
        for shown in (first, stamps['presentedSettled']):
            need(shown['clock'] == CLOCK_MONOTONIC and type(shown['ns']) is int, 'clock_domain', json.dumps(shown))
        need(stamps['firstRow'] == 0, 'not_first_result', f'first presented row {stamps["firstRow"]}')
        name = stamps['firstName']
        need(isinstance(name, str) and name and not name.startswith('/') and '..' not in name.split('/'),
             'raw_name', repr(name))
        need(fuzzy(self.args.query, name), 'not_a_match', f'{name!r} does not contain {self.args.query!r} in order')
        need(self.open_ns < self.key_ns <= first['ns'] <= stamps['presentedSettled']['ns'] <= time.monotonic_ns(),
             'clock_order', f'open {self.open_ns} key {self.key_ns} first {first["ns"]} settled {stamps["presentedSettled"]["ns"]}')
        return first['ns']

    def memory(self):
        self.track()
        rows = proc_rows()
        mine = rows[self.me]['cgroup']
        members = self.live_members()
        helpers = descendants(self.me, rows) - members
        cgroup = {pid for pid, row in rows.items() if row['cgroup'] == mine and row['state'] != 'Z'}
        foreign = cgroup - members - helpers
        need(not self.args.cgroup_check or not foreign, 'foreign_process', f'cgroup {mine} holds {sorted(foreign)} outside the run')
        before = {pid: rows[pid]['start'] for pid in members}
        sample = {pid: pss_kb(pid) for pid in sorted(members)}
        self.track()
        after = proc_rows()
        need(all(pid in after and after[pid]['start'] == start for pid, start in before.items())
             and self.live_members() == members, 'member_set_changed', 'a process came or went during the sample')
        self.evidence['memory'] = dict(cgroup=mine, pids=sample,
                                       scope='launcher tree and session, every member seen since launch')
        return sum(sample.values()) / 1024

    def close(self):
        if self.proc is None:
            return
        self.track()
        code = self.proc.poll()
        if code is None:
            try:
                self.ipc('quit', cleanup=True)
                code = self.proc.wait(timeout=10)
            except (Blocked, subprocess.TimeoutExpired) as error:
                code = None
                self.evidence['close_error'] = str(error)
        stop = time.monotonic() + 10
        while self.live_members() and time.monotonic() < stop:
            time.sleep(0.05)
        left = self.live_members()
        if code is None or left:
            if code is None:
                self.proc.kill()
            for sig in (signal.SIGTERM, signal.SIGKILL):
                for pid in self.live_members():
                    try:
                        os.kill(pid, sig)
                    except ProcessLookupError:
                        pass
                time.sleep(0.5)
            self.proc.poll()
        self.evidence['close'] = dict(exit_code=code, survivors=sorted(left), after_kill=sorted(self.live_members()))
        self.proc = None
        need(code == 0 and not left, 'failed_close', f'exit {code}, survivors {sorted(left)}')

    def run(self):
        try:
            try:
                self.prepare()
                cells = self.measure()
            except (KeyError, TypeError, ValueError, OSError) as error:
                raise Blocked('driver_error', f'{type(error).__name__}: {error}') from error
        except Blocked:
            try:
                self.close()
            except Blocked as error:
                self.evidence['close_blocked'] = error.detail
            self.drop_runtime()
            raise
        self.close()
        self.drop_runtime()
        self.evidence['cells'] = cells
        self.evidence['status'] = 'OFFLINE_CONTROL' if self.args.offline else 'CAPTURE_VALIDATED_NOT_ACCEPTANCE'

    def drop_runtime(self):
        runtime = getattr(self, 'runtime', None)
        if runtime is not None and runtime.parent == Path('/tmp') and runtime.name.startswith('b3r.'):
            shutil.rmtree(runtime, ignore_errors=True)

    def measure(self):
        self.launch()
        self.type_query()
        stamps = self.settle()
        presented = self.validate(stamps)
        pss = self.memory()
        self.evidence['stamps'] = json.loads(self.raw_stamps)
        self.evidence['clocks_ns'] = dict(open=self.open_ns, mapped=self.mapped_ns, ready=self.ready_ns, key=self.key_ns,
                                          typed=self.typed_ns, presented_first=presented,
                                          presented_settled=stamps['presentedSettled']['ns'])
        self.evidence['components_ms'] = dict(open_to_mapped=(self.mapped_ns - self.open_ns) / 1e6,
                                              mapped_to_ready=(self.ready_ns - self.mapped_ns) / 1e6,
                                              ready_to_key=(self.key_ns - self.ready_ns) / 1e6,
                                              key_to_presented_first=(presented - self.key_ns) / 1e6)
        return {CELLS[0]: (presented - self.open_ns) / 1e6, CELLS[1]: pss}


def parse_object(raw, name):
    try:
        value = json.loads(raw)
    except ValueError as error:
        raise Blocked('malformed_reply', f'{name}: {raw[:200]!r}') from error
    need(isinstance(value, dict), 'malformed_reply', f'{name}: {raw[:200]!r}')
    return value


def parse_stamps(raw):
    stamps = parse_object(raw, 'searchStamps')
    missing = [field for field in FIELDS if field not in stamps]
    need(not missing, 'missing_seam', 'searchStamps lacks ' + ','.join(missing))
    for field in ('gen', 'keyAt', 'firstRowsAt', 'settledAt'):
        need(type(stamps[field]) is int, 'malformed_reply', f'{field} {stamps[field]!r}')
    need(isinstance(stamps['query'], str), 'malformed_reply', 'query')
    return stamps


def fuzzy(query, name):
    rest = iter(name.lower())
    return all(char in rest for char in query)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest='command', required=True)
    run = sub.add_parser('run')
    run.add_argument('--flea', required=True, help='absolute launcher path; its sibling flea-gui is the window')
    run.add_argument('--corpus', required=True, help='census manifest from the census command or BENCH-07')
    run.add_argument('--query', required=True)
    run.add_argument('--output', required=True, help='new directory for this run')
    run.add_argument('--timeout', type=float, default=600)
    run.add_argument('--poll', type=float, default=0.25)
    run.add_argument('--tool-timeout', type=float, default=5, help='bound on each hyprctl, wtype or IPC call')
    run.add_argument('--offline', action='store_true', help='fake-tool control: script GUI, no cgroup check, never a cell')
    run.add_argument('--check-cgroup', action='store_true', help='keep the cgroup stray check under --offline')
    take = sub.add_parser('census')
    take.add_argument('root')
    take.add_argument('output')
    args = parser.parse_args(argv)
    if args.command == 'run':
        args.cgroup_check = not args.offline or args.check_cgroup
    if args.command == 'census':
        try:
            print(json.dumps(census(args.root, args.output), sort_keys=True))
            return 0
        except Blocked as error:
            print(f'BLOCKED {error}', file=sys.stderr)
            return 2
    capture = Run(args)
    try:
        try:
            capture.run()
        except (KeyError, TypeError, ValueError, OSError) as error:
            raise Blocked('driver_error', f'{type(error).__name__}: {error}') from error
        rc = 0
    except Blocked as error:
        capture.evidence.update(blocked=dict(code=error.code, detail=error.detail))
        rc = 2 if error.code in ('query_text', 'launcher', 'wrong_sibling', 'corpus_fields', 'corpus_root',
                                 'corpus_counts', 'corpus_hidden', 'corpus_census', 'output_exists') and not capture.launched else 3
    if capture.owns_output:
        write_new(capture.out / 'RESULT.json', capture.evidence)
    print(json.dumps(dict(status=capture.evidence['status'], cells=capture.evidence['cells'],
                          blocked=capture.evidence.get('blocked')), sort_keys=True))
    return rc


if __name__ == '__main__':
    sys.exit(main())
