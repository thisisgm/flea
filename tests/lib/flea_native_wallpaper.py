#!/usr/bin/env python3
"""Fail closed unless every discovered wallpaper is a still image or solid color."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

def discover(proc_root=Path("/proc")):
    images = set()
    renderers = []
    renderer_pids = set()
    for proc in proc_root.glob('[0-9]*'):
        try:
            if proc.stat().st_uid != os.getuid(): continue
            name = (proc / 'comm').read_text().strip()
            if re.search(r'mpvpaper|wallpaperengine|linux-wallpaper', name):
                raise RuntimeError('video/animated wallpaper process: ' + name)
            if name == 'swaybg':
                words = (proc / 'cmdline').read_bytes().decode().strip('\0').split('\0')
                renderers.append(name)
                renderer_pids.add(int(proc.name))
                found = False
                for i, word in enumerate(words):
                    if word in ('-i', '--image') and i + 1 < len(words): images.add(words[i+1]); found = True
                    elif word.startswith('--image='): images.add(word.split('=',1)[1]); found = True
                if not found and not any(w in ('-c','--color') or w.startswith('--color=') for w in words):
                    raise RuntimeError('swaybg wallpaper source is unreadable')
            if name in ('awww-daemon','swww-daemon'):
                client = name.split('-',1)[0]
                if not shutil.which(client): raise RuntimeError('missing wallpaper query client: ' + client)
                answer = subprocess.check_output([client,'query'],text=True,timeout=5)
                values = re.findall(r'\bimage:\s*(.+)',answer)
                if not values: raise RuntimeError('wallpaper query returned no image paths: ' + client)
                images.update(value.strip() for value in values)
                renderers.append(name)
                renderer_pids.add(int(proc.name))
        except (FileNotFoundError, ProcessLookupError):
            continue
    layers = json.loads(subprocess.check_output(['hyprctl','-j','layers'],text=True,timeout=5))
    def backgrounds(node):
        if isinstance(node,dict):
            levels=node.get('levels')
            if isinstance(levels,dict):
                yield from levels.get('0',[])
            for key,value in node.items():
                if key!='levels': yield from backgrounds(value)
        elif isinstance(node,list):
            for value in node: yield from backgrounds(value)
    for layer in backgrounds(layers):
        if not isinstance(layer,dict) or int(layer.get('pid',-1)) not in renderer_pids:
            raise RuntimeError('unsupported or unidentified active background layer: '+str(layer))
    return images, renderers

def inspect(proc_root=Path("/proc")):
    images, renderers = discover(proc_root)
    if not renderers: raise RuntimeError('no supported active wallpaper renderer; inspect the session and add its exact read-only source query')
    checked=[]
    for value in sorted(images):
        path=Path(value).resolve(strict=True)
        mime=subprocess.check_output(['file','--brief','--mime-type','--',str(path)],text=True,timeout=5).strip()
        if not mime.startswith('image/'): raise RuntimeError('wallpaper is not a still image: ' + str(path) + ' (' + mime + ')')
        frames=subprocess.check_output(['magick','identify','-ping','-format','%n\n',str(path)],text=True,timeout=10).split()
        if not frames or any(int(n)!=1 for n in frames): raise RuntimeError('animated wallpaper: ' + str(path))
        checked.append(dict(path=str(path),mime=mime,frames=1))
    return dict(still=True,renderers=renderers,images=checked)

if __name__ == "__main__":
    try:
        print(json.dumps(inspect()))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print("REFUSED: still-wallpaper preflight: " + str(error), file=sys.stderr)
        sys.exit(2)
