// Sample sysfs trees in a test dir: each disk a durable copy must confirm, beside the internal disks it must leave on the cache path.
use super::*;
use crate::backend::testdir::TestDir;

const USB_PART: &str =
    "devices/pci0000:00/0000:00:14.0/usb2/2-1/2-1:1.0/host0/target0:0:0/0:0:0:0/block/sdb/sdb1";
const SD_PART: &str = "devices/pci0000:00/0000:00:1c.0/0000:02:00.0/rtsx_pci_sdmmc.0/mmc_host/mmc0/mmc0:aaaa/block/mmcblk0/mmcblk0p1";
const TB_PCI: &str = "devices/pci0000:00/0000:00:07.0/0000:2c:00.0/0000:2d:01.0/0000:2e:00.0";
const NVME_PART: &str = "devices/pci0000:00/0000:00:1b.0/0000:01:00.0/nvme/nvme0/nvme0n1/nvme0n1p2";

struct Sys {
    d: TestDir,
}

impl Sys {
    fn new(tag: &str) -> Sys {
        let d = TestDir::new(tag);
        d.dir("sys/dev/block");
        d.dir("sys/class/block");
        d.dir("dev/mapper");
        Sys { d }
    }

    fn sys(&self) -> PathBuf {
        self.d.join("sys")
    }

    fn dev(&self) -> PathBuf {
        self.d.join("dev")
    }

    // A partition directory with its disk's removable file, linked from /sys/dev/block and /sys/class/block as the kernel links it.
    fn partition(&self, rel: &str, majmin: &str, disk_removable: &str) {
        let part = self.d.dir(&format!("sys/{rel}"));
        std::fs::write(part.join("partition"), "1\n").unwrap();
        std::fs::write(
            part.parent().unwrap().join("removable"),
            format!("{disk_removable}\n"),
        )
        .unwrap();
        self.link(rel, majmin);
    }

    fn link(&self, rel: &str, majmin: &str) {
        let name = rel.rsplit('/').next().unwrap();
        std::os::unix::fs::symlink(
            format!("../../{rel}"),
            self.d.join(&format!("sys/dev/block/{majmin}")),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            format!("../../{rel}"),
            self.d.join(&format!("sys/class/block/{name}")),
        )
        .unwrap();
        std::fs::write(self.d.join(&format!("dev/{name}")), "").unwrap();
    }

    fn entry(majmin: &str, source: &str) -> MountEntry {
        MountEntry {
            mount: PathBuf::from("/run/media/gm/STICK"),
            fstype: "ext4".into(),
            majmin: majmin.into(),
            source: source.into(),
        }
    }

    fn removable(&self, majmin: &str, source: &str) -> bool {
        removable_under(&self.sys(), &self.dev(), &Sys::entry(majmin, source))
    }

    // The question dest_is_durable asks of the same ext4 mount, so the copy path itself is pinned.
    fn durable(&self, majmin: &str, source: &str) -> bool {
        let entry = Sys::entry(majmin, source);
        crate::backend::durable::entry_is_durable_under(&self.sys(), &self.dev(), &entry)
    }
}

// The read durability had before: the usb class's bus segment and removable file, on the same link the test builds.
fn usb_class_read(rel: &str, disk_removable: &str) -> bool {
    crate::backend::extclass::is_usb(Some(&format!("../../{rel}")), Some(disk_removable))
}

#[test]
fn an_ext4_usb_stick_is_removable_by_its_bus() {
    let s = Sys::new("removable-usb");
    s.partition(USB_PART, "8:17", "0");
    assert!(
        s.removable("8:17", "/dev/sdb1"),
        "a usb disk whose removable file reads 0 still counts by its bus"
    );
}

#[test]
fn a_btrfs_usb_stick_is_found_through_its_source_when_its_number_is_anonymous() {
    let s = Sys::new("removable-btrfs");
    s.partition(USB_PART, "8:17", "1");
    assert!(
        s.removable("0:45", "/dev/sdb1"),
        "btrfs names 0:45, so the source finds the stick"
    );
    let class = crate::backend::extclass::classify_entry_with_magic(
        Path::new("/run/media/gm/STICK"),
        Some(&MountEntry {
            mount: PathBuf::from("/run/media/gm/STICK"),
            fstype: "btrfs".into(),
            majmin: "0:45".into(),
            source: "/dev/sdb1".into(),
        }),
        Some(0x9123683E),
    );
    assert_eq!(
        class, "",
        "the thumbnail class is read on its own and stays as it was"
    );
}

#[test]
fn an_sd_card_in_a_pci_reader_is_removable_and_soldered_emmc_is_not() {
    let s = Sys::new("removable-sd");
    s.partition(SD_PART, "179:1", "0");
    let card = s.sys().join(
        "devices/pci0000:00/0000:00:1c.0/0000:02:00.0/rtsx_pci_sdmmc.0/mmc_host/mmc0/mmc0:aaaa",
    );
    std::os::unix::fs::symlink("../../../mmc0:aaaa", card.join("block/mmcblk0/device")).unwrap();
    std::fs::write(card.join("type"), "SD\n").unwrap();
    assert!(
        !usb_class_read(SD_PART, "0"),
        "the usb class misses an SD card on PCI"
    );
    assert!(
        s.removable("179:1", "/dev/mmcblk0p1"),
        "an SD card counts by its card type"
    );
    assert!(
        s.durable("179:1", "/dev/mmcblk0p1"),
        "a copy onto the ext4 SD card confirms its bytes"
    );
    std::fs::write(card.join("type"), "MMC\n").unwrap();
    assert!(
        !s.removable("179:1", "/dev/mmcblk0p1"),
        "eMMC is soldered on and stays on the cache path"
    );
}

#[test]
fn a_thunderbolt_nvme_drive_is_removable_and_an_internal_nvme_is_not() {
    let s = Sys::new("removable-tb");
    let part = format!("{TB_PCI}/nvme/nvme1/nvme1n1/nvme1n1p1");
    s.partition(&part, "259:5", "0");
    std::fs::write(s.sys().join(TB_PCI).join("removable"), "removable\n").unwrap();
    s.partition(NVME_PART, "259:2", "0");
    assert!(
        !usb_class_read(&part, "0"),
        "the usb class misses a PCIe tunnel"
    );
    assert!(
        s.removable("259:5", "/dev/nvme1n1p1"),
        "the driver core marks a device behind an external port removable"
    );
    assert!(
        s.durable("259:5", "/dev/nvme1n1p1"),
        "a copy onto the ext4 Thunderbolt drive confirms its bytes"
    );
    assert!(
        !s.removable("259:2", "/dev/nvme0n1p2"),
        "an internal NVMe has no removable bus device above it"
    );
    assert!(
        !s.durable("259:2", "/dev/nvme0n1p2"),
        "a copy onto the internal ext4 NVMe keeps the cache path"
    );
    std::fs::write(
        s.sys()
            .join("devices/pci0000:00/0000:00:1b.0/0000:01:00.0/removable"),
        "fixed\n",
    )
    .unwrap();
    assert!(
        !s.removable("259:2", "/dev/nvme0n1p2"),
        "a fixed bus device is not removable"
    );
}

#[test]
fn a_dm_crypt_stick_counts_by_its_member_and_an_encrypted_internal_root_does_not() {
    let s = Sys::new("removable-dm");
    s.partition(USB_PART, "8:17", "0");
    s.partition(NVME_PART, "259:2", "0");
    for (dm, majmin, member) in [("dm-0", "254:0", USB_PART), ("dm-1", "254:1", NVME_PART)] {
        let rel = format!("devices/virtual/block/{dm}");
        s.d.dir(&format!("sys/{rel}/slaves"));
        let name = member.rsplit('/').next().unwrap();
        std::os::unix::fs::symlink(
            format!("../../../../../{member}"),
            s.sys().join(&rel).join("slaves").join(name),
        )
        .unwrap();
        s.link(&rel, majmin);
    }
    std::os::unix::fs::symlink("../dm-0", s.dev().join("mapper/luks-stick")).unwrap();
    std::os::unix::fs::symlink("../dm-1", s.dev().join("mapper/root")).unwrap();
    assert!(
        !usb_class_read("devices/virtual/block/dm-0", "0"),
        "the usb class misses a stick behind dm-crypt"
    );
    assert!(
        s.removable("254:0", "/dev/mapper/luks-stick"),
        "an unlocked stick counts by the partition below it"
    );
    assert!(
        !s.removable("254:1", "/dev/mapper/root"),
        "an encrypted internal disk stays on the cache path"
    );
    assert!(
        s.removable("0:31", "/dev/mapper/luks-stick"),
        "btrfs inside the unlocked stick counts through its source"
    );
    assert!(
        !s.removable("0:29", "/dev/mapper/root"),
        "btrfs on the encrypted internal root does not"
    );
}

#[test]
fn a_mount_with_no_block_device_is_never_removable() {
    let s = Sys::new("removable-none");
    for (majmin, source) in [
        ("0:50", "//nas/share"),
        ("0:27", "tmpfs"),
        ("0:40", "remote:"),
        ("0:41", "/dev/"),
        ("0:42", "/dev/gone"),
    ] {
        assert!(
            !s.removable(majmin, source),
            "{majmin} {source} names no disk"
        );
    }
    assert!(
        !s.removable("8:99", "/dev/sdz9"),
        "a number sysfs does not list answers false"
    );
}
