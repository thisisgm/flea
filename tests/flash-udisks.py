#!/usr/bin/python3
# The helper's pure decisions without a daemon, gi stubbed; usage: flash-udisks.py [helper path]; D-Bus calls run on scsi_debug, never here.
import importlib.machinery
import importlib.util
import os
import socket
import sys
import types
sys.dont_write_bytecode = True

gi = types.ModuleType("gi")
gi.require_version = lambda *args: None
repository = types.ModuleType("gi.repository")
repository.Gio = types.ModuleType("Gio")
repository.GLib = types.ModuleType("GLib")
repository.UDisks = types.ModuleType("UDisks")
repository.GLib.Error = type("GLibError", (Exception,), {})
repository.GLib.Variant = lambda *args: args
repository.Gio.DBusCallFlags = types.SimpleNamespace(ALLOW_INTERACTIVE_AUTHORIZATION=0)
gi.repository = repository
sys.modules["gi"] = gi
sys.modules["gi.repository"] = repository
sys.modules["gi.repository.Gio"] = repository.Gio
sys.modules["gi.repository.GLib"] = repository.GLib
sys.modules["gi.repository.UDisks"] = repository.UDisks

loader = importlib.machinery.SourceFileLoader("flea_udisks", sys.argv[1] if len(sys.argv) > 1 else "tools/flea-udisks")
spec = importlib.util.spec_from_loader("flea_udisks", loader)
helper = importlib.util.module_from_spec(spec)
loader.exec_module(helper)

failures = []


def call(name, *args):
    # A helper without the function reads as a failed check, never a traceback.
    fn = getattr(helper, name, None)
    return fn(*args) if fn else "missing " + name


def seam(name, *args):
    # The bench seam beside the helper, the one the helper itself loads.
    module = getattr(helper, "SEAM", None)
    fn = getattr(module, name, None) if module else None
    return fn(*args) if fn else "missing seam " + name


def check(label, got, want):
    if got != want:
        failures.append(label)
        print("FAIL %s: got %r want %r" % (label, got, want))
    else:
        print("ok   %s" % label)


# The machine half of a D-Bus failure, never the prose past the second colon.
check("busy name", helper.error_name(Exception("GDBus.Error:org.freedesktop.UDisks2.Error.DeviceBusy: Device is busy")),
      "org.freedesktop.UDisks2.Error.DeviceBusy")
check("denied name", helper.error_name(Exception("GDBus.Error:org.freedesktop.PolicyKit1.Error.Failed: dismissed")),
      "org.freedesktop.PolicyKit1.Error.Failed")
check("no marker", helper.error_name(Exception("plain failure")), "")
check("busy predicate", (helper.busy_name("org.freedesktop.UDisks2.Error.DeviceBusy"),
                         helper.busy_name("org.freedesktop.UDisks2.Error.Failed")), (True, False))
check("denied predicate", (helper.denied_name("org.freedesktop.UDisks2.Error.NotAuthorized"),
                           helper.denied_name("org.freedesktop.UDisks2.Error.Failed")), (True, False))
# The daemon's own system-disk judgement, off machine fields alone.
check("no drive", helper.is_system_drive(None), True)
check("hint system", helper.is_system_drive({"HintSystem": True, "Removable": True}), True)
check("removable", helper.is_system_drive({"HintSystem": False, "Removable": True}), False)
check("usb bus", helper.is_system_drive({"HintSystem": False, "Removable": False, "ConnectionBus": "usb"}), False)
check("internal bus", helper.is_system_drive({"HintSystem": False, "Removable": False, "ConnectionBus": ""}), True)
# The disk itself or one of its partitions, /dev/sda beside /dev/sda1, never a prefix twin.
check("on drive", (helper.on_drive("sda", "/dev/sda"), helper.on_drive("sda", "/dev/sda1"),
                   helper.on_drive("sda", "/dev/sdaa"), helper.on_drive("sda", "/dev/nvme0n1")),
      (True, True, False, False))
# A name ending in a digit takes pN partitions, so nvme0n10 and mmcblk10 are other disks.
check("on drive digit names", (helper.on_drive("nvme0n1", "/dev/nvme0n1p1"), helper.on_drive("nvme0n1", "/dev/nvme0n10"),
                               helper.on_drive("nvme0n1", "/dev/nvme0n10p1"), helper.on_drive("nvme0n1", "/dev/nvme0n1p"),
                               helper.on_drive("mmcblk1", "/dev/mmcblk1p2"), helper.on_drive("mmcblk1", "/dev/mmcblk10"),
                               helper.on_drive("mmcblk1", "/dev/mmcblk10p1"), helper.on_drive("sda", "/dev/sda1p1")),
      (True, False, False, False, True, False, False, False))
# The whole disk is the block whose Device is exactly /dev/<dev>, however many partitions list before it.
listed = [("/p1", {"Device": "/dev/sda1"}), ("/p2", {"Device": "/dev/sda2"}), ("/disk", {"Device": "/dev/sda"})]
check("whole disk among partitions", (call("whole_disk", listed, "sda") or ("", {}))[0], "/disk")
check("no whole disk listed", call("whole_disk", listed[:2], "sda"), None)
# Only an unlocked encrypted volume has a cleartext device to lock, and only a mounted block a mount to undo.
check("needs lock", (call("needs_lock", {"Encrypted": True, "Cleartext": "/org/udisks/dm"}),
                     call("needs_lock", {"Encrypted": True, "Cleartext": "/"}),
                     call("needs_lock", {"Encrypted": False, "Cleartext": "/"})), (True, False, False))
check("needs unmount", (call("needs_unmount", {"MountPoints": ["/run/media/gm/x"]}), call("needs_unmount", {"MountPoints": []})), (True, False))
# Every answer on stdout is compact JSON, the one shape the backend reads.
check("compact drives", call("encode", {"drives": [{"dev": "sda", "system": False}]}), '{"drives":[{"dev":"sda","system":false}]}')
check("compact eject", (call("encode", {"ok": True}), call("encode", {"ok": False, "err": "busy"})), ('{"ok":true}', '{"ok":false,"err":"busy"}'))
# The portal's own ay rule, copied rather than reinvented for the Device bytestring.
check("bytestring", (helper.bytestring(b"/dev/sda\0"), helper.bytestring([47, 100, 101, 118]),
                    helper.bytestring("nope")), ("/dev/sda", "/dev", ""))
# The one-byte answers the backend reads off the socket.
check("status bytes", (helper.STATUS_OK, helper.STATUS_DENIED, helper.STATUS_BUSY,
                       helper.STATUS_CHANGED, helper.STATUS_ERROR), (b"O", b"D", b"B", b"C", b"E"))
# O_EXCL | O_DIRECT | O_CLOEXEC from the os constants that OpenDevice takes; O_SYNC is what OpenForRestore paid.
check("open flags", helper.OPEN_FLAGS, os.O_EXCL | os.O_DIRECT | os.O_CLOEXEC)
# The same flags as x86_64 Linux spells them in asm-generic/fcntl.h, so a misnamed constant cannot agree with itself.
if os.uname().machine == "x86_64":
    check("open flags as x86_64 numbers", helper.OPEN_FLAGS, 0o200 | 0o40000 | 0o2000000)
# O_DIRECTORY is the bit once misread as O_EXCL, and a block device open with it fails ENOTDIR.
check("open flags carry no O_DIRECTORY", helper.OPEN_FLAGS & os.O_DIRECTORY, 0)
check("open flags carry no O_SYNC", helper.OPEN_FLAGS & os.O_SYNC, 0)

# A loop device is a bench drive only with its backing file strictly inside a marked root; --loop-root may lead the arguments.
import tempfile
with tempfile.TemporaryDirectory() as tmp:
    root = os.path.join(tmp, "root")
    os.makedirs(root)
    sysfs = os.path.join(tmp, "sys")
    os.makedirs(os.path.join(sysfs, "block", "loop3", "loop"))
    backing = os.path.join(root, "backing.img")
    open(backing, "w").close()
    outside = os.path.join(tmp, "outside.img")
    open(outside, "w").close()
    link = os.path.join(root, "escape.img")
    os.symlink(outside, link)

    def point(path):
        with open(os.path.join(sysfs, "block", "loop3", "loop", "backing_file"), "w") as out:
            out.write(path + "\n")

    point(backing)
    check("loop without a marker", seam("loop_allowed", "loop3", root, sysfs), False)
    open(os.path.join(root, ".flea-test-sandbox"), "w").close()
    check("loop inside a marked root", seam("loop_allowed", "loop3", root, sysfs), True)
    check("loop with no root", seam("loop_allowed", "loop3", "", sysfs), False)
    check("a disk is never a loop", seam("loop_allowed", "sda", root, sysfs), False)
    point(outside)
    check("loop backed outside the root", seam("loop_allowed", "loop3", root, sysfs), False)
    point(link)
    check("loop backed through an escaping link", seam("loop_allowed", "loop3", root, sysfs), False)
    check("system verdict yields to the seam", call("treat_as_system", "loop3", None, root, sysfs), True)
    point(backing)
    check("system verdict with the seam", call("treat_as_system", "loop3", None, root, sysfs), False)
    check("system verdict for a real disk", call("treat_as_system", "sda", None, root, sysfs), True)
check("options lead the arguments", (call("split_options", ["--loop-root", "/r", "sda"]), call("split_options", ["sda", "sdb"])),
      (("/r", ["sda"]), ("", ["sda", "sdb"])))

# C-r2 F26: an unlocked LUKS volume's filesystem lives on its cleartext dm block, which is unmounted before the volume locks.
calls = []


class Recorder:
    def __init__(self, path):
        self.path = path

    def call_sync(self, method, *args):
        calls.append((self.path, method))


helper.iface_at = lambda path, iface: Recorder(path)
luks = [("/disk", {"Device": "/dev/sda", "MountPoints": [], "Encrypted": False, "Cleartext": "/", "Backing": "/"}),
        ("/sda1", {"Device": "/dev/sda1", "MountPoints": [], "Encrypted": True, "Cleartext": "/dm1", "Backing": "/"}),
        ("/dm1", {"Device": "/dev/dm-1", "MountPoints": ["/run/media/gm/x"], "Encrypted": False, "Cleartext": "/", "Backing": "/sda1"}),
        ("/dm2", {"Device": "/dev/dm-2", "MountPoints": ["/home"], "Encrypted": False, "Cleartext": "/", "Backing": "/nvme0n1p2"})]
check("cleartext unmount then lock", (helper.unmount_all(luks, "sda"), helper.lock_all(luks, "sda"), calls),
      ("", "", [("/dm1", "Unmount"), ("/sda1", "Lock")]))

# A volume unmounted since the list was read (the password prompt waits) is no busy drive; only a real refusal stops the pass.
refusals = {"/p1": "org.freedesktop.UDisks2.Error.NotMounted", "/p2": "org.freedesktop.UDisks2.Error.DeviceBusy"}
tried = []


class Refuser:
    def __init__(self, path):
        self.path = path

    def call_sync(self, method, *args):
        tried.append(self.path)
        raise helper.GLib.Error("GDBus.Error:%s: refused" % refusals[self.path])


helper.iface_at = lambda path, iface: Refuser(path)
mounted = [("/disk", {"Device": "/dev/sdb", "MountPoints": [], "Encrypted": False, "Cleartext": "/", "Backing": "/"}),
           ("/p1", {"Device": "/dev/sdb1", "MountPoints": ["/run/media/gm/a"], "Encrypted": False, "Cleartext": "/", "Backing": "/"}),
           ("/p2", {"Device": "/dev/sdb2", "MountPoints": ["/run/media/gm/b"], "Encrypted": False, "Cleartext": "/", "Backing": "/"})]
check("a volume already unmounted is passed over, a busy one stops the pass", (helper.unmount_all(mounted, "sdb"), tried), ("busy", ["/p1", "/p2"]))
tried.clear()
check("an unmounted volume alone is no refusal", (helper.unmount_all(mounted[:2], "sdb"), tried), ("", ["/p1"]))
helper.iface_at = lambda path, iface: Recorder(path)

# A-r2 F3: eject asks the daemon's own system verdict again, so a removable disk udisks hints as system is never powered off.
import contextlib
import io
calls.clear()
luks[0][1]["Drive"] = "/drive"
helper.blocks = lambda: luks
helper.drive_for = lambda path: {"HintSystem": True, "Removable": True}
answer = io.StringIO()
with contextlib.redirect_stdout(answer):
    helper.cmd_eject("sda", "")
check("eject refuses a drive udisks hints as system", (answer.getvalue().strip(), calls), ('{"ok":false,"err":"error"}', []))

# The control: the same drive and blocks with HintSystem off unmount the cleartext, lock, then power off.
calls.clear()
helper.drive_for = lambda path: {"HintSystem": False, "Removable": True}
answer = io.StringIO()
with contextlib.redirect_stdout(answer):
    helper.cmd_eject("sda", "")
check("eject unmounts the cleartext, locks, then powers off", (answer.getvalue().strip(), calls),
      ('{"ok":true}', [("/dm1", "Unmount"), ("/sda1", "Lock"), ("/drive", "PowerOff")]))

# C-r2 F25: the helper as packaging installs it, alone in its directory, carries no bench seam at all.
import shutil
with tempfile.TemporaryDirectory() as tmp:
    lone_dir = os.path.join(tmp, "lib")
    os.makedirs(lone_dir)
    lone_path = os.path.join(lone_dir, "flea-udisks")
    shutil.copy(loader.path, lone_path)
    lone_loader = importlib.machinery.SourceFileLoader("flea_udisks_lone", lone_path)
    lone = importlib.util.module_from_spec(importlib.util.spec_from_loader("flea_udisks_lone", lone_loader))
    lone_loader.exec_module(lone)
    root = os.path.join(tmp, "root")
    os.makedirs(root)
    open(os.path.join(root, ".flea-test-sandbox"), "w").close()
    backing = os.path.join(root, "backing.img")
    open(backing, "w").close()
    sysfs = os.path.join(tmp, "sys")
    os.makedirs(os.path.join(sysfs, "block", "loop3", "loop"))
    with open(os.path.join(sysfs, "block", "loop3", "loop", "backing_file"), "w") as out:
        out.write(backing + "\n")
    check("an installed helper keeps the daemon's system verdict for a marked loop", lone.treat_as_system("loop3", None, root, sysfs), True)
    lone.blocks = lambda: []
    check("an installed helper refuses --loop-root", lone.main(["flea-udisks", "drives", "--loop-root", root, "loop3"]), 2)

# OpenDevice flags travel as i: udisks looks up flags with g_variant_lookup "i", so a u misses and the open silently drops O_EXCL and O_DIRECT.
seen = {}


class TapedVariant:
    def __init__(self, fmt, value):
        self.fmt = fmt
        self.value = value

    def unpack(self):
        return self.value


class TapedProxy:
    @classmethod
    def new_for_bus_sync(cls, *args):
        return cls()

    # Gio.DBusProxy's own in-arguments, named as GObject introspection names them; the reply type belongs to Gio.DBusConnection's call.
    def call_with_unix_fd_list_sync(self, method_name, parameters, flags, timeout_msec, fd_list, cancellable):
        seen["method"] = method_name
        seen["params"] = parameters
        seen["rest"] = (flags, timeout_msec, fd_list, cancellable)
        fd = os.open("/dev/null", os.O_RDONLY)
        seen["fd"] = fd

        class TapedList:
            def get(self, _i):
                return fd

        return (TapedVariant("(h)", (0,)), TapedList())


taped = types.SimpleNamespace(sent=[])
# A refusal answers through sendmsg, so a broken OpenDevice call reads as a failed check, never a traceback.
taped.sendmsg = lambda data, _anc: taped.sent.append(data)
helper.GLib.Variant = TapedVariant
helper.GLib.VariantType = lambda *args: args
helper.Gio.BusType = types.SimpleNamespace(SYSTEM=0)
helper.Gio.DBusProxyFlags = types.SimpleNamespace(NONE=0)
helper.Gio.DBusProxy = TapedProxy
helper.socket = types.SimpleNamespace(fromfd=lambda *args: taped,
                                      send_fds=lambda sock, status, fds: taped.sent.append((status, fds)),
                                      AF_UNIX=socket.AF_UNIX, SOCK_SEQPACKET=socket.SOCK_SEQPACKET)
helper.blocks = lambda: luks
helper.drive_for = lambda path: {"HintSystem": False, "Removable": True, "ConnectionBus": "usb", "Serial": ""}
helper.check_open_authorized = lambda: True
helper.cmd_open("sda", "", 0, "")
params = seen.get("params")
inner = params.value if params else None
flags_variant = inner[1]["flags"] if inner else None
check("OpenDevice flags travel as i", (seen.get("method"), getattr(flags_variant, "fmt", None),
                                       getattr(flags_variant, "value", None), inner[0] if inner else None),
      ("OpenDevice", "i", helper.OPEN_FLAGS, "rw"))
check("OpenDevice passes flags, timeout, no fd list, no cancellable", seen.get("rest"),
      (helper.Gio.DBusCallFlags.ALLOW_INTERACTIVE_AUTHORIZATION, -1, None, None))
check("the descriptor answer carries ok and the opened descriptor", taped.sent, [([helper.STATUS_OK], [seen.get("fd")])])
if "fd" in seen:
    os.close(seen["fd"])

# EY-549: the fake's parameters are the real proxy's in-arguments, read from PyGObject where it exists (East2); skipped where gi is absent; GIDirection 1 is OUT (out_fd_list).
import inspect
import subprocess
probe = subprocess.run([sys.executable, "-I", "-c",
                        "try:\n    import gi\nexcept ImportError:\n    raise SystemExit('no-gi')\n"
                        "gi.require_version('Gio', '2.0')\nfrom gi.repository import Gio\n"
                        "print(' '.join(a.get_name() for a in Gio.DBusProxy.call_with_unix_fd_list_sync.get_arguments()"
                        " if int(a.get_direction()) != 1))"],
                       capture_output=True, text=True)
if probe.stderr.strip() == "no-gi":
    print("skip the fake proxy takes the real in-arguments: no PyGObject here")
else:
    if probe.returncode:
        print(probe.stderr.strip())
    check("the fake proxy takes the real in-arguments", (probe.returncode, probe.stdout.split()),
          (0, list(inspect.signature(TapedProxy.call_with_unix_fd_list_sync).parameters)[1:]))


# A proxy the daemon never hands over still answers the socket, never a traceback.
class RefusingProxy:
    @classmethod
    def new_for_bus_sync(cls, *args):
        raise helper.GLib.Error("GDBus.Error:org.freedesktop.DBus.Error.NoReply: daemon is gone")


helper.Gio.DBusProxy = RefusingProxy
taped.sent.clear()
check("a refused proxy answers error", (helper.cmd_open("sda", "", 0, ""), taped.sent[-1:]),
      (1, [[helper.STATUS_ERROR]]))
helper.Gio.DBusProxy = TapedProxy

if failures:
    print("%d failed: %s" % (len(failures), failures))
    sys.exit(1)
print("flash-udisks: all checks passed")
