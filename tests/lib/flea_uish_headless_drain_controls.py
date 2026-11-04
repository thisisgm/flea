"""EY-25: the headless host's owned teardown waits out a driver that is still draining its windows, instead of forcing it."""
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time

import flea_uish_headless
from flea_transport import MARKER, REPO, drain_timeout
from flea_transport_lifecycle import identity, subreaper

# Longer than the 3 s grace the host used to give, well inside the transport's drain deadline.
HELD_S = 4.5
DRIVER = "import sys, time\nsys.stdin.buffer.read()\ntime.sleep(float(sys.argv[1]))\n"
# A wedged driver: SIGTERM is ignored, so only the forced stop's SIGKILL after its grace ends it; it says ready once ignoring.
WEDGED = ("import signal, sys, time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nprint('ready', flush=True)\n"
          "sys.stdin.buffer.read()\ntime.sleep(float(sys.argv[1]))\n")
# Slack over the forced grace for SIGKILL and the reap, far under the 30 s drain deadline.
FORCED_SLACK_S = 2


def held_host(held_s, driver=DRIVER):
    """A Host whose driver holds one drain slot: it reads to the end of its commands, then drains for held_s before it exits."""
    parent = REPO / ".superpowers/m9-uish-02"
    parent.mkdir(parents=True, exist_ok=True)
    host = flea_uish_headless.Host.__new__(flea_uish_headless.Host)
    host.drain_s = drain_timeout()
    host.root = Path(tempfile.mkdtemp(prefix="host.", dir=parent)).resolve()
    (host.root / MARKER).write_text("owned GPUI headless host (held drain control)\n")
    environment = dict(PATH=os.environ.get("PATH", "/usr/bin:/bin"), FLEA_TEST_WINDOW=str(host.root))
    host.log = (host.root / "driver.log").open("wb")
    host.selector = selectors.DefaultSelector()
    subreaper()
    host.process = subprocess.Popen([sys.executable, "-B", "-c", driver, str(held_s)], env=environment,
                                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    start_new_session=True)
    current = identity(host.process.pid)
    host.record = dict(root=str(host.root), pid=host.process.pid, group=host.process.pid,
                       start=current["start"], leader_start=current["start"])
    host.selector.register(host.process.stdout, selectors.EVENT_READ)
    return host


def fixreaders_ey25_owned_teardown_waits_for_a_held_drain(check):
    check(drain_timeout() > HELD_S, "the drain deadline leaves room for the held drain")
    host = held_host(HELD_S)
    started = time.monotonic()
    error = None
    try:
        host.stop()
    except Exception as failure:
        error = failure
    elapsed = time.monotonic() - started
    check(error is None, f"owned teardown forced a draining driver: {error}")
    check(elapsed >= HELD_S - 0.1, f"teardown returned before the drain ended: {elapsed:.2f} s")
    check(host.process.returncode == 0, f"the driver did not exit on its own: {host.process.returncode}")
    check(not host.root.exists(), "the marked host root survived teardown")
    print(f"PASS fixreaders_ey25 held={HELD_S} s elapsed={elapsed:.2f} s grace={drain_timeout()} s")


def fixreaders_ey25_forced_teardown_keeps_the_short_grace(check):
    check(drain_timeout() > flea_uish_headless.TEARDOWN_GRACE_S + FORCED_SLACK_S,
          "the drain deadline is longer than a forced stop")
    host = held_host(drain_timeout(), WEDGED)
    check(host.process.stdout.readline() == b"ready\n", "the wedged driver never said it ignores SIGTERM")
    started = time.monotonic()
    host.stop(terminate=True)
    elapsed = time.monotonic() - started
    check(elapsed >= flea_uish_headless.TEARDOWN_GRACE_S, f"a forced stop killed after {elapsed:.2f} s, inside its grace")
    check(elapsed < flea_uish_headless.TEARDOWN_GRACE_S + FORCED_SLACK_S,
          f"a forced stop waited {elapsed:.2f} s, past its {flea_uish_headless.TEARDOWN_GRACE_S} s grace")
    check(host.process.returncode == -signal.SIGKILL, f"the wedged driver was not killed: {host.process.returncode}")
    check(not host.root.exists(), "the marked host root survived teardown")
    print(f"PASS fixreaders_ey25_forced elapsed={elapsed:.2f} s grace={flea_uish_headless.TEARDOWN_GRACE_S} s")


if __name__ == "__main__":
    def check(condition, message):
        if not condition:
            raise AssertionError(message)
    fixreaders_ey25_owned_teardown_waits_for_a_held_drain(check)
    fixreaders_ey25_forced_teardown_keeps_the_short_grace(check)
