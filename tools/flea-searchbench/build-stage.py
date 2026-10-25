"""Patch only a fresh marked archive of the exact candidate, then build it."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import io

HERE = Path(__file__).resolve().parent
COMMIT = '0ebbcffae14609a3663c90db6d5f73c5e86c97a4'


def extract_source(data, destination):
    with tarfile.open(fileobj=io.BytesIO(data)) as stream:
        members = [member for member in stream.getmembers() if member.isfile() or member.isdir()]
        stream.extractall(destination, members=members, filter='data')


def build(repo, root):
    if not root.is_absolute() or root != root.resolve() or not (root / '.findbench-root').is_file():
        raise ValueError('absolute marked build root required')
    source = root / 'candidate'
    source.mkdir()
    archive = subprocess.run(['git', '-C', str(repo), 'archive', COMMIT], check=True, capture_output=True).stdout
    extract_source(archive, source)
    patch = HERE / 'engine/flea-metrics.patch'
    subprocess.run(['patch', '-p1', '--directory', str(source), '-i', str(patch)], check=True)
    (source / 'src/backend/stagebench.rs').write_bytes((HERE / 'stage.rs').read_bytes())
    mod = source / 'src/backend/mod.rs'
    mod.write_text(mod.read_text() + '\npub(crate) mod stagebench;\n')
    main = source / 'src/main.rs'
    main.write_text(main.read_text().replace('fn main() {', '''fn main() {
    let bench_args: Vec<String> = std::env::args().collect();
    if bench_args.get(1).map(String::as_str) == Some("--search-stage") {
        if let Err(error) = backend::stagebench::main(&bench_args) { eprintln!("{error}"); std::process::exit(2); }
        return;
    }
''', 1))
    walk = source / 'src/backend/searchwalk.rs'
    walk.write_text(walk.read_text().replace('#[cfg(test)]\npub(crate) fn walk(', 'pub(crate) fn walk(', 1))
    manifest = source / 'Cargo.toml'
    manifest.write_text('''[package]
name = "flea"
version = "0.4.0"
edition = "2021"
[workspace]
[dependencies]
[features]
flash-test-seam = []
[profile.release]
lto = true
codegen-units = 1
strip = true
''')
    lock = source / 'Cargo.lock'
    lock.write_text('version = 4\n[[package]]\nname = "flea"\nversion = "0.4.0"\n')
    target = root / 'candidate-target'
    subprocess.run(['cargo', 'build', '--locked', '--release', '--manifest-path', str(manifest), '--target-dir', str(target)], check=True)
    binary = target / 'release/flea'
    identity = dict(commit=COMMIT, binary=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                    metrics_patch_sha256=hashlib.sha256(patch.read_bytes()).hexdigest(),
                    stage_sha256=hashlib.sha256((HERE / 'stage.rs').read_bytes()).hexdigest())
    (root / 'candidate.json').write_text(json.dumps(identity, indent=2) + '\n')
    return identity


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('root', type=Path)
    parser.add_argument('--repo', type=Path, default=HERE.parents[1])
    args = parser.parse_args()
    print(json.dumps(build(args.repo, args.root)))
