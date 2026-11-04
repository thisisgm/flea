"""Actual private output bytes, independent decoders and source/output correspondence."""
import json
import os
from pathlib import Path
import tempfile
import unittest
import struct
import zlib
import tracemalloc

from .bundle import pdf, outputs
from . import png
from .evidence import Blocked, Commands, MARKER, Owned, digest


class OutputProofTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-output.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned independent output decoding\n")
        self.fx = self.root / "fx"
        for name in ("tmp", "home", "config", "cache", "state", "runtime", "data", "v/work"):
            (self.fx / name).mkdir(parents=True, exist_ok=True)
        self.work = self.fx / "v/work"
        self.sources = []
        for name in ("one.png", "two.png"):
            path = self.work / name
            png.write(path, 2, 2, bytes([255 if name == "one.png" else 0, 0, 0, 255]) * 4)
            self.sources.append({"path": "fx/v/work/" + name, "sha256": digest(path)})
        (self.root / "fixtures.json").write_text(json.dumps({"copied": [
            {"path": Path(s["path"]).name, "sha256": s["sha256"]} for s in self.sources]}))

    def test_actual_sandbox_decoder_receipt_accepts_valid_pdf(self):
        path = self.work / "combined.pdf"
        path.write_bytes(pdf.document([[(12, 20, 40, "output")]], "Fixture", "Fixture", 2048))
        commands = Commands()
        decode = outputs.decode_output(self.root, path, "PDF", commands)
        (self.root / "commands.json").write_text(json.dumps(commands.records))
        record = {"sources": self.sources, "outputs": [{"path": "fx/v/work/combined.pdf", "sha256": digest(path),
                                                         "format": "PDF", "decode": decode}]}
        self.assertTrue(outputs.output_proof(self.root, record))
        self.assertEqual(commands.records[0]["exit_code"], 0)
        self.assertEqual(commands.records[0]["argv"][0], "bwrap")
        self.assertIn("--unshare-all", commands.records[0]["argv"])

    def test_changed_source_cannot_rewrite_its_own_baseline_hash(self):
        source = self.work / "one.png"
        source.write_bytes((self.work / "two.png").read_bytes())
        output = self.work / "output.png"
        png.write(output, 2, 2, bytes([0, 0, 255, 255]) * 4)
        record = {"sources": [{"path": "fx/v/work/one.png", "sha256": digest(source)}],
                  "outputs": [{"path": "fx/v/work/output.png", "sha256": digest(output), "format": "PNG"}]}
        with self.assertRaisesRegex(Blocked, "original fixture"):
            outputs.output_proof(self.root, record)

    def test_avif_generic_mif1_is_not_a_heic_output(self):
        output = self.work / "avif.heic"
        output.write_bytes(b"\x00\x00\x00\x20ftypavif\x00\x00\x00\x00avifmif1" + b"\x00" * 64)
        record = {"sources": [self.sources[0]], "outputs": [
            {"path": "fx/v/work/avif.heic", "sha256": digest(output), "format": "HEIC"}]}
        with self.assertRaisesRegex(Blocked, "wrong actual format"):
            outputs.output_proof(self.root, record)

    def test_same_output_path_cannot_fill_two_matrix_rows(self):
        output = self.work / "output.png"
        png.write(output, 2, 2, bytes([0, 0, 255, 255]) * 4)
        row = {"path": "fx/v/work/output.png", "sha256": digest(output), "format": "PNG"}
        with self.assertRaisesRegex(Blocked, "duplicates"):
            outputs.output_proof(self.root, {"sources": self.sources, "outputs": [row, dict(row)]})

    def test_real_decoder_accepts_one_bit_png_and_refuses_missing_iend_or_invalid_palette(self):
        def chunk(name, data):
            return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data))
        def image(depth, color, palette=b""):
            return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, depth, color, 0, 0, 0))
                    + palette + chunk(b"IDAT", zlib.compress(b"\x00\x00")) + chunk(b"IEND", b""))
        valid = self.work / "one-bit.png"
        valid.write_bytes(image(1, 0))
        commands = Commands()
        decoded = outputs.decode_output(self.root, valid, "PNG", commands)
        (self.root / "commands.json").write_text(json.dumps(commands.records))
        record = {"sources": [self.sources[0]], "outputs": [{"path": "fx/v/work/one-bit.png", "sha256": digest(valid),
                                                           "format": "PNG", "decode": decoded}]}
        self.assertTrue(outputs.output_proof(self.root, record))
        for i, data in enumerate((image(8, 0)[:-12], image(8, 3, chunk(b"PLTE", b"\x00")))):
            invalid = self.work / ("invalid-" + str(i) + ".png")
            invalid.write_bytes(data)
            with self.assertRaises(Blocked):
                outputs.decode_output(self.root, invalid, "PNG", Commands())

    def test_pdf_page_tree_without_decodable_contents_cannot_pass(self):
        path = self.work / "broken-content.pdf"
        path.write_bytes(pdf.pack([
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << >> /Contents 3 0 R >>",
            "<< /Type /Pages /Count 1 /Kids [1 0 R] >>",
            "<< /Filter /FlateDecode /Length 7 >>\nstream\ninvalid\nendstream"], 2, "Fixture", "Fixture", 2048))
        commands = Commands()
        with self.assertRaises(Blocked):
            decode = outputs.decode_output(self.root, path, "PDF", commands)
            (self.root / "commands.json").write_text(json.dumps(commands.records))
            outputs.output_proof(self.root, {"sources": self.sources, "outputs": [
                {"path": "fx/v/work/broken-content.pdf", "sha256": digest(path), "format": "PDF", "decode": decode}]})

    def test_two_page_pdf_cannot_omit_a_rendered_page_even_if_file_is_removed(self):
        path = self.work / "two-pages.pdf"
        path.write_bytes(pdf.document([[(12,20,40,"one")], [(12,20,40,"two")]], "Fixture", "Fixture", 4096))
        commands = Commands()
        decode = outputs.decode_output(self.root, path, "PDF", commands)
        (self.root / "commands.json").write_text(json.dumps(commands.records))
        record = {"sources": self.sources, "outputs": [{"path": "fx/v/work/two-pages.pdf", "sha256": digest(path),
                  "format": "PDF", "decode": decode}]}
        self.assertEqual(len(decode["pages"]), 2)
        self.assertTrue(outputs.output_proof(self.root, record))
        missing = sorted(decode["pages"])[-1]
        decode["pages"].pop(missing)
        with self.assertRaisesRegex(Blocked, "omitted actual rendered"):
            outputs.output_proof(self.root, record)
        Owned(self.root).child(missing).unlink()
        with self.assertRaisesRegex(Blocked, "actual decoder progress"):
            outputs.output_proof(self.root, record)

    def test_actual_webp_decoder_format_label_matches_the_schema(self):
        from .fixtures import namespace
        commands = Commands()
        commands.run(namespace(Path("/usr/bin/magick"), self.fx, "/fx", {},
                               arguments=["/fx/v/work/one.png", "/fx/v/work/converted.webp"]))
        path = self.work / "converted.webp"
        decode = outputs.decode_output(self.root, path, "WebP", commands)
        (self.root / "commands.json").write_text(json.dumps(commands.records))
        self.assertTrue(outputs.output_proof(self.root, {"sources": [self.sources[0]], "outputs": [
            {"path": "fx/v/work/converted.webp", "sha256": digest(path), "format": "WebP", "decode": decode}]}))

    def test_unproven_oversized_png_uses_bounded_streaming_memory(self):
        path = Owned(self.root).child("fx/v/work/oversized.png")
        path.write_bytes(b"\x89PNG\r\n\x1a\n")
        with path.open("r+b") as fixture:
            fixture.truncate(129 * 1024 * 1024)
        record = {"sources": [self.sources[0]], "outputs": [
            {"path": "fx/v/work/oversized.png", "sha256": digest(path), "format": "PNG"}]}
        (self.root / "commands.json").write_text("[]")
        tracemalloc.start()
        try:
            with self.assertRaises(Blocked):
                outputs.output_proof(self.root, record)
            peak = tracemalloc.get_traced_memory()[1]
        finally:
            tracemalloc.stop()
        self.assertLess(peak, 8 * 1024 * 1024, "validator loaded the oversized artifact into memory")

    def test_separate_pdf_requires_every_source_output_with_real_decoder_receipts(self):
        commands, converted = Commands(), []
        for i, source in enumerate(self.sources):
            path = self.work / (str(i) + ".pdf")
            path.write_bytes(pdf.document([[(12, 20, 40, str(i))]], "Fixture", "Fixture", 2048))
            converted.append({"path": "fx/v/work/" + path.name, "sha256": digest(path), "format": "PDF",
                              "source": source["path"], "decode": outputs.decode_output(self.root, path, "PDF", commands)})
        (self.root / "commands.json").write_text(json.dumps(commands.records))
        record = {"sources": self.sources, "outputs": converted}
        outputs.conversion_proof(self.root, record, "multiple-images-separate-PDF")
        for missing in ({"sources": self.sources, "outputs": converted[:1]},
                        {"sources": self.sources, "outputs": [converted[0], dict(converted[1], source=self.sources[0]["path"])]}):
            with self.assertRaisesRegex(Blocked, "one-to-one"):
                outputs.conversion_proof(self.root, missing, "multiple-images-separate-PDF")


if __name__ == "__main__":
    unittest.main()
