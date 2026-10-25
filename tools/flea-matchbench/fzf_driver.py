# fzf entrant for flea-matchbench: one fzf process per run holds the corpus, queried through its --listen server.
import fcntl
import json
import os
import secrets
import socket
import struct
import subprocess
import sys
import termios
import threading
import time
import urllib.request


def emit(value):
    sys.stdout.write(json.dumps(value, separators=(',', ':')) + '\n')
    sys.stdout.flush()


def schedstat_ns(pid):
    # Sum of on-CPU nanoseconds over every live task; fzf's Go threads persist for the process lifetime.
    total = 0
    for task in os.listdir(f'/proc/{pid}/task'):
        try:
            with open(f'/proc/{pid}/task/{task}/schedstat') as handle:
                total += int(handle.read().split()[0])
        except (FileNotFoundError, ProcessLookupError):
            continue
    return total


def free_port():
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        return probe.getsockname()[1]


class Fzf:
    def __init__(self, binary, corpus, threads):
        self.key = secrets.token_hex(16)
        self.port = free_port()
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 200, 0, 0))
        env = {'PATH': '/usr/bin:/bin', 'TERM': 'xterm-256color', 'HOME': os.environ.get('HOME', '/'),
               'FZF_API_KEY': self.key, 'GOMAXPROCS': str(threads), 'LC_ALL': 'C.UTF-8'}

        def controlling_tty():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        self.corpus = open(corpus, 'rb')
        argv = [binary, '--scheme=path', '--sync', f'--listen=127.0.0.1:{self.port}', '--no-mouse']
        self.process = subprocess.Popen(argv, stdin=self.corpus, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                        env=env, preexec_fn=controlling_tty, pass_fds=(slave,), close_fds=True)
        os.close(slave)
        self.master = master
        # The interface draws into the pty; draining it keeps fzf from blocking on a full terminal buffer.
        threading.Thread(target=self.drain, daemon=True).start()

    def drain(self):
        try:
            while os.read(self.master, 65536):
                pass
        except OSError:
            pass

    def request(self, method, body=None, limit=None):
        url = f'http://127.0.0.1:{self.port}/' + (f'?limit={limit}' if limit is not None else '')
        data = None if body is None else body.encode()
        request = urllib.request.Request(url, data=data, method=method, headers={'x-api-key': self.key})
        with urllib.request.urlopen(request, timeout=600) as response:
            return response.read()

    def state(self, limit):
        while True:
            try:
                raw = self.request('GET', limit=limit)
            except OSError:
                if self.process.poll() is not None:
                    raise RuntimeError('fzf exited')
                time.sleep(0.005)
                continue
            if raw:
                return json.loads(raw)
            time.sleep(0.0005)

    def settle(self, text, limit):
        # Final means: the query reached the prompt, input reading ended and the matcher reported 100 percent.
        while True:
            status = self.state(limit)
            if status['query'] == text and not status['reading'] and status['progress'] == 100:
                return status
            time.sleep(0.0005)

    def query(self, text, limit):
        self.request('POST', body='change-query:' + text)
        return self.settle(text, limit)


def main():
    binary, threads = sys.argv[1], int(sys.argv[2])
    cpus = len(os.sched_getaffinity(0))
    if cpus != threads:
        raise SystemExit(f'CPU affinity {cpus} does not match the requested thread budget {threads}')
    emit(dict(t='boot', cpus=cpus, engine='fzf', profile='native', version=os.environ.get('FZF_VERSION', ''),
              licence='MIT', parallel='library matcher partitions, GOMAXPROCS'))
    fzf = None
    try:
        for line in sys.stdin:
            fields = line.rstrip('\n').split('\t', 5)
            if fields[0] == 'O' and len(fields) == 2 and fzf is None:
                at = time.perf_counter_ns()
                fzf = Fzf(binary, fields[1], threads)
                status = fzf.settle('', 1)
                emit(dict(t='ready', n=status['totalCount'], build_ms=(time.perf_counter_ns() - at) / 1e6,
                          build_cpu_ns=schedstat_ns(fzf.process.pid), read_ms=None))
            elif fields[0] == 'Q' and len(fields) == 6 and fzf:
                gen, limit, offset, text = int(fields[1]), int(fields[2]), int(fields[3]), fields[5]
                cpu = schedstat_ns(fzf.process.pid)
                at = time.perf_counter_ns()
                status = fzf.query(text, limit + offset)
                core = (time.perf_counter_ns() - at) / 1e6
                paths = [item['text'] for item in status['matches']][offset:offset + limit]
                emit(dict(t='result', gen=gen, n=status['matchCount'], paths=paths, core_ms=core,
                          cpu_ns=schedstat_ns(fzf.process.pid) - cpu, clock='observed over --listen, 0.5 ms polls'))
            elif fields[0] == 'A' and len(fields) == 4 and fzf:
                gen, target, text = int(fields[1]), fields[2], fields[3]
                at = time.perf_counter_ns()
                status = fzf.query(text, 1)
                status = fzf.state(status['matchCount'])
                core = (time.perf_counter_ns() - at) / 1e6
                texts = [item['text'] for item in status['matches']]
                rank = texts.index(target) + 1 if target in texts else 0
                emit(dict(t='audit', gen=gen, n=status['matchCount'], rank=rank, core_ms=core))
            elif fields[0] == 'P':
                emit(dict(t='pong'))
            elif fields[0] == 'X':
                break
            else:
                raise SystemExit('invalid command: ' + fields[0])
    finally:
        if fzf:
            fzf.process.terminate()
            try:
                fzf.process.wait(10)
            except subprocess.TimeoutExpired:
                fzf.process.kill()
            fzf.corpus.close()


if __name__ == '__main__':
    main()
