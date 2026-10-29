"""Private Unix-socket IPP fixture with retained receiver-side traffic and no printer backend."""
import os
from pathlib import Path
import socket
import stat
import struct
import threading
import time

from ..evidence import Blocked, Owned, absolute, digest, save

MAX_REQUEST = 64 * 1024 * 1024
PPD = b'''*PPD-Adobe: "4.3"
*FormatVersion: "4.3"
*FileVersion: "1.0"
*LanguageVersion: English
*LanguageEncoding: ISOLatin1
*PCFileName: "FLEA.PPD"
*Manufacturer: "Private fixture"
*ModelName: "Brother HL-L3270CDW"
*NickName: "Brother HL-L3270CDW"
*ColorDevice: True
*cupsFilter2: "application/pdf application/vnd.cups-pdf 0 -"
*OpenUI *Duplex/Two-sided: PickOne
*DefaultDuplex: None
*Duplex None/Off: ""
*Duplex DuplexNoTumble/Long edge: ""
*Duplex DuplexTumble/Short edge: ""
*CloseUI: *Duplex
*OpenUI *ColorModel/Color: PickOne
*DefaultColorModel: RGB
*ColorModel RGB/Color: ""
*ColorModel Gray/Grayscale: ""
*CloseUI: *ColorModel
'''


def http_request(stream, interim=None, allow_get=False):
    raw = bytearray()
    def line():
        value = stream.readline(65537)
        if not value or len(value) > 65536:
            raise Blocked("private CUPS request header is incomplete or too large")
        raw.extend(value)
        if len(raw) > MAX_REQUEST:
            raise Blocked("private CUPS request exceeds the fixture byte budget")
        return value
    first = line()
    get = allow_get and first.startswith(b"GET ")
    if not first.startswith(b"POST ") and not get:
        raise Blocked("private CUPS only accepts IPP POST requests")
    headers = {}
    while (value := line()) != b"\r\n":
        key, separator, value = value.partition(b":")
        if not separator or key.lower() in headers:
            raise Blocked("invalid or repeated private CUPS HTTP header")
        headers[key.lower()] = value.strip()
    if headers.get(b"expect", b"").lower() == b"100-continue" and interim:
        interim()
    def read(count):
        if not 0 <= count <= MAX_REQUEST or len(raw) + count > MAX_REQUEST:
            raise Blocked("private CUPS request exceeds the fixture byte budget")
        value = stream.read(count)
        if len(value) != count:
            raise Blocked("private CUPS request body is truncated")
        raw.extend(value)
        return value
    body = bytearray()
    if headers.get(b"transfer-encoding", b"").lower() == b"chunked":
        while True:
            count = int(line().split(b";", 1)[0].strip(), 16)
            if not count:
                while line() != b"\r\n":
                    pass
                break
            body.extend(read(count))
            if read(2) != b"\r\n":
                raise Blocked("invalid private CUPS chunk framing")
    else:
        body.extend(read(int(headers.get(b"content-length", b"0"))))
    if not get and headers.get(b"content-type", b"").split(b";", 1)[0].strip().lower() != b"application/ipp":
        raise Blocked("private CUPS request is not application/ipp")
    return bytes(raw), bytes(body)


def ipp_document(body):
    """Walk attribute lengths, so a document marker in an attribute can never become document data."""
    if len(body) < 9 or body[:2] not in (b"\x01\x01", b"\x02\x00"):
        raise Blocked("invalid private CUPS IPP header")
    offset, attributes, group = 8, [], None
    while offset < len(body):
        tag, offset = body[offset], offset + 1
        if tag == 3:
            return attributes, body[offset:]
        if tag < 0x10:
            group = tag
            continue
        if group is None or offset + 2 > len(body):
            raise Blocked("invalid private CUPS attribute group")
        name_size = int.from_bytes(body[offset:offset + 2], "big")
        offset += 2
        if offset + name_size + 2 > len(body):
            raise Blocked("truncated private CUPS attribute name")
        name = body[offset:offset + name_size]
        offset += name_size
        size = int.from_bytes(body[offset:offset + 2], "big")
        offset += 2
        if offset + size > len(body):
            raise Blocked("truncated private CUPS attribute value")
        attributes.append((group, tag, name, body[offset:offset + size]))
        offset += size
    raise Blocked("private CUPS request has no end-of-attributes delimiter")


def attribute(tag, name, value):
    name = name.encode()
    value = value.encode() if isinstance(value, str) else value
    return bytes([tag]) + struct.pack(">H", len(name)) + name + struct.pack(">H", len(value)) + value


class PrivateSink:
    def __init__(self, case, fx, *, reject=False):
        self.case, self.fx = Owned(case).path, absolute(fx)
        Owned(fx.parent).child(fx.name)
        self.path = absolute(fx / "runtime/cups.sock")
        self.reject = reject
        self.requests, self.errors = [], []
        self.stop = threading.Event()
        self.server = None
        self.thread = None
        self.jobs = set()
        self.next_job = 1

    def start(self):
        if self.path.exists() or self.path.is_symlink():
            raise Blocked("private CUPS socket already exists")
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            self.server.bind(str(self.path))
            self.path.chmod(0o600)
            self.server.listen(1)
            self.server.settimeout(.1)
            info = self.path.stat()
            save(self.case / "cups-start.json", {"producer": "tui16-private-cups-v1", "producer_sha256": digest(Path(__file__)),
                 "socket": "/fx/runtime/cups.sock", "socket_inode": info.st_ino, "socket_mode": stat.S_IFMT(info.st_mode),
                 "start_ns": time.time_ns(), "pid": os.getpid()})
            self.thread = threading.Thread(target=self.serve, name="private-cups", daemon=True)
            from ..calendar_native import shielded
            # The receiver inherits the blocked cancellation mask; only the main thread handles it after cleanup.
            with shielded():
                self.thread.start()
        except BaseException as error:
            from ..calendar_native import shielded
            with shielded():
                self.stop.set()
                self.server.close()
                started = self.thread is not None and self.thread.ident is not None
                if started:
                    self.thread.join(3)
                    if self.thread.is_alive():
                        raise Blocked("private CUPS startup receiver did not stop") from error
                self.errors.append("startup interrupted or failed: " + type(error).__name__)
                save(self.case / "cups-start-failure.json", {"reason": str(error), "type": type(error).__name__,
                     "thread_started": started, "thread_joined": started and not self.thread.is_alive()})
                self.server = None
            raise
        return self

    def response(self, body):
        operation = int.from_bytes(body[2:4], "big")
        attributes, document = ipp_document(body)
        status = 0x0500 if self.reject and operation in (2, 6) else 0
        job_id = None
        result = body[:2] + struct.pack(">H", status) + body[4:8] + b"\x01"
        result += attribute(0x47, "attributes-charset", "utf-8") + attribute(0x48, "attributes-natural-language", "en")
        if operation in (0x4001, 0x4002, 0x000b):
            result += b"\x04" + attribute(0x42, "printer-name", "flea-private")
            result += attribute(0x41, "printer-info", "Brother HL-L3270CDW")
            result += attribute(0x45, "printer-uri-supported", "ipp://localhost/printers/flea-private")
            result += attribute(0x23, "printer-state", struct.pack(">I", 3))
            result += attribute(0x22, "printer-is-accepting-jobs", b"\x01")
            result += attribute(0x49, "document-format-supported", "application/pdf")
            result += attribute(0x21, "copies-default", struct.pack(">I", 1))
            result += attribute(0x44, "sides-default", "one-sided")
            result += attribute(0x44, "print-color-mode-default", "color")
        elif operation in (2, 5, 6):
            if operation != 5 and not document.startswith(b"%PDF-"):
                raise Blocked("private Print-Job has no PDF document after its attributes")
            if operation in (2, 5):
                job_id = self.next_job
                self.next_job += 1
                self.jobs.add(job_id)
            else:
                for _, tag, name, value in attributes:
                    if name == b"job-id" and tag == 0x21 and len(value) == 4:
                        job_id = int.from_bytes(value, "big")
                    elif name == b"job-uri":
                        job_id = int(value.rsplit(b"/", 1)[-1])
                if job_id not in self.jobs:
                    raise Blocked("Send-Document has no private created job")
            result += b"\x02" + attribute(0x21, "job-id", struct.pack(">I", job_id))
            result += attribute(0x45, "job-uri", f"ipp://localhost/jobs/{job_id}")
            result += attribute(0x23, "job-state", struct.pack(">I", 8 if status else 3 if operation == 5 else 9))
        else:
            status = 0x0401
            result = result[:2] + struct.pack(">H", status) + result[4:]
        return result + b"\x03", operation, attributes, document, status, job_id

    def serve(self):
        while not self.stop.is_set():
            try:
                client, _ = self.server.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            try:
                with client:
                    client.settimeout(2)
                    with client.makefile("rb") as stream:
                        raw, body = http_request(stream, lambda: client.sendall(b"HTTP/1.1 100 Continue\r\n\r\n"), allow_get=True)
                    if raw.startswith(b"GET "):
                        response, operation, attrs, document, status, job_id = PPD, 0, [], b"", 0, None
                        mime = b"application/vnd.cups-ppd"
                    else:
                        response, operation, attrs, document, status, job_id = self.response(body)
                        mime = b"application/ipp"
                    serial = len(self.requests) + 1
                    directory = self.case / "cups"
                    directory.mkdir(exist_ok=True)
                    request_path, response_path = directory / f"{serial}.http", directory / f"{serial}.response"
                    request_path.write_bytes(raw)
                    wire = b"HTTP/1.1 200 OK\r\nContent-Type: " + mime + b"\r\nContent-Length: " + str(len(response)).encode() + b"\r\nConnection: close\r\n\r\n" + response
                    response_path.write_bytes(wire)
                    received = {"path": str(request_path.relative_to(self.case)), "sha256": digest(request_path),
                                "response": str(response_path.relative_to(self.case)), "response_sha256": digest(response_path),
                                "operation": operation, "status": status, "received_ns": time.time_ns(),
                                "job_id": job_id,
                                "attributes": [{"group": g, "tag": t, "name_hex": n.hex(), "value_hex": v.hex()} for g, t, n, v in attrs]}
                    if operation in (2, 6):
                        job = self.fx / "runtime/cups/jobs" / f"{serial}.pdf"
                        job.parent.mkdir(parents=True, exist_ok=True)
                        job.write_bytes(document)
                        received["document"] = {"path": str(Path("fx") / job.relative_to(self.fx)),
                                                "sha256": digest(job), "size": len(document)}
                    self.requests.append(received)
                    client.sendall(wire)
            except (OSError, ValueError, Blocked) as error:
                self.errors.append(str(error))

    def close(self):
        self.stop.set()
        if self.server:
            self.server.close()
        started = self.thread is not None and self.thread.ident is not None
        if started:
            self.thread.join(3)
            if self.thread.is_alive():
                raise Blocked("private CUPS receiver did not stop")
        record = {"producer": "tui16-private-cups-v1", "producer_sha256": digest(Path(__file__)),
                  "transport": "private-unix", "socket": "/fx/runtime/cups.sock", "requests": self.requests,
                  "errors": self.errors, "closed_ns": time.time_ns(), "thread_joined": started and not self.thread.is_alive(),
                  "start_sha256": digest(self.case / "cups-start.json")}
        save(self.case / "cups-traffic.json", record)
        return record
