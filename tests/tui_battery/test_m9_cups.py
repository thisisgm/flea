"""Real receiver-side private IPC controls, with no Flea, native display or physical printer."""
import hashlib
import io
import json
import os
from pathlib import Path
import socket
import struct
import tempfile
import unittest

from .bundle import cups, pdf
from .bundle.outputs import cups_proof
from .evidence import Blocked, MARKER, digest


def request(document, operation=2, extra=b""):
    charset = b"\x47\x00\x12attributes-charset\x00\x05utf-8"
    language = b"\x48\x00\x1battributes-natural-language\x00\x02en"
    body = b"\x02\x00" + struct.pack(">HI", operation, 1) + b"\x01" + charset + language + extra + b"\x03" + document
    return (b"POST /printers/flea-private HTTP/1.1\r\nContent-Type: application/ipp\r\nContent-Length: "
            + str(len(body)).encode() + b"\r\n\r\n" + body)


class PrivateCupsTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-cups.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned private CUPS IPC fixture\n")
        self.fx = self.root / "fx"
        work = self.fx / "v/work"
        work.mkdir(parents=True)
        self.document = pdf.document([[(12, 20, 40, "private fixture")]], "Fixture", "Fixture", 2048)
        (work / "input.pdf").write_bytes(self.document)
        (self.root / "fixtures.json").write_text(json.dumps({"copied": [
            {"path": "input.pdf", "sha256": hashlib.sha256(self.document).hexdigest()}]}))

    def send(self, sink, wire):
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(5)
            client.connect(str(sink.path))
            client.sendall(wire)
            chunks = []
            while data := client.recv(65536):
                chunks.append(data)
        return b"".join(chunks)

    def test_actual_private_receiver_records_document_and_stops(self):
        sink = cups.PrivateSink(self.root, self.fx).start()
        try:
            reply = self.send(sink, request(self.document))
        finally:
            receipt = sink.close()
        self.assertTrue(reply.startswith(b"HTTP/1.1 200 OK"))
        self.assertFalse(sink.thread.is_alive())
        self.assertEqual(receipt["errors"], [])
        self.assertTrue(cups_proof(self.root, receipt))
        retained = self.root / receipt["requests"][0]["document"]["path"]
        self.assertEqual(retained.read_bytes(), self.document)

    def test_declared_socket_and_request_file_without_receiver_cannot_pass(self):
        raw = self.root / "fabricated.http"
        raw.write_bytes(request(self.document))
        fake = {"transport": "private-unix", "socket": "/fx/runtime/cups.sock",
                "requests": [{"path": raw.name, "sha256": digest(raw)}]}
        with self.assertRaises(Blocked):
            cups_proof(self.root, fake)

    def test_pdf_marker_inside_job_name_is_not_document_data(self):
        value = b"%PDF- only in attribute"
        extra = b"\x42\x00\x08job-name" + struct.pack(">H", len(value)) + value
        wire = request(b"", extra=extra)
        _, body = cups.http_request(io.BytesIO(wire))
        _, document = cups.ipp_document(body)
        self.assertEqual(document, b"")
        sink = cups.PrivateSink(self.root, self.fx).start()
        try:
            self.assertEqual(self.send(sink, wire), b"")
        finally:
            receipt = sink.close()
        self.assertTrue(receipt["errors"])
        with self.assertRaises(Blocked):
            cups_proof(self.root, receipt)

    def test_rejected_request_does_not_become_accepted_print_proof(self):
        sink = cups.PrivateSink(self.root, self.fx, reject=True).start()
        try:
            self.send(sink, request(self.document))
        finally:
            receipt = sink.close()
        with self.assertRaisesRegex(Blocked, "not accepted"):
            cups_proof(self.root, receipt)

    def test_create_job_then_send_document_is_correlated(self):
        sink = cups.PrivateSink(self.root, self.fx).start()
        try:
            response = self.send(sink, request(b"", operation=5))
            _, _, body = response.partition(b"\r\n\r\n")
            attributes, _ = cups.ipp_document(body)
            job = next(value for _, _, name, value in attributes if name == b"job-id")
            extra = b"\x21\x00\x06job-id\x00\x04" + job
            self.send(sink, request(self.document, operation=6, extra=extra))
        finally:
            receipt = sink.close()
        self.assertEqual([r["operation"] for r in receipt["requests"]], [5, 6])
        self.assertTrue(cups_proof(self.root, receipt))

    def test_chunked_request_preserves_wire_bytes_and_document(self):
        original = request(self.document)
        _, body = cups.http_request(io.BytesIO(original))
        wire = (b"POST /printers/flea-private HTTP/1.1\r\nContent-Type: application/ipp\r\nTransfer-Encoding: chunked\r\n\r\n"
                + hex(len(body))[2:].encode() + b"\r\n" + body + b"\r\n0\r\n\r\n")
        raw, decoded = cups.http_request(io.BytesIO(wire))
        self.assertEqual(raw, wire)
        self.assertEqual(decoded, body)
        sink = cups.PrivateSink(self.root, self.fx).start()
        try:
            self.send(sink, wire)
        finally:
            receipt = sink.close()
        self.assertTrue(cups_proof(self.root, receipt))


if __name__ == "__main__":
    unittest.main()
