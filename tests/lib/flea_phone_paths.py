"""Map the phone double's returned paths into its private canonical GVFS mount."""
from pathlib import Path
import sys

script = Path(sys.argv[1])
before_mtp, after_mtp, before_afc, after_afc = sys.argv[2:]
fixture = script.parent.parent / 'gvfs'
if not script.is_absolute() or script.is_symlink() or not (fixture / '.flea-test-sandbox').is_file():
    raise SystemExit('phone double needs an absolute marked fixture')
source = script.read_text()
for before, after in ((before_mtp, after_mtp), (before_afc, after_afc)):
    if not source.count(before) or not after.startswith('/run/user/') or '/gvfs/' not in after:
        raise SystemExit('phone double path mapping is incomplete')
    source = source.replace(before, after)
script.write_text(source)
