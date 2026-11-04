"""BENCH-07 request and collector use the same validated same-host schema."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / 'fair'))
import clocks


def cache_gate(output, mode, context):
    return clocks.cache_gate(output, mode, context)
