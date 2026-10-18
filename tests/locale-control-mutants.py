"""Prove the real locale runner rejects each multi-label control with one dead check."""
import argparse
import ast
from pathlib import Path
import runpy
import shlex
import shutil
import subprocess
import sys
import tempfile
import traceback

REPO = Path(__file__).resolve().parents[1]
MUTANTS = {
    'continuations': ('shell', 'continued ffmpeg flags are parsed'),
    'exports': ('shell', 'function scope is rejected'),
    'case-scope': ('shell', 'global scope after case a)'),
    'block-boundaries': ('shell', 'pipe loop preserves later global scope'),
    'block-balance': ('shell', 'unbalanced scope raises:'),
    'physical-lines': ('shell', 'newline-only physical line with'),
    'fixture-scope': ('parsers', 'matched fixture sort child environment'),
    'charset': ('parsers', 'ps preserves UTF-8 process arguments'),
    'gio-charset': ('parsers', 'gio preserves UTF-8 display names'),
}
REGRESS_FLAGS = {'gio-charset': 'charset'}


# Sample input: check('continued ffmpeg flags are parsed', parsed(name, suffix)).
def audit(control, variant):
    kind, victim = MUTANTS[control]
    path = REPO / 'tests' / ('locale-' + kind + '.py')
    tree = ast.parse(path.read_text(), filename=str(path))
    if variant == 'mutant':
        callbacks = [node for node in ast.walk(tree)
                     if isinstance(node, ast.FunctionDef) and node.name == 'check']
        if len(callbacks) != 1:
            raise RuntimeError('expected exactly one check callback in ' + str(path))
        guard = ast.parse('if label.startswith(' + repr(victim) + '):\n'
                          '    print("MUTANT muted check: " + label, file=__import__("sys").stderr)\n'
                          '    return\n').body[0]
        callbacks[0].body.insert(0, guard)
        ast.fix_missing_locations(tree)
    sys.argv = [str(path), '--regress-' + REGRESS_FLAGS.get(control, control)]
    exec(compile(tree, str(path), 'exec'), {'__name__': '__main__', '__file__': str(path)})


# Sample input: run() { ... } followed by run 'mutant --regress-exports' 1 DETECTED... python3 ... .
def probe(control, variant, keep):
    parent = REPO / '.superpowers/i0'
    if not parent.resolve().is_relative_to(REPO):
        raise RuntimeError('REFUSED mutant fixture parent outside the repository')
    parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='locale-mutant.', dir=parent))
    marker = root / '.flea-test-sandbox'
    marker.write_text('locale control mutant fixture\n')
    source = (REPO / 'tests/locale.sh').read_text()
    start = source.index('run() {\n')
    runner = source[start:source.index('\n}\n', start) + 3]
    label = variant + ' --regress-' + control
    detection = 'DETECTED --regress-' + REGRESS_FLAGS.get(control, control)
    command = ['run', label, '1', detection, 'env', 'LC_ALL=C', 'LANG=C',
               sys.executable, '-B', str(Path(__file__).resolve()), control, variant, '--audit']
    body = ('set -uo pipefail\nbox=' + shlex.quote(str(root)) +
            '\npassed=0\nfailed=0\nserial=0\n' + runner + '\n' +
            shlex.join(command) + '\n[ "$failed" -eq 0 ]\n')
    script = root / 'runner.sh'
    script.write_text(body)
    result = subprocess.run(['/bin/bash', str(script)], capture_output=True, text=True)
    output = result.stdout + result.stderr
    (root / 'runner.log').write_text(output)
    (root / 'runner.rc').write_text(str(result.returncode) + '\n')
    child = (root / 'check-1.log').read_text()
    victim = MUTANTS[control][1]
    gio_observation = runpy.run_path(str(REPO / 'tests/locale-shell-scope.py'))['GIO_UTF8_OBSERVED']
    if variant == 'mutant':
        expected = f'FAIL locale {label}: expected exit 1, got 1'
        good = (result.returncode == 1 and expected in output.splitlines()
                and any(line.startswith('MUTANT muted check: ' + victim) for line in child.splitlines())
                and not any(line.startswith('FAIL ' + victim) for line in child.splitlines())
                and (control != 'gio-charset' or gio_observation not in child.splitlines())
                and detection not in child.splitlines())
    else:
        expected = f'ok   locale {label}: exit 1'
        good = (result.returncode == 0 and expected in output.splitlines()
                and detection in child.splitlines()
                and (any(line.startswith('FAIL ' + victim) for line in child.splitlines())
                     or (control == 'gio-charset' and gio_observation in child.splitlines())))
    print(('PASS ' if good else 'FAIL ') + label + f': runner exit {result.returncode}; victim={victim}')
    print(expected if good else output, end='\n')
    if keep or not good:
        print('evidence=' + str(root))
    else:
        if not (root.is_absolute() and str(root) and root.resolve().parent == parent.resolve()
                and not root.is_symlink() and root.name.startswith('locale-mutant.') and marker.is_file()):
            raise RuntimeError('REFUSED removal of an unmarked mutant fixture')
        shutil.rmtree(root)
    return 0 if good else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('control', nargs='?', choices=tuple(MUTANTS))
    parser.add_argument('variant', nargs='?', choices=('mutant', 'twin'))
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--audit', action='store_true')
    parser.add_argument('--keep', action='store_true')
    args = parser.parse_args()
    if args.list:
        print('\n'.join(MUTANTS))
        return 0
    if args.control is None or args.variant is None:
        parser.error('control and variant are required')
    if args.audit:
        audit(args.control, args.variant)
        raise RuntimeError('audit did not exit')
    return probe(args.control, args.variant, args.keep)


if __name__ == '__main__':
    try:
        status = main()
    except Exception:
        traceback.print_exc()
        status = 3
    sys.exit(status)
