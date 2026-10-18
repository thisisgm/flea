"""Require the real harness helper to stop when its absolute deadline passes."""
import ast
from pathlib import Path
import queue
import types

TIMEOUT_SECONDS = 0.020
TICK_SECONDS = 0.001
EARLY_REPLY_SECONDS = 0.005
LATE_REPLY_SECONDS = 1.000

# Importing the driver starts a backend, so exercise its actual helper through its syntax tree.
tree = ast.parse(Path("tests/empty-trash.py").read_text())
helper = next(node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name == "until")
clock = types.SimpleNamespace(now=0.0)


class Traffic:
    def __init__(self, finish=LATE_REPLY_SECONDS):
        self.finish = finish

    def get(self, timeout):
        clock.now += TICK_SECONDS
        if clock.now >= self.finish:
            return {"t": "transferdone"}
        return {"t": "transferprogress"}


context = {"held": [], "messages": Traffic(), "queue": queue, "time": types.SimpleNamespace(monotonic=lambda: clock.now)}
exec(compile(ast.Module(body=[helper], type_ignores=[]), "tests/empty-trash.py", "exec"), context)
context["messages"] = Traffic(EARLY_REPLY_SECONDS)
assert context["until"](lambda message: message["t"] == "transferdone", TIMEOUT_SECONDS)["t"] == "transferdone"
assert 0 < clock.now < TIMEOUT_SECONDS


class Quiet:
    def get(self, timeout):
        clock.now += timeout
        raise queue.Empty


clock.now = 0.0
context["messages"] = Quiet()
try:
    context["until"](lambda message: message["t"] == "transferdone", TIMEOUT_SECONDS)
except queue.Empty:
    assert clock.now == TIMEOUT_SECONDS
else:
    raise AssertionError("a quiet queue must time out")
print("PASS controls: matching message before deadline; quiet queue times out")
clock.now = 0.0
saved = {"t": "transferdone", "id": "held"}
context["held"] = [{"t": "jobs"}, saved]
assert context["until"](lambda message: message["t"] == "transferdone", TIMEOUT_SECONDS) == saved
assert context["held"] == [{"t": "jobs"}] and clock.now == 0.0
context["held"] = []
print("PASS held messages: matching reply returned; unrelated reply preserved")
clock.now = 0.0
context["messages"] = Traffic()
try:
    reply = context["until"](lambda message: message["t"] == "transferdone", TIMEOUT_SECONDS)
except queue.Empty:
    assert TIMEOUT_SECONDS <= clock.now < TIMEOUT_SECONDS + TICK_SECONDS, "expiry must not happen before the budget is spent"
    print(f"PASS deadline enforced at virtual_seconds={clock.now:.3f}")
else:
    print(f"OBSERVED deadline={TIMEOUT_SECONDS:.3f} virtual_seconds={clock.now:.3f} reply={reply}")
    raise AssertionError("unrelated progress must not keep until alive past its absolute deadline")
