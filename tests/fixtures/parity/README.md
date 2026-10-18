The frozen corpus contains 533 QML and JavaScript test sources from v0.3.12, commit `f21789e56cc4b0877caa3156fff7bce52a8dd9c2`. `frozen-sources.tsv` pins every source's SHA256. The deterministic `frozen-sources.zip` occupies 1,837,510 bytes in the source tree; its uncompressed sources occupy 6,418,188 bytes.

`corpus.py` verifies every archive member against the manifest. In a checkout that holds the pinned commit it also reads and verifies the pinned Git objects, using those bytes for the test oracle. A source export, or a checkout without that commit such as a `git clone --depth 1`, uses the committed copy. Live test sources may change independently. Both paths retain the independent lexical audit of all frozen failure sites.

To regenerate the archive from the pinned objects, run this from the repository root:

```python
import csv
from pathlib import Path
import subprocess
import zipfile

fixture = Path('tests/fixtures/parity')
revision = 'f21789e56cc4b0877caa3156fff7bce52a8dd9c2'
with zipfile.ZipFile(fixture / 'frozen-sources.zip', 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for row in csv.DictReader((fixture / 'frozen-sources.tsv').open(), delimiter='\t'):
        data = subprocess.check_output(['git', 'show', f'{revision}:{row["path"]}'])
        item = zipfile.ZipInfo(row['path'])
        item.compress_type = zipfile.ZIP_DEFLATED
        item.external_attr = 0o100644 << 16
        archive.writestr(item, data, compresslevel=9)
```

Case IDs bind normalized expression tokens and enclosing function declarations. Repeated expressions also bind their peer content and surrounding non-assertion tokens. Inserting an identical peer changes those identities, so previous matrices and status overlays refuse stale proof. Scope IDs bind the complete source row; identical scope rows are rejected.

`case-id-migration.tsv` records the transition from the earlier label-and-counter IDs for all 64 preserved and 22 restored fixture sites. Each mapping retains the source location, column, sink forms and exact source hash. The suite verifies the migrated snapshots against that record.
