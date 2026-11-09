//! FIX2: gio-lent kernel rows release like the frozen oracle, and other gvfs schemes stay refused.
use super::{
    menus,
    mount::{Effect, Mounts},
};
use flea::ui_logic_device_rail::menu::{rail_menu, RailAction};
use flea::ui_logic_network_shares::rail;

const GIO: &str = "Drive(0): fixture\n  Type: GProxyDrive (GProxyVolumeMonitorGoa)\nMount(0): photos -> file:///mnt/kmount-photos\n  Type: GDaemonMount\nMount(1): iso-images -> file:///mnt/kmount-iso\n  Type: GDaemonMount\nMount(2): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n  Type: GDaemonMount\nMount(3): gdrive -> file:///fixture/home/gdrive\n  Type: GDaemonMount\nMount(4): vol on host -> afp://host/vol/\n  Type: GDaemonMount\nMount(5): spaced -> file:///mnt/kmount%20spaced\n  Type: GDaemonMount\n";
const MINFO: &str = "30 1 0:45 / /mnt/kmount-photos rw - nfs4 nas:/exports/photos rw\n31 1 0:46 / /mnt/kmount-iso rw - cifs //nas/isos rw\n35 1 0:50 / /fixture/home/gdrive rw - fuse.rclone remote: rw\n36 1 0:51 / /mnt/kmount-rclone rw - fuse.rclone remote: rw\n37 1 0:52 / /mnt/kmount\\040spaced rw - cifs //nas/spaced rw\n";

fn row(path_or_uri: &str) -> flea::ui_logic_device_rail::RailEntry {
    rail::rebuild(GIO, &[], MINFO)
        .into_iter()
        .find(|e| e.path == path_or_uri || e.uri == path_or_uri)
        .expect("fixture row")
}
fn argv(path_or_uri: &str) -> Option<Vec<String>> {
    menus::unmount(&row(path_or_uri), GIO).map(|j| j.argv)
}
fn strings(items: &[&str]) -> Option<Vec<String>> {
    Some(items.iter().map(|s| s.to_string()).collect())
}

#[test]
fn fix2_control_unlent_fuse_row_still_unmounts() {
    assert_eq!(
        argv("/mnt/kmount-rclone"),
        strings(&["fusermount3", "-u", "--", "/mnt/kmount-rclone"])
    );
}

#[test]
fn fix2_gio_lent_cifs_kernel_row_unmounts_like_the_oracle() {
    let e = row("/mnt/kmount-iso");
    assert_eq!(e.uri, "file:///mnt/kmount-iso");
    assert!(rail_menu(&e)
        .iter()
        .any(|r| r.action() == Some(RailAction::Unmount)));
    assert_eq!(
        argv("/mnt/kmount-iso"),
        strings(&["gio", "mount", "-u", "--", "file:///mnt/kmount-iso"])
    );
}

#[test]
fn fix2_gio_lent_nfs_and_encoded_rows_unmount_by_their_listed_uri() {
    assert_eq!(
        argv("/mnt/kmount-photos"),
        strings(&["gio", "mount", "-u", "--", "file:///mnt/kmount-photos"])
    );
    assert_eq!(
        argv("/mnt/kmount spaced"),
        strings(&["gio", "mount", "-u", "--", "file:///mnt/kmount%20spaced"])
    );
}

#[test]
fn fix2_gio_lent_fuse_row_unmounts_by_path() {
    let e = row("/fixture/home/gdrive");
    assert_eq!(e.uri, "file:///fixture/home/gdrive");
    assert_eq!(
        argv("/fixture/home/gdrive"),
        strings(&["fusermount3", "-u", "--", "/fixture/home/gdrive"])
    );
}

#[test]
fn fix2_a_file_uri_that_does_not_name_the_row_path_is_refused() {
    let mut e = row("/mnt/kmount-iso");
    for bad in [
        "file:///mnt/other",
        "file://user:secret@host/mnt/kmount-iso",
        "file:///mnt/kmount-iso/../x",
        "file:///mnt/kmount-iso%0A",
    ] {
        e.uri = bad.into();
        assert!(menus::unmount(&e, GIO).is_none(), "{bad}");
    }
    e.uri = "file:///mnt/kmount-iso/".into();
    assert!(menus::unmount(&e, GIO).is_some());
    e.path = "/".into();
    e.uri = "file:///".into();
    assert!(menus::unmount(&e, GIO).is_none(), "root is never released");
}

#[test]
fn fix2_a_fuse_row_needs_a_valid_mount_path() {
    let mut e = row("/fixture/home/gdrive");
    e.path = "relative".into();
    assert!(menus::unmount(&e, GIO).is_none());
    e.path = "/".into();
    assert!(menus::unmount(&e, GIO).is_none());
}

#[test]
fn fix2_mounted_gvfs_rows_of_other_schemes_keep_the_refusal() {
    let e = row("afp://host/vol/");
    assert!(menus::unmount(&e, GIO).is_none());
    let menus::Open::Remote(request) = menus::activate(&e, 7) else {
        panic!("remote row")
    };
    let effects = Mounts::default().open_share(request, 0);
    assert!(
        matches!(effects.as_slice(), [Effect::Say { note, .. }] if note.key == "network.invalid_address"),
        "{effects:?}"
    );
}
