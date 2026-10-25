"""Common requested-row work; product-rich output remains a separate clock."""
import hashlib
import json
import mimetypes
import os
import time


def materialize(tree, paths):
    start, cpu = time.perf_counter_ns(), time.process_time_ns()
    rows = []
    for rel in paths:
        path = os.path.join(os.fsencode(tree), os.fsencode(rel))
        meta = os.lstat(path)
        try:
            tag = os.getxattr(path, 'user.xdg.tags', follow_symlinks=False).hex()
        except OSError:
            tag = ''
        rows.append(dict(path=os.fsencode(rel).hex(), mode=meta.st_mode, size=meta.st_size,
                         mtime_ns=meta.st_mtime_ns, mime=mimetypes.guess_type(rel)[0], tags=tag))
    payload = json.dumps(rows, separators=(',', ':')).encode()
    elapsed = time.perf_counter_ns() - start
    decode = time.process_time_ns()
    json.loads(payload)
    return dict(us=elapsed / 1000, cpu_us=(decode - cpu) / 1000,
                decode_cpu_us=(time.process_time_ns() - decode) / 1000,
                bytes=len(payload), rows=len(rows), sha256=hashlib.sha256(payload).hexdigest())
