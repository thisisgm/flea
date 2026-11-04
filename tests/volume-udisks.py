#!/usr/bin/python3
import contextlib
import copy
import importlib.machinery
import importlib.util
import io
import json
from pathlib import Path
import sys
import types
import unittest


class Variant:
    def __init__(self, signature, value):
        self.signature = signature
        self.value = value

    def unpack(self):
        return self.value


gi = types.ModuleType("gi")
gi.require_version = lambda *args: None
repository = types.ModuleType("gi.repository")
repository.GLib = types.SimpleNamespace(Error=type("BusError", (Exception,), {}), Variant=Variant)
repository.Gio = types.SimpleNamespace(DBusCallFlags=types.SimpleNamespace(ALLOW_INTERACTIVE_AUTHORIZATION=1))
gi.repository = repository
sys.modules.update({"gi": gi, "gi.repository": repository})
loader = importlib.machinery.SourceFileLoader("volume_helper", sys.argv[1] if len(sys.argv) > 1 else "tools/flea-udisks")
helper = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
loader.exec_module(helper)
original_drive_for = helper.drive_for


class Interface:
    def __init__(self, fixture, path, name):
        self.fixture, self.path, self.name = fixture, path, name

    def get_cached_property(self, key):
        data = self.fixture["drives"][self.path] if self.name == "Drive" else self.fixture["blocks"][self.path][self.name]
        value = data.get(key)
        if key == "Device" and isinstance(value, str):
            value = value.encode() + b"\0"
        if key == "MountPoints" and value is not None:
            value = [point.encode() + b"\0" for point in value]
        return Variant("", value) if value is not None else None

    def call_sync(self, method, options, *args):
        calls.append((self.path, self.name, method, options.unpack()))
        failure = errors.get((self.path, method))
        if callable(failure):
            failure = failure()
        if failure:
            raise helper.GLib.Error("GDBus.Error:org.freedesktop.UDisks2.Error." + failure + ": fixture refusal")
        if method == "Mount":
            self.fixture["blocks"][self.path]["Filesystem"]["MountPoints"] = ["/run/media/user/Data"]
            return Variant("(s)", ("/run/media/user/Data",))
        if method == "Unmount":
            self.fixture["blocks"][self.path]["Filesystem"]["MountPoints"].pop(0)
        if method == "PowerOff" and any(block.get("Filesystem", {}).get("MountPoints") for block in self.fixture["blocks"].values()):
            raise helper.GLib.Error("GDBus.Error:org.freedesktop.UDisks2.Error.DeviceBusy: still mounted")
        return Variant("()", ())


class Object:
    def __init__(self, fixture, path):
        self.fixture, self.path = fixture, path

    def get_object_path(self):
        return self.path

    def get_interface(self, name):
        short = name.rsplit(".", 1)[-1]
        return Interface(self.fixture, self.path, short) if short in self.fixture["blocks"][self.path] else None


calls, errors = [], {}


class Volumes(unittest.TestCase):
    def setUp(self):
        self.data = json.loads(Path("tests/fixtures/mounts/mbr-ntfs.json").read_text())
        self.attach()

    def attach(self):
        calls.clear()
        errors.clear()
        helper.manager = lambda: types.SimpleNamespace(get_objects=lambda: [Object(self.data, path) for path in self.data["blocks"]])
        helper.iface_at = lambda path, name: Interface(self.data, path, name.rsplit(".", 1)[-1])
        helper.Gio.BusType = types.SimpleNamespace(SYSTEM=0)
        helper.Gio.DBusProxyFlags = types.SimpleNamespace(NONE=0)
        helper.Gio.DBusProxy = types.SimpleNamespace(new_for_bus_sync=lambda *args: Interface(self.data, args[4], args[5].rsplit(".", 1)[-1]))
        helper.drive_for = original_drive_for

    def command(self, *args):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = helper.main(["flea-udisks", *args])
        self.assertEqual(status, 0)
        return json.loads(output.getvalue())

    def rows(self, unmounted=False):
        return self.command("volumes", *(["--unmounted"] if unmounted else []))["volumes"]

    def test_udisks_80_rules_ntfs_recovery_label_and_partition_table(self):
        for switch in (False, True):
            with self.subTest(switch=switch):
                rows = self.rows(switch)
                self.assertEqual([row["device"] for row in rows], ["/dev/sdb2"])
                self.assertEqual(rows[0]["kind"], "volume")
                self.assertEqual(rows[0]["release"], "unmount")
                self.assertEqual(rows[0]["releaseGlyph"], "eject")
                self.assertFalse(rows[0]["removable"])

    def test_gvfs_should_include_volume_filesystem_independent_of_internal_transport(self):
        self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"] = []
        self.assertEqual(self.rows(), [])
        rows = self.rows(True)
        self.assertEqual([row["device"] for row in rows], ["/dev/sdb2"])
        self.assertFalse(rows[0]["mounted"])
        self.assertEqual(rows[0]["release"], "")
        self.assertEqual(calls, [])

    def test_udisks_80_rules_gpt_efi_hfsplus_and_usb_bridge(self):
        self.data = json.loads(Path("tests/fixtures/mounts/gpt-hfsplus.json").read_text())
        self.attach()
        for switch in (False, True):
            rows = self.rows(switch)
            self.assertEqual([row["device"] for row in rows], ["/dev/sdc2"])
            self.assertTrue(rows[0]["removable"])
            self.assertEqual(rows[0]["release"], "eject")

    def test_gvfs_should_include_volume_hint_ignore_trumps_everything(self):
        self.data["blocks"]["/blocks/sdb2"]["Block"]["HintIgnore"] = True
        self.assertEqual(self.rows(), [])
        self.assertEqual(self.rows(True), [])

    def test_udisks_block_hint_system_hides_even_mounted_filesystems(self):
        self.data["blocks"]["/blocks/sdb2"]["Block"]["HintSystem"] = True
        self.assertEqual(self.rows(), [])
        self.assertEqual(self.rows(True), [])

    def test_udisks_80_rules_partition_types_do_not_depend_on_labels(self):
        for parttype in ("0xef", "EF", "0x27", "0x17", "c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "E3C9E316-0B5C-4DB8-817D-F92DF00215AE", "de94bba4-06d1-4d40-a16a-bfd50179d6ac"):
            with self.subTest(parttype=parttype):
                self.data["blocks"]["/blocks/sdb2"]["Partition"]["Type"] = parttype
                self.assertEqual(self.rows(True), [])

    def test_udisks_80_rules_isohybrid_filesystem_undo_keeps_installer_partition(self):
        block = self.data["blocks"]["/blocks/sdb2"]
        for parttype in ("0x0", "0x17"):
            block["Partition"].update(Type=parttype, Number=1)
            block["Block"].update(IdType="iso9660", IdLabel="Installer")
            self.assertEqual([row["device"] for row in self.rows(True)], ["/dev/sdb2"])
        block["Block"]["HintIgnore"] = True
        self.assertEqual(self.rows(True), [])

    def test_filesystem_interface_and_usage_are_required(self):
        block = self.data["blocks"]["/blocks/sdb2"]
        for key, value in (("IdType", ""), ("IdUsage", "raid")):
            original = block["Block"][key]
            block["Block"][key] = value
            self.assertEqual(self.rows(True), [])
            block["Block"][key] = original
        del block["Filesystem"]
        self.assertEqual(self.rows(True), [])

    def test_whole_disk_with_filesystem_and_partition_table_is_never_a_volume(self):
        disk = self.data["blocks"]["/blocks/sdb"]
        disk["Block"].update(IdType="ntfs", IdUsage="filesystem")
        disk["Filesystem"] = {"MountPoints": ["/mnt/whole"]}
        self.assertEqual([row["device"] for row in self.rows(True)], ["/dev/sdb2"])
        del disk["PartitionTable"]
        self.assertEqual(len(self.rows(True)), 2)

    def test_release_fixed_mounted_volume_ignores_show_unmounted_setting(self):
        self.data["drives"]["/drives/data"]["CanPowerOff"] = True
        self.assertEqual(self.command("release", "/mnt/data/subdir"), {"ok": True})
        self.assertEqual([(path, method) for path, _, method, _ in calls], [("/blocks/sdb2", "Unmount")])
        self.assertEqual(calls[0][3], ({},))

    def test_release_ignores_other_mounts_on_a_fixed_drive(self):
        self.data["blocks"]["/blocks/sdb1"]["Filesystem"]["MountPoints"] = ["/boot"]
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": True})
        self.assertEqual([(path, method) for path, _, method, _ in calls], [("/blocks/sdb2", "Unmount")])

    def test_release_uses_component_boundaries_and_never_a_system_mount(self):
        for path in ("/mnt/database", "/", "/mnt", "relative"):
            self.assertEqual(self.command("release", path), {"ok": False, "err": "notListed"})
        self.assertEqual(calls, [])
        self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"] = ["/"]
        self.assertEqual(self.rows(True), [])
        self.assertEqual(self.command("release", "/etc"), {"ok": False, "err": "notListed"})

    def test_release_selects_longest_mount_and_preserves_hidden_nested_mount(self):
        nested = copy.deepcopy(self.data["blocks"]["/blocks/sdb2"])
        nested["Block"]["Device"] = "/dev/sdb3"
        nested["Filesystem"]["MountPoints"] = ["/mnt/data/nested"]
        self.data["blocks"]["/blocks/sdb3"] = nested
        self.assertEqual(self.command("release", "/mnt/data/nested/file"), {"ok": True})
        self.assertEqual(calls[0][0], "/blocks/sdb3")
        calls.clear()
        nested["Filesystem"]["MountPoints"] = ["/mnt/data/nested"]
        nested["Block"]["HintIgnore"] = True
        self.assertEqual(self.command("release", "/mnt/data/nested/file"), {"ok": False, "err": "notListed"})
        self.assertEqual(calls, [])

    def test_usb_release_unmounts_siblings_then_powers_off_without_remount(self):
        self.data = json.loads(Path("tests/fixtures/mounts/gpt-hfsplus.json").read_text())
        self.data["blocks"]["/blocks/sdc1"]["Filesystem"]["MountPoints"] = ["/run/media/user/EFI"]
        self.attach()
        self.assertEqual(self.command("release", "/run/media/user/Archive/file"), {"ok": True})
        self.assertEqual(set((path, method) for path, _, method, _ in calls[:-1]), {("/blocks/sdc1", "Unmount"), ("/blocks/sdc2", "Unmount")})
        self.assertEqual([(path, method) for path, _, method, _ in calls[-1:]], [("/drives/data", "PowerOff")])
        self.assertEqual(len(calls), 3)
        self.assertEqual(self.rows(True)[0]["mounted"], False)
        self.assertNotIn("Mount", [method for _, _, method, _ in calls])

    def test_removable_drive_with_system_mount_is_never_powered_off(self):
        self.data["drives"]["/drives/data"].update(Removable=True, CanPowerOff=True)
        self.data["blocks"]["/blocks/sdb1"]["Filesystem"]["MountPoints"] = ["/boot"]
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": False, "err": "busy"})
        self.assertEqual(calls, [])

    def test_nested_removable_mount_unmounts_before_its_parent(self):
        self.data["drives"]["/drives/data"].update(Removable=True, CanPowerOff=True)
        nested = copy.deepcopy(self.data["blocks"]["/blocks/sdb2"])
        nested["Block"]["Device"] = "/dev/sdb3"
        nested["Filesystem"]["MountPoints"] = ["/mnt/data/nested"]
        self.data["blocks"]["/blocks/sdb3"] = nested
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": True})
        self.assertEqual([(path, method) for path, _, method, _ in calls], [("/blocks/sdb3", "Unmount"), ("/blocks/sdb2", "Unmount"), ("/drives/data", "PowerOff")])

    def test_unlocked_usb_volume_inherits_backing_drive_and_locks_before_poweroff(self):
        self.data["drives"]["/drives/data"].update(ConnectionBus="usb", CanPowerOff=True)
        backing = self.data["blocks"]["/blocks/sdb2"]
        cleartext = copy.deepcopy(backing)
        cleartext["Block"].update(Device="/dev/mapper/vault", Drive="/", CryptoBackingDevice="/blocks/sdb2")
        cleartext["Filesystem"]["MountPoints"] = ["/mnt/data", "/mnt/data-alias"]
        self.data["blocks"]["/blocks/vault"] = cleartext
        backing["Block"].update(IdUsage="crypto", IdType="crypto_LUKS")
        backing["Encrypted"] = {"CleartextDevice": "/blocks/vault"}
        del backing["Filesystem"]
        rows = self.rows()
        self.assertEqual([row["device"] for row in rows], ["/dev/mapper/vault"])
        self.assertTrue(rows[0]["removable"])
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": True})
        self.assertEqual(cleartext["Filesystem"]["MountPoints"], [])
        self.assertEqual([(path, method) for path, _, method, _ in calls], [("/blocks/vault", "Unmount"), ("/blocks/vault", "Unmount"), ("/blocks/sdb2", "Lock"), ("/drives/data", "PowerOff")])

    def test_busy_or_denied_unmount_never_powers_off(self):
        self.data["drives"]["/drives/data"].update(Removable=True, CanPowerOff=True)
        for error, expected in (("DeviceBusy", "busy"), ("NotAuthorized", "denied")):
            calls.clear()
            errors[("/blocks/sdb2", "Unmount")] = error
            self.assertEqual(self.command("release", "/mnt/data"), {"ok": False, "err": expected})
            self.assertEqual([method for _, _, method, _ in calls], ["Unmount"])

    def test_ejectable_media_ejects_without_poweroff(self):
        self.data["drives"]["/drives/data"].update(MediaRemovable=True, Ejectable=True, CanPowerOff=False)
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": True})
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount", "Eject"])

    def test_unsupported_poweroff_keeps_successful_unmount(self):
        self.data["drives"]["/drives/data"].update(Removable=True, Ejectable=False, CanPowerOff=False)
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": True})
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount"])

    def test_unmount_race_and_mount_failure_keep_machine_errors(self):
        def unmounted_elsewhere():
            self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"] = []
            return "NotMounted"
        errors[("/blocks/sdb2", "Unmount")] = unmounted_elsewhere
        self.assertEqual(self.command("release", "/mnt/data"), {"ok": True})
        self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"] = []
        errors[("/blocks/sdb2", "Mount")] = "NotAuthorized"
        self.assertEqual(self.command("mount", "/dev/sdb2"), {"ok": False, "err": "denied"})

    def test_mount_race_reads_external_mount_instead_of_reporting_busy(self):
        block = self.data["blocks"]["/blocks/sdb2"]
        block["Filesystem"]["MountPoints"] = []
        def mounted_elsewhere():
            block["Filesystem"]["MountPoints"] = ["/mnt/external"]
            return "AlreadyMounted"
        errors[("/blocks/sdb2", "Mount")] = mounted_elsewhere
        self.assertEqual(self.command("mount", "/dev/sdb2"), {"ok": True, "path": "/mnt/external"})
        self.assertEqual([method for _, _, method, _ in calls], ["Mount"])

    def test_invalid_arguments_never_read_daemon(self):
        def forbidden():
            self.fail("invalid arguments read the daemon")
        helper.manager = forbidden
        for args in (("volumes", "bad"), ("mount",), ("release", "a", "b")):
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(helper.main(["flea-udisks", *args]), 2)

    def test_mount_uses_listed_filesystem_device_and_is_idempotent(self):
        self.assertEqual(self.command("mount", "/dev/sdb2"), {"ok": True, "path": "/mnt/data"})
        self.assertEqual(calls, [])
        self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"] = []
        self.assertEqual(self.command("mount", "/dev/sdb2"), {"ok": True, "path": "/run/media/user/Data"})
        self.assertEqual([(path, method) for path, _, method, _ in calls], [("/blocks/sdb2", "Mount")])
        self.assertEqual(self.command("mount", "/dev/sdb1"), {"ok": False, "err": "notListed"})

    def test_daemon_failure_is_an_error_not_an_empty_listing(self):
        def unavailable():
            raise helper.GLib.Error("fixture unavailable")
        helper.manager = unavailable
        for args in (("volumes",), ("mount", "/dev/sdb2"), ("release", "/mnt/data")):
            self.assertEqual(self.command(*args), {"ok": False, "err": "error"})

    def test_fixed_release_does_not_report_success_with_target_still_mounted(self):
        filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
        filesystem["MountPoints"] = ["/mnt/data", "/mnt/data-alias"]
        self.assertEqual(self.command("release", "/mnt/data-alias/file"), {"ok": True})
        self.assertEqual(filesystem["MountPoints"], [])
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount", "Unmount"])

    def test_removable_release_does_not_need_second_attempt_for_idle_alias(self):
        self.data["drives"]["/drives/data"].update(ConnectionBus="usb", CanPowerOff=True)
        filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
        filesystem["MountPoints"] = ["/mnt/data", "/mnt/data-alias"]
        self.assertEqual(self.command("release", "/mnt/data-alias/file"), {"ok": True})
        self.assertEqual(filesystem["MountPoints"], [])
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount", "Unmount", "PowerOff"])

    def test_visible_data_mount_survives_hidden_alias(self):
        filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
        for points in (["/mnt/data", "/mnt/.mirror"], ["/mnt/.mirror", "/mnt/data"]):
            for switch in (False, True):
                with self.subTest(points=points, switch=switch):
                    filesystem["MountPoints"] = points[:]
                    rows = self.rows(switch)
                    self.assertEqual([row["device"] for row in rows], ["/dev/sdb2"])
                    self.assertEqual(rows[0]["path"], "/mnt/data")
                    self.assertEqual(self.command("mount", "/dev/sdb2"), {"ok": True, "path": "/mnt/data"})
                    self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": True})
                    self.assertEqual(filesystem["MountPoints"], [])

    def test_unmount_without_progress_stops_before_poweroff(self):
        original = Interface.call_sync
        def no_progress(proxy, method, options, *args):
            if method == "Unmount":
                calls.append((proxy.path, proxy.name, method, options.unpack()))
                return Variant("()", ())
            return original(proxy, method, options, *args)
        self.data["drives"]["/drives/data"].update(ConnectionBus="usb", CanPowerOff=True)
        Interface.call_sync = no_progress
        try:
            self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": False, "err": "busy"})
            self.assertEqual([method for _, _, method, _ in calls], ["Unmount"])
            self.assertEqual(self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"], ["/mnt/data"])
        finally:
            Interface.call_sync = original

    def test_unmount_attempt_limit_refuses_partial_release(self):
        for removable in (False, True):
            calls.clear()
            self.data["drives"]["/drives/data"].update(Removable=removable, CanPowerOff=True)
            filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
            filesystem["MountPoints"] = ["/mnt/data"] + ["/mnt/alias-%d" % index for index in range(500)]
            self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": False, "err": "busy"})
            self.assertTrue(filesystem["MountPoints"])
            self.assertEqual(len(calls), 32)
            self.assertEqual(set(method for _, _, method, _ in calls), {"Unmount"})

    def test_release_drains_three_and_thirty_two_aliases(self):
        for removable in (False, True):
            for count in (3, 32):
                with self.subTest(removable=removable, count=count):
                    calls.clear()
                    self.data["drives"]["/drives/data"].update(Removable=removable, CanPowerOff=True)
                    filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
                    filesystem["MountPoints"] = ["/mnt/data"] + ["/mnt/alias-%d" % index for index in range(count - 1)]
                    self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": True})
                    self.assertEqual(filesystem["MountPoints"], [])
                    self.assertEqual([method for _, _, method, _ in calls], ["Unmount"] * count + (["PowerOff"] if removable else []))

    def test_late_sibling_remount_returns_busy_before_lock_or_poweroff(self):
        self.data["drives"]["/drives/data"].update(Removable=True, CanPowerOff=True)
        original = helper.blocks
        reads = []
        def remount_at_final_check():
            reads.append(len(reads) + 1)
            if len(reads) == 4:
                self.data["blocks"]["/blocks/sdb1"]["Filesystem"]["MountPoints"] = ["/mnt/raced"]
            elif len(reads) == 5:
                self.data["blocks"]["/blocks/sdb1"]["Filesystem"]["MountPoints"] = []
            return original()
        helper.blocks = remount_at_final_check
        self.addCleanup(setattr, helper, "blocks", original)
        self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": False, "err": "busy"})
        self.assertEqual(len(reads), 4)
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount"])

    def test_hidden_alias_does_not_block_removable_release(self):
        self.data["drives"]["/drives/data"].update(ConnectionBus="usb", CanPowerOff=True)
        filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
        filesystem["MountPoints"] = ["/mnt/.mirror", "/mnt/data"]
        self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": True})
        self.assertEqual(filesystem["MountPoints"], [])
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount", "Unmount", "PowerOff"])

    def test_visible_alias_does_not_make_system_volume_releasable(self):
        filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
        for protected in ("/", "/boot"):
            filesystem["MountPoints"] = [protected, "/mnt/data"]
            for switch in (False, True):
                self.assertEqual(self.rows(switch), [])
            self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": False, "err": "notListed"})
        self.assertEqual(calls, [])

    def test_only_hidden_mounts_stay_hidden_under_both_settings(self):
        self.data["blocks"]["/blocks/sdb2"]["Filesystem"]["MountPoints"] = ["/mnt/.mirror"]
        self.assertEqual(self.rows(False), [])
        self.assertEqual(self.rows(True), [])
        self.assertEqual(self.command("release", "/mnt/.mirror/file"), {"ok": False, "err": "notListed"})
        self.assertEqual(calls, [])

    def test_notmounted_without_fresh_empty_mountpoints_is_not_success(self):
        errors[("/blocks/sdb2", "Unmount")] = "NotMounted"
        self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": False, "err": "busy"})
        self.assertEqual([method for _, _, method, _ in calls], ["Unmount"])

    def test_sol_gap64_post_unmount_refresh_races_never_lock_or_poweroff(self):
        for race in ("daemon", "replacement", "protected"):
            with self.subTest(race=race):
                self.setUp()
                self.data["drives"]["/drives/data"].update(Removable=True, CanPowerOff=True)
                filesystem = self.data["blocks"]["/blocks/sdb2"]["Filesystem"]
                if race == "protected":
                    filesystem["MountPoints"].append("/mnt/alias")
                original_call = Interface.call_sync
                original_blocks = helper.blocks

                def intercept(proxy, method, options, *args):
                    result = original_call(proxy, method, options, *args)
                    if method == "Unmount":
                        if race == "daemon":
                            def unavailable():
                                raise helper.GLib.Error("daemon disappeared after Unmount")
                            helper.blocks = unavailable
                        elif race == "replacement":
                            self.data["blocks"]["/blocks/sdb2"]["Block"]["Device"] = "/dev/replaced"
                        else:
                            filesystem["MountPoints"] = ["/boot"]
                    return result

                Interface.call_sync = intercept
                try:
                    expected = "busy" if race == "protected" else "error"
                    self.assertEqual(self.command("release", "/mnt/data/file"), {"ok": False, "err": expected})
                    self.assertEqual([method for _, _, method, _ in calls], ["Unmount"])
                finally:
                    Interface.call_sync = original_call
                    helper.blocks = original_blocks


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Volumes))
    print("VOLUME_UDISKS %s tests=%d" % ("PASS" if result.wasSuccessful() else "FAIL", result.testsRun))
    sys.exit(0 if result.wasSuccessful() else 1)
