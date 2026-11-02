"""A private systemd scope retains every worker until whole-cgroup CPU is read."""

import json
import os
from pathlib import Path
import subprocess
import sys
import time

# Seconds a helper may take to exit on its own after the session command ends.
GRACE_S = 5


def user_usec(path):
    # Sample input: "usage_usec 912345\nuser_usec 800000\nsystem_usec 112345\n"
    fields = dict(line.split() for line in (path / "cpu.stat").read_text().splitlines())
    value = int(fields["user_usec"])
    if value < 0:
        raise ValueError("negative cgroup user CPU")
    return value


class Scope:
    def __init__(self, root):
        self.root = root
        self.unit = "flea-b7-" + root.name + ".scope"

    def command(self, argv, env, prefix=()):
        # Scope launch uses the host's user bus; only the child receives the private application environment.
        helper = str(Path(__file__).resolve())
        marked = [sys.executable, "-B", helper, "--start", str(self.root / "cgroup-start.json"), *argv]
        return ["systemd-run", "--user", "--scope", "--quiet", "--unit=" + self.unit,
                sys.executable, "-B", helper, str(self.root / "cgroup-cpu.json"),
                json.dumps(env), *prefix, *marked]

    def result(self):
        try:
            receipt = json.loads((self.root / "cgroup-cpu.json").read_text())
            value = receipt["end_user_usec"] - receipt["start_user_usec"]
            if receipt["boundary"] == "private-session-command" and receipt["status"] == 0 and receipt["remaining_pids"] == [] and value > 0:
                return value / 1e6
        except (OSError, ValueError, KeyError, TypeError):
            pass
        return None

    def stop(self):
        subprocess.run(["systemctl", "--user", "stop", self.unit], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


def scope_group(membership):
    # Sample input: "0::/user.slice/user-1000.slice/user@1000.service/app.slice/flea-b7-sample.scope\n"
    paths = [line.removeprefix("0::") for line in membership.splitlines() if line.startswith("0::/")]
    if len(paths) != 1 or not paths[0].endswith(".scope") or "/flea-b7-" not in paths[0]:
        raise ValueError("B7 CPU collector is not in its dedicated cgroup v2 scope")
    return Path("/sys/fs/cgroup") / paths[0].lstrip("/")


def current_group():
    return scope_group(Path("/proc/self/cgroup").read_text())


def namespace_pids():
    # Sample input: os.listdir("/proc") inside bwrap gives ["1", "2", "37", "self", "meminfo"]; pid 1 is bwrap.
    own = os.getpid()
    return sorted(int(name) for name in os.listdir("/proc") if name.isdigit() and int(name) not in (1, own))


def lingering():
    deadline = time.monotonic() + GRACE_S
    while True:
        pids = namespace_pids()
        if not pids or time.monotonic() >= deadline:
            return pids
        time.sleep(0.01)


def mark_start(output, argv):
    # bwrap's pid 1 kills every helper when it exits, so only this in-sandbox parent can still see them.
    if Path("/proc/1/comm").read_text().strip() != "bwrap":
        raise ValueError("B7 start marker is not inside the bwrap pid namespace")
    group = current_group()
    start = user_usec(group)
    Path(output).write_text(json.dumps(dict(cgroup=str(group), user_usec=start, at_ns=time.monotonic_ns())) + "\n")
    status = subprocess.call(argv)
    Path(output).with_name("cgroup-namespace.json").write_text(json.dumps(dict(status=status, remaining_pids=lingering())) + "\n")
    return status


def collect(output, env, argv):
    group = current_group()
    own = str(os.getpid())
    if (group / "cgroup.procs").read_text().split() != [own]:
        raise ValueError("B7 scope was not empty before launch")
    initial = user_usec(group)
    process = subprocess.Popen(argv, env=env)
    status = process.wait()
    deadline = time.monotonic() + GRACE_S
    while True:
        groups = [group, *group.rglob("cgroup.procs")]
        pids = set()
        for item in groups:
            procs = item / "cgroup.procs" if item == group else item
            pids.update(procs.read_text().split())
        remaining = sorted(pids - {own})
        if not remaining or time.monotonic() >= deadline:
            break
        time.sleep(0.01)
    end = user_usec(group)
    start = json.loads(Path(output).with_name("cgroup-start.json").read_text())
    if start["cgroup"] != str(group) or start["user_usec"] < initial:
        raise ValueError("cgroup start boundary differs from this scope")
    inside = json.loads(Path(output).with_name("cgroup-namespace.json").read_text())
    remaining += [f"sandbox:{pid}" for pid in inside["remaining_pids"]]
    Path(output).write_text(json.dumps(dict(cgroup=str(group), boundary="private-session-command",
        start_ns=start["at_ns"], end_ns=time.monotonic_ns(), initial_user_usec=initial,
        start_user_usec=start["user_usec"], end_user_usec=end, remaining_pids=remaining, status=status)) + "\n")
    return status if not remaining else 1


if __name__ == "__main__":
    if sys.argv[1] == "--start":
        sys.exit(mark_start(sys.argv[2], sys.argv[3:]))
    else:
        sys.exit(collect(sys.argv[1], json.loads(sys.argv[2]), sys.argv[3:]))
