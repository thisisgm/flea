"""Delegate camera RAW validation to the pinned corpus owner's verifier."""

from pathlib import Path
import subprocess
import tempfile

from inventory import digest


def verify(repo, evidence_root, corpus_root):
    tool = repo / "tools/flea-raw-corpus"
    manifest = repo / "tools/flea-raw-corpus.tsv"
    if not tool.is_file() or not manifest.is_file():
        raise ValueError("stage official tools/flea-raw-corpus and tools/flea-raw-corpus.tsv first")
    corpus_root = Path(corpus_root)
    if not corpus_root.is_absolute() or corpus_root.resolve() != corpus_root or not corpus_root.is_dir():
        raise ValueError("RAW corpus root must be an existing canonical absolute directory")
    proof = Path(tempfile.mkdtemp(prefix="raw-verification-", dir=evidence_root))
    (proof / ".flea-test-sandbox").touch()
    output = dict(corpus_root=str(corpus_root), verifier_sha256=digest(tool), corpus_manifest_sha256=digest(manifest), sets={})
    for name in ("twelve", "hundred"):
        command = [str(tool), "verify", str(corpus_root), "--set", name]
        with (proof / f"{name}.stdout").open("xb") as stdout, (proof / f"{name}.stderr").open("xb") as stderr:
            result = subprocess.run(command, cwd=repo, stdout=stdout, stderr=stderr, timeout=300)
        if result.returncode:
            raise ValueError(f"RAW {name} verification failed, exit {result.returncode}; see {proof}")
        output["sets"][name] = dict(directory=str(corpus_root / name), argv=command, rc=0,
                                   stdout=str(proof / f"{name}.stdout"), stderr=str(proof / f"{name}.stderr"))
    return output
