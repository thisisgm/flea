"""Hold real copy reads until the test releases each invocation."""
from pathlib import Path
import subprocess
import time

# A held copy reaches its controlled read well inside this, even on a loaded runner.
READY_DEADLINE_S = 20
# How often the ready marker is polled while the copy is on its way to the read.
READY_POLL_S = 0.005


class CopyBarrier:
    def __init__(self, root):
        self.root = root
        self.directory = root / 'barriers'
        self.library = root / 'copy-barrier.so'

    @classmethod
    def build(cls, root):
        assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
        barrier = cls(root)
        barrier.directory.mkdir()
        subprocess.run(['cc', '-shared', '-fPIC', '-Wall', '-Werror', '-o',
                        str(barrier.library), str(Path(__file__).with_name('jobs-barrier.c'))], check=True)
        return barrier

    def environment(self, env):
        if self.library.is_file():
            env['LD_PRELOAD'] = str(self.library)
            env['FLEA_JOBS_BARRIER'] = str(self.root)

    def ready(self):
        return set(self.directory.glob('*.ready'))

    def wait(self, before):
        deadline = time.monotonic() + READY_DEADLINE_S
        while True:
            ready = self.ready() - before
            if ready:
                assert len(ready) == 1, ready
                return next(iter(ready))
            assert time.monotonic() < deadline, 'copy never reached its controlled read'
            time.sleep(READY_POLL_S)

    def release(self, ready):
        assert ready.is_absolute() and ready.parent == self.directory
        assert (self.root / '.flea-test-sandbox').is_file()
        ready.with_suffix('.release').touch()

    def close(self):
        assert self.root.is_absolute() and (self.root / '.flea-test-sandbox').is_file()
        (self.directory / 'release-all').touch()
