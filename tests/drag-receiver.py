#!/usr/bin/env python3
"""One Wayland drop target. Logs the offer and exits.

tests/drag.sh starts this beside Flea. It is not a file manager: it accepts the
drop, writes the MIME types, the uri-list body and the action mask, and quits.
"""
import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, GLib, Gtk

log_path = sys.argv[1]


def write(text):
    with open(log_path, "a", encoding="utf-8") as handle:
        handle.write(text)
        if not text.endswith("\n"):
            handle.write("\n")


class Receiver(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="com.thisisgm.FleaDragReceiver")

    def do_activate(self):
        window = Gtk.ApplicationWindow(application=self, title="flea-drag-receiver")
        window.set_default_size(420, 320)
        label = Gtk.Label(label="drop here")
        label.set_hexpand(True)
        label.set_vexpand(True)
        window.set_child(label)
        target = Gtk.DropTargetAsync.new(
            Gdk.ContentFormats.new(["text/uri-list", "text/plain"]),
            Gdk.DragAction.COPY | Gdk.DragAction.MOVE,
        )
        target.connect("drop", self.on_drop)
        label.add_controller(target)
        window.present()
        write("ready")

    def on_drop(self, _target, drop, _x, _y):
        formats = drop.get_formats()
        write(f"actions={int(drop.get_actions())}")
        write(f"formats={formats.to_string() if formats is not None else ''}")
        drop.read_async(
            ["text/uri-list", "text/plain"],
            GLib.PRIORITY_DEFAULT,
            None,
            self.on_read,
        )
        return True

    def on_read(self, drop, result):
        try:
            stream, mime = drop.read_finish(result)
            chunks = []
            while True:
                piece = stream.read_bytes(65536, None)
                data = piece.get_data()
                if not data:
                    break
                chunks.append(data)
            body = b"".join(chunks).decode("utf-8", "replace")
            write(f"mime={mime}")
            write("body<<")
            write(body)
            write(">>")
        except Exception as error:
            write(f"read-error={error}")
        drop.finish(Gdk.DragAction.COPY)
        self.quit()


def main():
    app = Receiver()
    GLib.timeout_add_seconds(90, app.quit)
    raise SystemExit(app.run(None))


if __name__ == "__main__":
    main()
