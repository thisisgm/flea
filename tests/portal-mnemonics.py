#!/usr/bin/env python3
"""GTK mnemonic markers are not printed in the native chooser's accept label."""
import ast
from pathlib import Path
import re
import sys
path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / 'tools/flea-portal'
tree = ast.parse(path.read_text())
function = next((node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'mnemonic_free'), None)
assert function is not None, 'accept labels still expose GTK mnemonic markers'
namespace = {'re': re}
exec(compile(ast.Module(body=[function], type_ignores=[]), str(path), 'exec'), namespace)
for before, after in (('_Open', 'Open'), ('_OK', 'OK'), ('Save _As', 'Save As'), ('Under__score', 'Under_score'), ('雪_を開く', '雪を開く'), ('', ''), ('literal_', 'literal_')):
    assert namespace['mnemonic_free'](before) == after, before
print('portal mnemonic labels passed')
