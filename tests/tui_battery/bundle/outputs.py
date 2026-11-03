"""Regular artifact I/O and real conversion/private-CUPS side-effect receipts."""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import stat
import io

from ..evidence import Blocked, Owned, absolute

@contextmanager
def regular(path):
    descriptor = os.open(absolute(path), os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise Blocked("candidate artifact is not a regular file: " + str(path))
        yield source


def digest(path):
    with regular(path) as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def load(path):
    from ..fixtures import regular_bytes
    return json.loads(regular_bytes(absolute(path)))


def decode_output(case, path, kind, commands):
    """Run the actual independent decoder in the existing private fixture namespace before sealing its receipt."""
    from ..fixtures import namespace
    case, path = Owned(case).path, absolute(path)
    fx = case / "fx"
    if not path.is_relative_to(fx):
        raise Blocked("decoder input escaped the private fixture")
    tool = Path("/usr/bin/pdftoppm" if kind == "PDF" else "/usr/bin/magick")
    if not tool.is_file() or not os.access(tool, os.X_OK):
        raise Blocked("independent output decoder is unavailable")
    inside = "/fx/" + str(path.relative_to(fx))
    index = len(commands.records)
    pages = None
    if kind == "PDF":
        directory = fx / "runtime/decoded" / str(index)
        directory.mkdir(parents=True, exist_ok=False)
        prefix = "/fx/" + str(directory.relative_to(fx)) + "/page"
        arguments = ["-r", "12", "-scale-to", "64", "-png", "-progress", inside, prefix]
    else:
        arguments = ["-regard-warnings", inside, "-format", "%m\n", "info:"]
    commands.run(namespace(tool, fx, "/fx", {}, arguments=arguments), timeout=60)
    if kind == "PDF":
        pages = {str(p.relative_to(case)): digest(p) for p in sorted(directory.glob("page-*.png"))}
        if not pages:
            raise Blocked("PDF decoder produced no rendered pages")
    return {"command": index, "sha256": digest(path), "decoder_sha256": digest(tool), "pages": pages}


def output_proof(case, record):
    """Require retained source and output bytes, independently checking output format signatures."""
    sources = record.get("sources", [])
    outputs = record.get("outputs", [])
    if not sources or not outputs:
        raise Blocked("real conversion inputs and outputs are required")
    fixture = load(case / "fixtures.json")
    originals = {"fx/v/work/" + entry["path"]: entry["sha256"] for entry in fixture.get("copied", [])}
    paths = []
    for row in sources + outputs:
        relative = Path(row["path"])
        path = absolute(case / relative)
        if relative.is_absolute() or not path.is_relative_to(case / "fx") or digest(path) != row["sha256"]:
            raise Blocked("converted source/output escaped the private fixture or changed")
        paths.append(path)
    if any(originals.get(row["path"]) != row["sha256"] for row in sources):
        raise Blocked("conversion source differs from its original fixture bytes")
    if set(paths[:len(sources)]) & set(paths[len(sources):]):
        raise Blocked("conversion overwrote its source")
    if len(set(paths)) != len(paths):
        raise Blocked("conversion receipt duplicates an input or output path")
    for row, path in zip(outputs, paths[len(sources):]):
        with regular(path) as source:
            head = source.read(64)
        formats = {"PNG": head.startswith(b"\x89PNG\r\n\x1a\n"), "JPEG": head.startswith(b"\xff\xd8\xff"),
                   "WebP": head[:4] == b"RIFF" and head[8:12] == b"WEBP", "PDF": head.startswith(b"%PDF-"),
                   "TIFF": head[:4] in (b"II*\0", b"MM\0*"), "BMP": head.startswith(b"BM"),
                   "AVIF": head[4:8] == b"ftyp" and b"avif" in head[8:],
                   "HEIC": head[4:8] == b"ftyp" and any(brand in head[8:] for brand in (b"heic", b"heix", b"hevc", b"hevx"))}
        if formats.get(row.get("format")) is not True:
            raise Blocked("converted output is empty or has the wrong actual format")
        decode = row.get("decode", {})
        commands = load(case / "commands.json")
        index = decode.get("command")
        command = commands[index] if type(index) is int and 0 <= index < len(commands) else {}
        argv = command.get("argv", [])
        wanted = "/" + str(path.relative_to(case))
        tool = "/usr/bin/pdftoppm" if row["format"] == "PDF" else "/usr/bin/magick"
        mount = ["--ro-bind", tool, "/opt/flea"]
        at = max((i for i, word in enumerate(argv) if word == "--chdir"), default=-1)
        arguments = argv[at + 2:] if at >= 0 else []
        if (not argv or Path(argv[0]).name != "bwrap" or "--unshare-all" not in argv or wanted not in argv
                or type(command.get("exit_code")) is not int or command["exit_code"] != 0 or type(command.get("end_ns")) is not int
                or command.get("start_ns", command["end_ns"]) >= command["end_ns"]
                or decode.get("sha256") != row["sha256"] or decode.get("decoder_sha256") != digest(Path(tool))
                or not any(argv[i:i + 3] == mount for i in range(len(argv) - 2))
                or row["format"] != "PDF" and command.get("stderr", "").strip()):
            raise Blocked("converted output lacks an actual sandboxed decoder receipt")
        if row["format"] == "PDF":
            prefix = "/fx/runtime/decoded/" + str(index) + "/page"
            if arguments != ["/opt/flea", "-r", "12", "-scale-to", "64", "-png", "-progress", wanted, prefix] or not decode.get("pages"):
                raise Blocked("converted PDF lacks independently rendered pages")
            directory = case / "fx/runtime/decoded" / str(index)
            if {str(page.relative_to(case)) for page in directory.iterdir()} != set(decode["pages"]):
                raise Blocked("PDF decoder page receipt omitted actual rendered files")
            progress = [line.split(maxsplit=2) for line in command.get("stderr", "").splitlines()]
            if (not progress or any(len(row) != 3 or not row[0].isdigit() or not row[1].isdigit() for row in progress)
                    or [int(row[0]) for row in progress] != list(range(1, len(progress) + 1))
                    or any(int(row[1]) != len(progress) for row in progress)
                    or {row[2] for row in progress} != {"/" + name for name in decode["pages"]}):
                raise Blocked("PDF rendered pages disagree with actual decoder progress or contain decoder errors")
            numbers = []
            for name, sha in decode["pages"].items():
                page = absolute(case / name)
                if page.parent != case / "fx/runtime/decoded" / str(index) or not page.name.startswith("page-") or page.suffix != ".png":
                    raise Blocked("PDF rendered page escaped its decoder output directory")
                number = page.stem.removeprefix("page-")
                if not number.isdigit() or digest(page) != sha:
                    raise Blocked("PDF rendered page is missing or changed")
                numbers.append(int(number))
                with regular(page) as rendered:
                    head = rendered.read(64)
                if not head.startswith(b"\x89PNG\r\n\x1a\n") or len(head) < 24 or any(
                        not 0 < int.from_bytes(head[a:a+4], "big") <= 64 for a in (16, 20)):
                    raise Blocked("PDF decoder page is empty or exceeds its render bound")
            if sorted(numbers) != list(range(1, len(numbers) + 1)):
                raise Blocked("PDF decoder page inventory is not contiguous")
        elif (arguments != ["/opt/flea", "-regard-warnings", wanted, "-format", "%m\n", "info:"]
              or {name.upper() for name in command.get("stdout", "").split()} != {row["format"].upper()}):
            raise Blocked("converted image lacks an independently decoded format")
    return True


def cups_proof(case, record):
    """A private sink declaration needs actual HTTP/IPP Print-Job traffic and document bytes."""
    from . import cups
    if not (case / "cups-start.json").is_file():
        raise Blocked("private CUPS receiver start receipt is missing")
    start = load(case / "cups-start.json")
    if (record.get("transport") != "private-unix" or record.get("socket") != "/fx/runtime/cups.sock"
            or not record.get("requests") or record.get("errors") or record.get("thread_joined") is not True
            or record.get("producer") != "tui16-private-cups-v1"
            or record.get("producer_sha256") != digest(Path(cups.__file__))
            or start.get("producer_sha256") != record["producer_sha256"]
            or record.get("start_sha256") != digest(case / "cups-start.json")
            or not stat.S_ISSOCK(start.get("socket_mode", 0))):
        raise Blocked("private CUPS sink needs private socket and actual traffic")
    original = {}
    for row in load(case / "fixtures.json").get("copied", []):
        original.setdefault(row["sha256"], []).append(row["path"])
    printed, created = False, set()
    for request in record["requests"]:
        path = absolute(case / request["path"])
        if not path.is_relative_to(case) or digest(path) != request["sha256"]:
            raise Blocked("CUPS traffic artifact changed")
        from ..fixtures import regular_bytes
        data = regular_bytes(path)
        response = absolute(case / request["response"])
        if not response.is_relative_to(case) or digest(response) != request["response_sha256"]:
            raise Blocked("CUPS receiver response changed")
        if not start["start_ns"] < request.get("received_ns", 0) < record.get("closed_ns", 0):
            raise Blocked("CUPS receiver lifecycle does not cover request")
        if data.startswith(b"GET "):
            continue
        _, body = cups.http_request(io.BytesIO(data))
        operation = int.from_bytes(body[2:4], "big")
        _, document = cups.ipp_document(body)
        if operation != request.get("operation"):
            raise Blocked("CUPS request operation disagrees with receiver")
        if operation == 5 and request.get("status") == 0:
            created.add(request["job_id"])
        if operation not in (2, 6):
            continue
        if not document.startswith(b"%PDF-") or (operation == 6 and request.get("job_id") not in created):
            raise Blocked("private CUPS sink lacks actual document data or its created job")
        header, separator, reply = regular_bytes(response).partition(b"\r\n\r\n")
        if (not separator or not header.startswith(b"HTTP/1.1 200 ") or reply[2:4] != b"\0\0"
                or type(request.get("status")) is not int or request["status"] != 0):
            raise Blocked("private CUPS submission was not accepted")
        row = request.get("document", {})
        retained = absolute(case / row.get("path", ""))
        sha = hashlib.sha256(document).hexdigest()
        if (not retained.is_relative_to(case / "fx") or sha not in original
                or row.get("sha256") != sha or digest(retained) != sha or row.get("size") != len(document)):
            raise Blocked("private CUPS document differs from original fixture or retained receiver bytes")
        if not any(digest(absolute(case / "fx/v/work" / name)) == sha for name in original[sha]):
            raise Blocked("private CUPS source document no longer matches its original fixture")
        printed = True
    if not printed:
        raise Blocked("private CUPS sink received no accepted document")
    return True


def conversion_proof(case, receipt, check):
    output_proof(case, receipt)
    if check == "multiple-images-separate-PDF":
        sources = {source["path"] for source in receipt["sources"]}
        correspondence = [output.get("source") for output in receipt["outputs"]]
        if (len(sources) < 2 or len(correspondence) != len(sources) or set(correspondence) != sources
                or any(output["format"] != "PDF" for output in receipt["outputs"])):
            raise Blocked("separate PDF outputs must correspond one-to-one with all source images")


def validate_operations(directory, row):
    from ..control_native import sealed
    required = {"convert-variants": ("real-converted-output", "operation-output.json", output_proof),
                "operations-formats": ("output-validation", "operation-output.json", output_proof),
                "operations-refusals": ("no-physical-print", "cups-traffic.json", cups_proof)}
    if row["id"] not in required:
        return
    check, name, validate = required[row["id"]]
    bindings = load(directory / "checks.json")
    wanted = {check: None}
    if row["id"] == "convert-variants":
        wanted.update({"image-JPEG-PNG-WebP-AVIF-HEIC-TIFF-BMP-PDF": {"JPEG", "PNG", "WebP", "AVIF", "HEIC", "TIFF", "BMP", "PDF"},
                       "PDF-PNG-JPEG-WebP-TIFF": {"PNG", "JPEG", "WebP", "TIFF"},
                       "multiple-images-combined-PDF-in-list-order": {"PDF"}, "multiple-images-separate-PDF": {"PDF"},
                       "office-to-PDF-without-format-list": {"PDF"}, "PDF-all-pages-and-range": None,
                       "metadata-toggle-and-PDF-dimming": None, "collision-keeps-source": None})
    for check, formats in wanted.items():
        seen = set()
        for record in bindings[check]:
            case = absolute(directory / record["capture"])
            if not case.is_relative_to(directory / "captures") or name not in record["proof"]["artifacts"]:
                raise Blocked("operation assertion lacks its sealed side-effect receipt")
            receipt = sealed(case, name)
            if validate is output_proof:
                conversion_proof(case, receipt, check)
            else:
                validate(case, receipt)
            seen.update(o["format"] for o in receipt.get("outputs", []))
        if formats and not formats <= seen:
            raise Blocked("conversion matrix omitted actual format outputs: " + check)

