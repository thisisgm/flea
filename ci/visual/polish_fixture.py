"""Create the visual lane's private Permissions and archive refusal fixtures."""
import os
from pathlib import Path
import sys
import zipfile

root = Path(sys.argv[1])
if not root.is_absolute() or root.name != "FxPolish":
    raise SystemExit("Expected the visual lane's absolute FxPolish fixture path")
root.mkdir(parents=True, exist_ok=True)
(root / ".flea-test-sandbox").write_text("FX-POLISH\n")
special = root / "special.txt"
special.write_text("special\n")
os.chmod(special, 0o4644)
with zipfile.ZipFile(root / "unsafe.zip", "w") as archive:
    entry = zipfile.ZipInfo("../outside.txt", date_time=(2025, 6, 15, 10, 0, 0))
    archive.writestr(entry, "An extractor must refuse this entry.\n")
for path in (special, root / "unsafe.zip", root):
    os.utime(path, (1749981600, 1749981600))
