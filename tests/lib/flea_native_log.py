"""Native diagnostics have no Qt exemptions; deliberate messages keep exact accounting."""
from pathlib import Path
from collections import Counter
import re
import sys

DIAGNOSTIC = re.compile(r"^flea-gui:|warn|error|panic|fatal|refused|cannot|\bfailed\b|FLEA_IPC_NOT_PORTED", re.I)


def message(line):
    return line.removeprefix("flea-gui: ")


def unexpected(log, expected):
    allowed = {message(value) for value in expected}
    return [line for line in log.splitlines()
            if DIAGNOSTIC.search(line) and message(line) not in allowed]


def check(log, expected):
    errors = unexpected(log, expected)
    observed = Counter(message(line) for line in log.splitlines())
    for warning, wanted in Counter(message(value) for value in expected).items():
        if observed[warning] != wanted:
            errors.append(f"expected native warning count={observed[warning]}, registered {wanted}: {warning}")
    return errors


if __name__ == "__main__":
    expected = [line for line in Path(sys.argv[1]).read_text().splitlines() if line]
    errors = check(Path(sys.argv[2]).read_text(errors="replace"), expected)
    for line in errors:
        print(line)
    sys.exit(bool(errors))
