#!/usr/bin/python3
import json
import pathlib
import struct
import sys
import threading
import zlib
root = pathlib.Path(__file__).resolve().parent
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
lock = threading.Lock()
listing = 0
current = str(root)
rows = [dict(n=n, d=d, s=600, m=1, p=16877 if d else 33188, i=i) for n, d, i in [
    ('a.pdf', False, 'application-pdf'), ('b.mp4', False, 'video-x-generic'),
    ('c.3mf', False, 'model'), ('d.txt', False, 'text-plain'),
    ('e.png', False, 'image-png'), ('folder', True, 'folder')]]

def u32(n):
    return struct.pack('<I', n)

def text(s):
    raw = s.encode()
    return u32(len(raw)) + raw

def cached_model():
    summary = json.dumps(dict(format='stl', facts=dict(triangles=1)))
    data = b'FL3D\x01\0\0\0' + text(summary) + u32(0) + text('stl') + u32(0)
    data += u32(1) + text('plate') + u32(0) + u32(0)
    data += u32(1) + text('triangle') + u32(0) + u32(0) + u32(1)
    data += struct.pack('<12d', 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0)
    data += u32(3)
    for p in [(0, 0, 0), (20, 0, 0), (0, 20, 10)]:
        data += struct.pack('<3f', *p) + bytes(5)
    data += u32(1) + struct.pack('<3I', 0, 1, 2) + bytes(15) + u32(0xffffffff)
    return data + u32(0) + u32(0)

def png():
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>2I5B', 2, 2, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress((b'\0' + bytes([120, 60, 30]) * 2) * 2)) + chunk(b'IEND', b'')

def send(value):
    with lock:
        print(json.dumps(value), flush=True)

def delayed(value, seconds):
    timer = threading.Timer(seconds, send, [value])
    timer.daemon = True
    timer.start()

if '--backend' not in sys.argv:
    print('{}')
    sys.exit(0)
(root / 'cached.fl3d').write_bytes(cached_model())
(root / 'poster.png').write_bytes(png())
for line in sys.stdin:
    request = json.loads(line)
    command = request.get('c')
    with (root / 'requests.jsonl').open('a') as log:
        log.write(line)
    if command == 'list':
        listing += 1
        current = request['path']
        send(dict(t='listed', path=current, n=len(rows)))
        send(dict(t='rows', start=0, listing=listing, rows=rows))
    elif command == 'peek':
        child = request['path'].endswith('/folder')
        value = dict(t='peeked', path=request['path'], n=int(child), rows=[dict(n='seed.txt', d=False)] if child else [], first=request.get('first', 0), hidden=request.get('hidden', False), hiddenLast=request.get('hiddenLast', False), failed=False, mode=0)
        if child:
            delayed(value, 0.2)
        else:
            send(value)
    elif command == 'fsinfo':
        send(dict(t='fsinfo', path=current, fs='tmpfs', dev=1, total=100, free=90, **{'class': 'local'}))
    elif command == 'meta':
        value = dict(t='meta', row=request['row'], token=request['token'], w=2, h=2, lines=2, ms=2000)
        if request['row'] == 1:
            delayed(value, 0.1)
        else:
            send(value)
    elif command == 'code':
        send(dict(t='code', token=request['token'], slot=request['slot'], state='plain', lines=[[0, 'first actual line'], [0, 'second actual line']], total=2))
    elif command == 'thumb':
        # The backend echoes a tagged request's identity, as src/backend/thumbidentity.rs does.
        identity = {key: request[key] for key in ('thumbToken', 'thumbPath', 'thumbListing') if key in request}
        for row in request['rows']:
            send(dict(t='thumbed', row=row, file=str(root / 'poster.png'), **identity))
    elif command == 'model3d' and not request.get('cancel'):
        send(dict(t='model3d', slot=request['slot'], token=request['token'], state='ready', file=str(root / 'cached.fl3d'), thumbnail='', data=dict(format='stl', facts=dict(triangles=1, dimensions_mm=[20,20,10]), metadata={}, plates=[])))
    elif command == 'quit':
        break
