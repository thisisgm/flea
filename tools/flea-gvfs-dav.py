"""Read-only loopback DAV fixture, controlled through its sandbox Unix socket."""
import email.utils
import http.server
import os
from pathlib import Path
import re
import selectors
import socket
import subprocess
import sys
import urllib.parse
import xml.etree.ElementTree as ET


class Handler(http.server.BaseHTTPRequestHandler):
    def setup(self):
        self.request.settimeout(2)
        super().setup()

    def log_message(self, template, *args):
        print(template % args, file=sys.stderr, flush=True)

    def target(self):
        path = urllib.parse.unquote(urllib.parse.urlsplit(self.path).path)
        if path in ("/", "/share.zip", "/share.zip/"):
            return self.server.root
        prefix = "/share.zip/"
        if not path.startswith(prefix):
            return None
        name = path[len(prefix):]
        target = self.server.root / name
        if not name or "/" in name or name in (".", "..") or target.is_symlink():
            return None
        return target if target.is_file() else None

    def reply(self, status, body=b"", content_type="text/plain"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("DAV", "1")
        self.send_header("Allow", "OPTIONS, PROPFIND, GET, HEAD")
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def do_OPTIONS(self):
        self.reply(200)

    def do_PROPFIND(self):
        target = self.target()
        if target is None:
            self.reply(404)
            return
        depth = self.headers.get("Depth", "infinity")
        if depth not in ("0", "1"):
            self.reply(403)
            return
        multistatus = ET.Element("{DAV:}multistatus")
        targets = [target]
        if target == self.server.root and depth == "1":
            targets += sorted(p for p in target.iterdir() if p.is_file() and not p.is_symlink())
        for item in targets:
            folder = item == self.server.root
            href = "/share.zip/" + ("" if folder else urllib.parse.quote(item.name))
            response = ET.SubElement(multistatus, "{DAV:}response")
            ET.SubElement(response, "{DAV:}href").text = href
            propstat = ET.SubElement(response, "{DAV:}propstat")
            prop = ET.SubElement(propstat, "{DAV:}prop")
            ET.SubElement(prop, "{DAV:}displayname").text = "share.zip" if folder else item.name
            resource = ET.SubElement(prop, "{DAV:}resourcetype")
            if folder:
                ET.SubElement(resource, "{DAV:}collection")
            metadata = item.stat()
            ET.SubElement(prop, "{DAV:}getcontentlength").text = str(0 if folder else metadata.st_size)
            ET.SubElement(prop, "{DAV:}getcontenttype").text = "httpd/unix-directory" if folder else "text/plain"
            ET.SubElement(prop, "{DAV:}getlastmodified").text = email.utils.formatdate(metadata.st_mtime, usegmt=True)
            ET.SubElement(prop, "{DAV:}getetag").text = f'"{metadata.st_mtime_ns:x}-{metadata.st_size:x}"'
            ET.SubElement(propstat, "{DAV:}status").text = "HTTP/1.1 200 OK"
        self.reply(207, ET.tostring(multistatus, encoding="utf-8", xml_declaration=True), "application/xml")

    def do_GET(self):
        target = self.target()
        if target is None:
            self.reply(404)
        elif target == self.server.root:
            self.reply(200)
        else:
            self.reply(200, target.read_bytes())

    do_HEAD = do_GET


def uri(root):
    value = (root / "uri").read_text().strip()
    if not re.fullmatch(r"dav://127\.0\.0\.1:[0-9]+/share\.zip/", value):
        raise RuntimeError("invalid fixture URI")
    return value


def serve(root):
    os.umask(0o077)
    control_path = root / "control.sock"
    stop_client = None
    with socket.socket(socket.AF_UNIX) as control:
        control.bind(str(control_path))
        control.listen(1)
        try:
            with http.server.HTTPServer(("127.0.0.1", 0), Handler) as server:
                server.root = root / "src"
                value = f"dav://127.0.0.1:{server.server_port}/share.zip/"
                (root / "uri").write_text(value + "\n")
                print(value, flush=True)
                sys.stdout.close()
                with selectors.DefaultSelector() as ready:
                    ready.register(control, selectors.EVENT_READ)
                    ready.register(server, selectors.EVENT_READ)
                    while stop_client is None:
                        for key, _ in ready.select():
                            if key.fileobj is control:
                                client = control.accept()[0]
                                client.settimeout(2)
                                try:
                                    if client.recv(16) == b"stop":
                                        stop_client = client
                                    else:
                                        client.close()
                                except OSError:
                                    client.close()
                            else:
                                server.handle_request()
        finally:
            control_path.unlink(missing_ok=True)
            if stop_client is not None:
                stop_client.close()


def start(root):
    with (root / "server.log").open("w") as log:
        process = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()), "serve", str(root)],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=log)
    try:
        with selectors.DefaultSelector() as ready:
            ready.register(process.stdout, selectors.EVENT_READ)
            if not ready.select(timeout=5):
                raise RuntimeError("DAV server readiness timed out")
        result = process.stdout.readline().decode().strip()
        if not result:
            raise RuntimeError("DAV server failed: " + (root / "server.log").read_text().strip())
        if result != uri(root):
            raise RuntimeError("DAV server returned an unexpected URI")
        print(result)
    except BaseException:
        process.terminate()
        process.wait(timeout=5)
        raise
    finally:
        process.stdout.close()


def stop(root):
    path = root / "control.sock"
    if not path.exists():
        return
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(5)
        try:
            client.connect(str(path))
        except ConnectionRefusedError:
            # A killed server leaves its socket file behind with nobody listening; the caller removes the sandbox.
            return
        client.sendall(b"stop")
        if client.recv(1):
            raise RuntimeError("invalid DAV stop response")
    if path.exists():
        raise RuntimeError("DAV server did not stop")


def main():
    action, directory = sys.argv[1:]
    root = Path(directory)
    if not root.is_absolute() or root.is_symlink() or not (root / ".flea-test-sandbox").is_file():
        raise RuntimeError("expected an owned absolute fixture directory")
    if action == "start":
        start(root)
    elif action == "serve":
        serve(root)
    elif action == "uri":
        print(uri(root))
    elif action == "stop":
        stop(root)
    else:
        raise RuntimeError("unknown DAV fixture action")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"flea-gvfs-fixture: {error}", file=sys.stderr)
        sys.exit(1)
