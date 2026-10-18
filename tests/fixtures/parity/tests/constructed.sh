#!/bin/bash
python3 - "$PWD" <<'PY'
import glob, os, sys
for path in glob.glob(os.path.join(sys.argv[1], 'ui', '*.qml')):
    print(open(path).read())
PY
