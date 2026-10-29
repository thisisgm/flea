"""Builds the read-only fixture bundle the native battery imports with --fixtures; see tests/tui_battery/README.md."""
import argparse
import calendar
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from ..evidence import Blocked, MARKER, absolute, output_root
from . import models, office, pdf, raster

STAMP_NS = calendar.timegm((2025, 6, 15, 10, 0, 0)) * 10 ** 9  # the boards' Modified, 2025-06-15 10:00 UTC
FILE, DIR, LOCKED, LINK, TOOL = 0o644, 0o755, 0o000, 0o777, 0o755
TOOLS = "/usr/bin:/bin"

NOTES = b"Visual gate notes\n\nThis file is the fixture's plain text sample.\nLine four keeps the preview busy.\n"
TODO = b"# Todo\n\n- [x] draw the list\n- [ ] draw the grid\n"
MAIN_RS = (b"// The visual gate's code sample.\nfn main() {\n    let names = [\"alpha\", \"beta\", \"gamma\"];\n"
           b"    for name in names {\n        println!(\"hello {name}\");\n    }\n}\n")
# Seeds the bench world's encoders made; a seed directory supplies them, otherwise the bench recipe makes them here.
SEEDS = ("photo.png", "report.pdf", "clip.mp4", "sunset.jpg", "tone.ogg", "bundle.zip")
RECIPES = {
    "photo.png": ["magick", "-seed", "7", "-size", "800x600", "plasma:fractal", "-strip", "{out}"],
    "sunset.jpg": ["magick", "-seed", "11", "-size", "640x480", "gradient:orange-purple", "-strip", "-quality", "85", "{out}"],
    "report.pdf": ["magick", "-size", "600x800", "xc:white", "-fill", "black", "-pointsize", "36", "-annotate", "+60+120",
                   "Visual gate report", "-pointsize", "20", "-annotate", "+60+200", "Page one of the fixture PDF.", "-depth", "8", "rgb:{out}"],
    "clip.mp4": ["ffmpeg", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=25", "-pix_fmt",
                 "yuv420p", "-fflags", "+bitexact", "-flags", "+bitexact", "-c:v", "libx264", "-threads", "1", "{out}"],
    "tone.ogg": ["ffmpeg", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-fflags", "+bitexact",
                 "-c:a", "libvorbis", "{out}"],
}
REPORT_SIZE, REPORT_BYTES = (600, 800), 21840  # the bench recipe's canvas and the bench seed's report.pdf size
OFFICE_SIZES = {"budget.xlsx": 7970, "garden-report.docx": 8025, "garden-report.pdf": 92286, "lease.docx": 8192, "talk.odp": 19532}
REPORT_PAGES = [
    [("Title", "Riverside Community Garden"),
     ("Subtitle", "Spring report, March to May 2025. Prepared for the garden committee, 2 June 2025."),
     ("Heading1", "Summary"),
     ("", "The garden opened all forty plots this spring, two more than last year, and every plot holder renewed."),
     ("", "Volunteers gave 412 hours across twelve work mornings. The new compost bays and the polytunnel repairs were finished in April."),
     ("", "Spending stayed within the budget the committee approved in February; the details are on page three.")],
    [("Heading1", "Plots and volunteers"),
     ("", "Forty plots are let, with eleven names on the waiting list. Three plots changed hands after the March inspection."),
     ("", "Work mornings ran on the first and third Saturday of each month. Attendance averaged fourteen people."),
     ("Heading1", "Growing"),
     ("", "Early potatoes and broad beans went in during March. The polytunnel carried tomato and chilli seedlings for the open evening.")],
    [("Heading1", "Budget and next steps"),
     ("", "Planned spending for the season was 1,125 pounds and actual spending was 1,108 pounds."),
     ("", "The committee proposes a second water butt, new signage for the main gate and a summer open day in July."),
     ("", "Sam Ellis, garden secretary")],
]
TALK = [("Composting at home", ["Open evening, Wednesday 14 May, 7 pm at the polytunnel"]),
        ("What goes in", ["Greens: peelings, grass, coffee grounds", "Browns: card, leaves, straw"]),
        ("Next steps", ["Bins are free to plot holders", "Ask at the shed on work mornings"])]
LEASE_PAGES = [[("Title", "Plot lease"), ("", "This lease lets one plot at Riverside Community Garden for the 2025 season.")]]
MINUTES = ("# 議事録\n\n日時: 2025年6月12日 19時\n場所: 集会所\n出席: 佐藤、鈴木、高橋、田中、伊藤\n\n## 議題\n\n"
           "1. 春の作業報告\n2. 予算の確認\n3. 夏のオープンデー\n4. 水やり当番\n\n## 決定事項\n\n"
           "- 雨水タンクを一台追加する。費用は三万円以内とする。\n- 夏のオープンデーは7月20日に開催する。\n"
           "- 正門に新しい案内板を設置する。\n- 水やり当番表を掲示板に貼る。\n\n## 次回\n\n"
           "- 7月10日 19時、集会所にて。\n- 担当: 佐藤(資料)、鈴木(会場)、高橋(連絡)。\n\n"
           "## メモ\n\n- 堆肥置き場の看板が古いので、次回までに案を出す。\n- 種の交換会を秋に開く案が出た。\n"
           "- 物置の鍵を一本追加で作る。\n- 新しい会員には案内の冊子を渡す。\n- 道具の貸し出し記録を続ける。\n"
           "- 苗の販売は六月末で終える。\n\n## 報告\n\n"
           "- 春の作業日は十二回、延べ四百十二時間だった。\n- 区画は四十区画すべてが埋まり、待機者は十一名。\n"
           "- ポリトンネルの修理は四月に完了した。\n- 堆肥置き場は三区画に増やした。\n- 会計報告は次回に回す。\n")
QUOTE_LINES = ["御見積書", "2025年6月10日", "リバーサイド市民農園 御中", "件名: 雨水タンク設置工事",
               "品名                数量    金額", "雨水タンク 200L      1      18,000円", "設置工事一式          1       9,500円",
               "配管部材              1       2,400円", "合計                         29,900円", "有効期限: 2025年7月10日"]
TAILSCALE = """#!/bin/sh
# Fixture tailscale: answers status --json with two online peers and one offline peer, and nothing else.
if [ "$1" = status ] && [ "$2" = --json ]; then
    cat <<'JSON'
{"BackendState":"Running","Self":{"HostName":"fixture","DNSName":"fixture.tail0000.ts.net.","Online":true},"Peer":{"nodekey:01":{"HostName":"east","DNSName":"east.tail0000.ts.net.","Online":true},"nodekey:02":{"HostName":"macbookair","DNSName":"macbookair.tail0000.ts.net.","Online":true},"nodekey:03":{"HostName":"pixel-9","DNSName":"pixel-9.tail0000.ts.net.","Online":false}}}
JSON
    exit 0
fi
echo "fixture tailscale only answers status --json" >&2
exit 1
"""
TAILDROP_SEND = """#!/bin/sh
# Fixture send helper: records the request under the private state directory and sends nothing.
printf '%s\\n' "$*" >> "${XDG_STATE_HOME:-/fx/state}/fixture-taildrop.log"
exit 0
"""
DROPBOX = """#!/bin/sh
# Fixture dropbox-cli: installed but signed out, as no account file exists in the private home.
echo "Dropbox isn't running!"
exit 1
"""
TRASHINFO = b"[Trash Info]\nPath=/fx/v/work/old-draft.txt\nDeletionDate=2025-06-14T09:30:00\n"


class Bundle:
    def __init__(self, root):
        self.root, self.store, self.provenance = root, {}, {}

    def blob(self, relative, data, mode=FILE):
        """Writes data at relative, hardlinking identical bytes already written."""
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256(data).hexdigest()
        if digest in self.store:
            os.link(self.store[digest], path)
        else:
            with path.open("xb") as output:
                output.write(data)
            path.chmod(mode)
            self.store[digest] = path
        return digest


def run(argv, scratch, timeout=180):
    tool = shutil.which(argv[0], path=TOOLS)
    if not tool:
        raise Blocked(f"{argv[0]} is required to build this fixture and is not installed")
    env = {"PATH": TOOLS, "HOME": str(scratch), "TMPDIR": str(scratch), "LANG": "C.UTF-8", "TZ": "UTC"}
    result = subprocess.run([tool] + argv[1:], cwd=scratch, env=env, stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout)
    if result.returncode:
        raise Blocked(f"{argv[0]} exited {result.returncode}: {result.stderr.decode(errors='replace')[:300]}")
    return result


def seed_bytes(name, seed, scratch, provenance):
    if seed and (seed / "Media" / name).is_file():
        data = (seed / "Media" / name).read_bytes()
        provenance[name] = "bench fixture seed Media/" + name
        return data
    out = scratch / ("recipe-" + name)
    if name == "bundle.zip":
        data = office.zip_exact([("main.rs", MAIN_RS, True)], 232)
        provenance[name] = "Python zipfile of Code/main.rs, padded to the bench archive's 232 bytes"
        return data
    run([part.replace("{out}", str(out)) for part in RECIPES[name]], scratch)
    data = out.read_bytes()
    recipe = " ".join(RECIPES[name][:1] + [a for a in RECIPES[name][1:] if "{out}" not in a])
    if name == "report.pdf":
        # ImageMagick's PDF writer stamps dates, XMP and an /ID from the clock, so magick draws raw pixels and pdf.py wraps them.
        if len(data) != REPORT_SIZE[0] * REPORT_SIZE[1] * 3:
            raise Blocked(f"magick wrote {len(data)} bytes of report.pdf pixels, not {REPORT_SIZE[0]}x{REPORT_SIZE[1]} RGB")
        provenance[name] = (f"bench recipe {recipe} as raw RGB, wrapped as one image page by the bundle's pdf.py "
                            f"with fixed Info dates and no /ID, padded to the bench file's {REPORT_BYTES} bytes")
        return pdf.image(*REPORT_SIZE, data, REPORT_BYTES)
    provenance[name] = "bench recipe: " + recipe
    return data


def work_files(seed_media, docs):
    """The v0.3.12 work tree as {path: (kind, bytes or target, mode)}; docs is 'v0312' or 'office'."""
    tree = {}

    def d(path, mode=DIR):
        tree[path] = ("directory", None, mode)

    def f(path, data, mode=FILE):
        tree[path] = ("file", data, mode)

    for name in ("A", "A/empty", "A/full", "Code", "Docs", "Empty", "Many", "Media"):
        d(name)
    d("Locked", LOCKED)
    for name in ("alpha.txt", "beta.md", "gamma.txt"):
        f("A/" + name, f"top file {name}\n".encode())
    f("A/.hidden-note", b"hidden\n")
    for i in range(1, 6):
        f(f"A/full/f{i}.txt", f"full file {i}\n".encode())
    f("Code/main.rs", MAIN_RS)
    f("Locked/secret.txt", b"locked away\n")
    for i in range(1, 81):
        f(f"Many/item-{i:02d}.txt", f"row {i:02d}\n".encode())
    f("notes.txt", NOTES)
    f("todo.md", TODO)
    f("Docs/notes.txt", NOTES)
    f("Docs/readme.md", TODO)
    f("Docs/report.pdf", seed_media["report.pdf"])
    for name in SEEDS:
        f("Media/" + name, seed_media[name])
    f("Media/main.rs", MAIN_RS)
    f("Media/blob.dat", b"\x00\x01\x02flea visual gate blob\x03")
    f("Media/broken.pdf", b"%PDF-1.4\nthis is not pdf body\n")
    f("Media/letter.odt", b"PK not really an office document\n")
    tree["Media/link.png"] = ("symlink", "photo.png", LINK)
    if docs == "office":
        for name, data in office_documents().items():
            f("Docs/" + name, data)
    return tree


_OFFICE = {}


def office_documents():
    """Builds the five office documents once per process; the cache takes only a whole set at the reference sizes."""
    if not _OFFICE:
        made = {}
        made["garden-report.docx"] = office.docx(REPORT_PAGES, "Spring report 2025", "Sam Ellis", OFFICE_SIZES["garden-report.docx"])
        pages = []
        for page in REPORT_PAGES:
            y, rows = 770, []
            for style, text in page:
                points = {"Title": 26, "Subtitle": 13, "Heading1": 16}.get(style, 11)
                for line in wrap(text, 470 / (points * 0.5)):
                    rows.append((points, 72, y, line))
                    y -= points * 1.6
                y -= 6
            pages.append(rows)
        made["garden-report.pdf"] = pdf.document(pages, "Spring report 2025", "Sam Ellis", OFFICE_SIZES["garden-report.pdf"])
        made["budget.xlsx"] = office.xlsx(OFFICE_SIZES["budget.xlsx"])
        thumb = raster.gradient_png(256, 144, [(52, 101, 56), (234, 222, 186)])
        made["talk.odp"] = office.odp(TALK, "Composting at home", thumb, OFFICE_SIZES["talk.odp"])
        package = office.docx(LEASE_PAGES, "Plot lease", "Sam Ellis", 5590)
        made["lease.docx"] = office.encrypted(package)
        for name, size in OFFICE_SIZES.items():
            if len(made[name]) != size:
                raise Blocked(f"{name} is {len(made[name])} bytes, the reference is {size}")
        _OFFICE.update(made)
    return dict(_OFFICE)


def wrap(text, width):
    words, lines, line = text.split(), [], ""
    for word in words:
        if line and len(line) + 1 + len(word) > width:
            lines.append(line)
            line = word
        else:
            line = (line + " " + word).strip()
    return lines + [line] if line else lines


def prints():
    return {"Prints": ("directory", None, DIR),
            "Prints/bracket.stl": ("file", models.stl(models.bracket(), "bracket.stl"), FILE),
            "Prints/knob.obj": ("file", models.obj(models.knob(), "knob.obj", 96400), FILE),
            "Prints/benchy.3mf": ("file", models.benchy_3mf(1900000), FILE),
            "Prints/benchy.gcode": ("file", models.benchy_gcode(3560000, 3640000), FILE),
            "Prints/shelf-kit.3mf": ("file", models.shelf_kit_3mf(1200000), FILE)}


def japanese():
    minutes = MINUTES.encode()
    if not 1150 <= len(minutes) < 1250:
        raise Blocked(f"議事録.md is {len(minutes)} bytes, the reference shows 1.2 kB")
    page = [(22 if i == 0 else 12, 72, 770 - i * 34, line) for i, line in enumerate(QUOTE_LINES)]
    return {"写真": ("directory", None, DIR),
            "写真/庭.png": ("file", raster.gradient_png(1024, 1009, [(76, 120, 60), (180, 200, 120), (90, 140, 200)], level=0), FILE),
            "見積書_2025.pdf": ("file", pdf.document([page], "御見積書", "高橋工務店", 86400, cjk=True), FILE),
            "議事録.md": ("file", minutes, FILE)}


def representations(bundle, media, office_docs, scratch, without):
    """Independent source rasters for the eight half-block media states, as auxiliary data files per family."""
    pages = "data/reference-pages/"
    made, omitted = {}, []
    kit = models.kit_reference(1378, 924, az=raster.ISO_AZ + 30)
    made["shelf-kit-plate1.png"] = (kit, "Plate 1 of shelf-kit.3mf on the P1S plate, flat shaded by the bundle's own rasterizer, "
                                         "isometric pitch, azimuth turned 30 degrees, from the same geometry the 3MF stores")
    if without:
        omitted = ["report-1.png", "clip-1.png", "sunset.png", "garden-report-docx-1.png", "garden-report-pdf-1.png"]
    else:
        (scratch / "in").mkdir()
        for name, data in (("report.pdf", media["report.pdf"]), ("clip.mp4", media["clip.mp4"]), ("sunset.jpg", media["sunset.jpg"]),
                           ("garden-report.docx", office_docs["garden-report.docx"]), ("garden-report.pdf", office_docs["garden-report.pdf"])):
            (scratch / "in" / name).write_bytes(data)
        run(["pdftoppm", "-png", "-r", "72", "-f", "1", "-l", "1", "-singlefile", "in/report.pdf", "report"], scratch)
        made["report-1.png"] = ((scratch / "report.png").read_bytes(), "pdftoppm -png -r 72 -f 1 -l 1 of report.pdf, its only page")
        run(["ffmpeg", "-loglevel", "error", "-i", "in/clip.mp4", "-frames:v", "1", "-pix_fmt", "rgb24", "-fflags", "+bitexact",
             "clip.png"], scratch)
        made["clip-1.png"] = ((scratch / "clip.png").read_bytes(), "ffmpeg first decoded frame of clip.mp4 as rgb24 PNG")
        run(["ffmpeg", "-loglevel", "error", "-i", "in/sunset.jpg", "-pix_fmt", "rgb24", "-fflags", "+bitexact", "sunset.png"], scratch)
        made["sunset.png"] = ((scratch / "sunset.png").read_bytes(), "ffmpeg decode of sunset.jpg as rgb24 PNG")
        profile = scratch / "profile"
        run(["soffice", f"-env:UserInstallation=file://{profile}", "--headless", "--convert-to", "pdf", "--outdir", "export",
             "in/garden-report.docx"], scratch, timeout=300)
        run(["pdftoppm", "-png", "-r", "50", "-f", "1", "-l", "1", "-singlefile", "export/garden-report.pdf", "docx"], scratch)
        made["garden-report-docx-1.png"] = ((scratch / "docx.png").read_bytes(), "LibreOffice headless PDF export of garden-report.docx, "
                                            "first page rasterized by pdftoppm -png -r 50")
        run(["pdftoppm", "-png", "-r", "50", "-f", "1", "-l", "1", "-singlefile", "in/garden-report.pdf", "gpdf"], scratch)
        made["garden-report-pdf-1.png"] = ((scratch / "gpdf.png").read_bytes(), "pdftoppm -png -r 50 -f 1 -l 1 of garden-report.pdf")
    rows = {}
    for name, (data, derivation) in made.items():
        sha = bundle.blob("aux/" + pages + name, data)
        rows[name] = ({"path": pages + name, "source": "aux/" + pages + name, "size": len(data), "sha256": sha, "mode": FILE}, derivation)

    def source(raster_name, inputs):
        if raster_name is None:
            return {"input": inputs, "raster": inputs, "derivation": "The PNG fixture is its own raster"}
        return {"input": inputs, "raster": pages + raster_name, "derivation": rows[raster_name][1]}

    plan = {
        "work": {"tui-list-pdf": [("report-1.png", "v/work/Media/report.pdf")], "tui-list-video": [("clip-1.png", "v/work/Media/clip.mp4")],
                 "tui-quicklook-image": [(None, "v/work/Media/photo.png")]},
        "office": {"tui-office": [("garden-report-docx-1.png", "v/work/Docs/garden-report.docx")],
                   "tui-print-sent": [("garden-report-pdf-1.png", "v/work/Docs/garden-report.pdf")],
                   "tui-combine": [(None, "v/work/Media/photo.png"), ("sunset.png", "v/work/Media/sunset.jpg")]},
        "prints": {"tui-3d-quicklook": [("shelf-kit-plate1.png", "v/work/Prints/shelf-kit.3mf")]},
        "ar": {"tui-lang-ar": [("report-1.png", "v/work/تقرير 2025.pdf")]},
    }
    result = {}
    for family, states in plan.items():
        auxiliary, reps = [], {}
        for state, sources in states.items():
            if any(r is not None and r not in rows for r, _ in sources):
                continue
            reps[state] = {"sources": [source(r, i) for r, i in sources]}
            for r, _ in sources:
                if r is not None and rows[r][0] not in auxiliary:
                    auxiliary.append(rows[r][0])
        result[family] = (auxiliary, reps)
    return result, omitted


def provider_auxiliary(bundle):
    out = []
    for name, text in (("tailscale", TAILSCALE), ("omarchy-tailscale-send", TAILDROP_SEND), ("dropbox-cli", DROPBOX)):
        data = text.encode()
        sha = bundle.blob("aux/tools/" + name, data, TOOL)
        out.append({"path": "tools/" + name, "source": "aux/tools/" + name, "size": len(data), "sha256": sha, "mode": TOOL})
    return out


def trash_auxiliary(bundle):
    out = []
    for path, data in (("data/Trash/files/old-draft.txt", b"an old draft\n"), ("data/Trash/info/old-draft.txt.trashinfo", TRASHINFO)):
        sha = bundle.blob("aux/" + path, data)
        out.append({"path": path, "source": "aux/" + path, "size": len(data), "sha256": sha, "mode": FILE})
    return out


def write_family(bundle, name, tree):
    root = Path("families") / name
    entries = []
    for path in sorted(tree, key=lambda p: (len(Path(p).parts), p)):
        kind, data, mode = tree[path]
        target = bundle.root / root / path
        row = {"path": path, "kind": kind, "mode": mode, "mtime_ns": STAMP_NS}
        if kind == "directory":
            target.mkdir(parents=True, exist_ok=True)
        elif kind == "symlink":
            target.parent.mkdir(parents=True, exist_ok=True)
            target.symlink_to(data)
            row["target"] = data
        else:
            row.update(size=len(data), sha256=bundle.blob(root / path, data))
        entries.append(row)
    return {"root": str(root), "entries": sorted(entries, key=lambda r: r["path"])}


def freeze(root):
    """Stamps the boards' mtime and makes the bundle read-only; directory modes for Locked apply only to imported copies."""
    for path in sorted(root.rglob("*"), key=lambda p: -len(p.parts)):
        if path.is_symlink():
            os.utime(path, ns=(STAMP_NS, STAMP_NS), follow_symlinks=False)
            continue
        path.chmod(0o555 if path.is_dir() else (0o555 if path.stat().st_mode & 0o100 else 0o444))
        os.utime(path, ns=(STAMP_NS, STAMP_NS))


def identity(root):
    """One digest over every bundle path, kind, mode, size and content, excluding the run's own sandbox marker."""
    h = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        relative = str(path.relative_to(root))
        if relative == MARKER:
            continue
        info = path.lstat()
        if path.is_symlink():
            h.update(f"{relative}\0l\0{os.readlink(path)}\n".encode())
        elif path.is_dir():
            h.update(f"{relative}\0d\0{info.st_mode & 0o777:o}\n".encode())
        else:
            h.update(f"{relative}\0f\0{info.st_mode & 0o777:o}\0{info.st_size}\0{hashlib.sha256(path.read_bytes()).hexdigest()}\n".encode())
    return h.hexdigest()


def build(output, seed=None, without_rasters=False):
    owner = output_root(output)
    root = owner.path
    bundle = Bundle(root)
    scratch = Path(tempfile.mkdtemp(prefix=".scratch.", dir=root))
    try:
        media = {name: seed_bytes(name, seed, scratch, bundle.provenance) for name in SEEDS}
        docs = office_documents()
        families = {}
        base = work_files(media, "v0312")
        office_tree = work_files(media, "office")
        trees = {"work": base, "office": office_tree, "prints": {**base, **prints()}, "ja": {**office_tree, **japanese()},
                 "ar": {**office_tree, "تقرير 2025.pdf": ("file", media["report.pdf"], FILE)}, "trash": base}
        providers = provider_auxiliary(bundle)
        reps, omitted = representations(bundle, media, docs, scratch, without_rasters)
        for name, tree in trees.items():
            spec = write_family(bundle, name, tree)
            auxiliary, representation = reps.get(name, ([], {}))
            spec["features"] = ["provider-fixtures"]
            spec["auxiliary"] = providers + auxiliary + (trash_auxiliary(bundle) if name == "trash" else [])
            if representation:
                spec["representations"] = representation
            families[name] = spec
    finally:
        if scratch.is_absolute() and scratch.parent == root and scratch.name.startswith(".scratch.") and (root / MARKER).is_file():
            for path in scratch.rglob("*"):
                if path.is_dir() and not path.is_symlink():
                    path.chmod(0o700)
            shutil.rmtree(scratch)
    provenance = dict(sorted(bundle.provenance.items()))
    provenance.update({"office documents": "Python OOXML and ODF writers with fixed timestamps; a private zip extra field (0x6C66), "
                                           "with at most 7 archive comment bytes, pads garden-report.docx, budget.xlsx and talk.odp to "
                                           "their reference sizes and lease.docx's inner package to 5590 bytes; lease.docx is that "
                                           "package in ECMA-376 Standard Encryption, AES-128, password fixture, as a 15-sector "
                                           "compound file of exactly 8192 bytes; garden-report.pdf is under PDF documents",
                       "PDF documents": "Python PDF writer with fixed Info dates; a comment block after the header pads each to size",
                       "3D models": "Python meshes from the fixed bracket, knob and boat geometry; 3MF thumbnails by the bundle rasterizer; "
                                    "3MF objects carry white/orange base materials and explicit 256 by 256 mm P1S printable_area; "
                                    "G-code carries an explicit 250 by 210 mm MK4 bed_shape. Shelf kit plate membership is "
                                    "[bracket, bracket, orange knob] then [orange knob]; the Quick Look independent raster "
                                    "uses those same translated meshes/materials/whole bed at isometric azimuth plus 30 degrees.",
                       "text and small files": "the bench recipe's exact bytes, written by Python"})
    manifest = {"schema": 1, "generator": "tools/flea-tui-fixture-bundle", "mtime": "2025-06-15 10:00 UTC",
                "families": families, "provenance": provenance, "representations_omitted": omitted}
    with (root / "manifest.json").open("x", encoding="utf-8") as output_file:
        json.dump(manifest, output_file, ensure_ascii=False, indent=1, sort_keys=True)
        output_file.write("\n")
    freeze(root)
    return {"output": str(root), "bundle_sha256": identity(root),
            "manifest_sha256": hashlib.sha256((root / "manifest.json").read_bytes()).hexdigest(),
            "families": {name: len(spec["entries"]) for name, spec in families.items()},
            "auxiliary": {name: len(spec["auxiliary"]) for name, spec in families.items()},
            "representations_omitted": omitted}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="a new directory beneath a marked mktemp root")
    parser.add_argument("--seed", type=Path, help="the bench fixture's work directory, whose Media seeds are copied byte for byte")
    parser.add_argument("--without-rasters", action="store_true",
                        help="skip the reference rasters that need pdftoppm, ffmpeg and soffice; those states then stay BLOCKED")
    args = parser.parse_args(argv)
    try:
        seed = absolute(args.seed) if args.seed else None
        if seed and not seed.is_dir():
            raise Blocked(f"seed is not a directory: {seed}")
        print(json.dumps(build(args.output, seed, args.without_rasters), ensure_ascii=False))
        return 0
    except (Blocked, OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(json.dumps({"status": "BLOCKED", "reason": str(error)}, ensure_ascii=False), file=sys.stderr)
        return 2
