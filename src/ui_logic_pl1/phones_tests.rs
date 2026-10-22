//! Phone and camera rows from gio mount -li, the rail menu each row offers, and where a release routes.
use super::menu::{rail_menu, MenuRow, RailAction};
use super::mounts::{parse_mounts, rail_key, same_entries, trashable};
use super::phones::parse_phones;
use super::rail::{route, Route};
use super::{Group, Kind, RailEntry};

const IDLE: &str = r#"Volume(0): SAMSUNG Android
  Type: GProxyVolume (GProxyVolumeMonitorMTP)
  ids:
   unix-device: '/dev/bus/usb/001/013'
  activation_root=mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/
  themed icons:  [multimedia-player]
  can_mount=1
  can_eject=0
  should_automount=1
"#;

const SAMSUNG_KEY: &str = "mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/";

/// The idle phone with its volume mounted: the indented Mount() and a top-level shadow mount.
fn live() -> String {
    [
        IDLE,
        r#"  Mount(0): SAMSUNG Android -> mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/
    Type: GProxyShadowMount (GProxyVolumeMonitorMTP)
    can_unmount=1
Mount(1): mtp -> mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/
  Type: GDaemonMount
  is_shadowed=1
"#,
    ]
    .concat()
}

/// The iPhone's two monitors: GPhoto2 for the camera store, AFC for the files (captured on iOS 26.6.2).
fn iphone() -> String {
    [
        r#"Volume(0): iPhone
  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)
  ids:
   unix-device: '/dev/bus/usb/001/005'
  activation_root=gphoto2://Apple_Inc._iPhone_00008130001641411883401C/
  can_mount=1
  can_eject=0
  should_automount=1
"#,
        r#"Volume(1): Documents on GM’s iPhone
  Type: GProxyVolume (GProxyVolumeMonitorAfc)
  ids:
   uuid: '00008130-001641411883401C'
  uuid=00008130-001641411883401C
  activation_root=afc://00008130-001641411883401C:3/
  can_mount=1
  can_eject=0
  should_automount=1
"#,
    ]
    .concat()
}

/// The menu's action ids in order with separators dropped, joined by commas as the JS checks did.
fn action_ids(entry: &RailEntry) -> String {
    rail_menu(entry).iter().filter_map(MenuRow::action).map(RailAction::id).collect::<Vec<_>>().join(",")
}

#[test]
#[rustfmt::skip]
fn box_without_a_phone_parses_to_no_rows() {
    let unplugged = "Drive(0): CT2000T700SSD3\n  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n";
    assert_eq!(parse_phones(unplugged).len(), 0, "a box with no phone parses to nothing");
    assert_eq!(parse_phones("").len(), 0, "empty gio output parses to nothing");
    assert_eq!(parse_phones("not gio output at all\n").len(), 0, "garbage gio output parses to nothing");
}

#[test]
#[rustfmt::skip]
fn plugged_phone_is_one_unmounted_row() {
    let rows = parse_phones(IDLE);
    assert_eq!(rows.len(), 1, "a plugged phone is one row");
    assert_eq!(rows[0].label, "SAMSUNG Android", "the row keeps the volume's own label");
    assert_eq!(rows[0].uri, SAMSUNG_KEY, "the row's uri is the activation root");
    assert!(!rows[0].mounted, "an unmounted phone reads as unmounted");
    assert_eq!((rows[0].group, rows[0].kind), (Group::Device, Kind::Phone), "a phone rides the DEVICES group");
    assert_eq!(rows[0].path, "", "the path stays empty until the open resolves it");
    assert_eq!(rows[0].size, None, "the row carries no size rather than none");
}

#[test]
#[rustfmt::skip]
fn mounted_phone_stays_one_row_and_offers_unmount() {
    let mounted = parse_phones(&live());
    assert_eq!(mounted.len(), 1, "the mounted phone is still one row");
    // gvfs prints can_mount=0 for a volume it has already mounted, so the row must survive that.
    let mounted_no_remount = "Volume(0): SAMSUNG Android\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n  can_mount=0\n  can_eject=0\n  Mount(0): SAMSUNG Android -> mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n";
    let live0 = parse_phones(mounted_no_remount);
    assert_eq!(live0.len(), 1, "a mounted volume that can no longer be mounted is still a row");
    let live0_state = live0.first().map_or("no row".to_string(), |r| format!("{}|{}", r.mounted, action_ids(r)));
    assert_eq!(live0_state, "true|open,unmountPhone", "and it reads as mounted, so the rail offers its Unmount");
    assert!(mounted[0].mounted, "the indented Mount() flips it to mounted");
    assert_eq!(parse_mounts(&live()).len(), 0, "the shadow mount is not a network row either");
}

#[test]
#[rustfmt::skip]
fn udisks_volumes_are_not_phones() {
    let stick = "Drive(4): WD_BLACK SN850X 4000GB\n  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n  Volume(0): STEAM\n    Type: GProxyVolume (GProxyVolumeMonitorUDisks2)\n    can_mount=0\n    Mount(0): STEAM -> file:///run/media/gm/STEAM\n";
    assert_eq!(parse_phones(stick).len(), 0, "a udisks volume under its drive is not a phone");
    let hoisted = "Volume(0): STEAM\n  Type: GProxyVolume (GProxyVolumeMonitorUDisks2)\n  activation_root=file:///run/media/gm/STEAM\n  can_mount=1\n";
    assert_eq!(parse_phones(hoisted).len(), 0, "a top-level udisks volume is still the wrong monitor");
}

#[test]
#[rustfmt::skip]
fn gphoto2_camera_is_a_row_but_not_a_network_share() {
    let camera = "Volume(0): Canon Digital Camera\n  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n  activation_root=gphoto2://%5Busb%3A001%2C010%5D/\n  can_mount=1\n";
    let cam = parse_phones(camera);
    assert_eq!(cam.len(), 1, "a gphoto2 camera is a row too");
    assert_eq!(cam[0].uri, "gphoto2://%5Busb%3A001%2C010%5D/", "its uri is the gphoto2 activation root");
    assert_eq!(parse_mounts("Mount(0): camera -> gphoto2://%5Busb%3A001%2C010%5D/\n").len(), 0, "a gphoto2 mount is not a network row");
}

#[test]
#[rustfmt::skip]
fn refusing_or_rootless_volume_is_no_row() {
    let refusing = "Volume(0): SAMSUNG Android\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/\n  can_mount=0\n";
    assert_eq!(parse_phones(refusing).len(), 0, "can_mount=0 is not a row");
    let rootless = "Volume(0): SAMSUNG Android\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  can_mount=1\n";
    assert_eq!(parse_phones(rootless).len(), 0, "no activation root is not a row");
}

#[test]
#[rustfmt::skip]
fn two_phones_are_two_rows_with_their_own_mounts() {
    let two = [IDLE, "Volume(1): Pixel 7\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://Google_Pixel_7_1A2B/\n  can_mount=1\n  Mount(0): Pixel 7 -> mtp://Google_Pixel_7_1A2B/\n"].concat();
    let pair = parse_phones(&two);
    assert_eq!(pair.len(), 2, "two phones are two rows");
    assert!(!pair[0].mounted, "the first stays unmounted");
    assert_eq!(format!("{}|{}", pair[1].mounted, pair[1].uri), "true|mtp://Google_Pixel_7_1A2B/", "the second's mount is its own");
}

#[test]
#[rustfmt::skip]
fn phone_menu_and_key_follow_the_shared_rail_rules() {
    let mounted = parse_phones(&live());
    let rows = parse_phones(IDLE);
    assert_eq!(action_ids(&mounted[0]), "open,unmountPhone", "a mounted phone offers its open and its release, never an eject");
    assert_eq!(action_ids(&rows[0]), "mountPhone", "an unmounted phone offers the mount its own row does");
    assert_eq!(rail_key(&mounted[0]), SAMSUNG_KEY, "a phone's key is its uri");
    // rowMenu was railMenu under a second name, so the one rail_menu answers both.
    assert_eq!(action_ids(&mounted[0]), "open,unmountPhone", "rowMenu adds no share rows to a phone");
}

#[test]
#[rustfmt::skip]
fn release_routes_by_key_to_the_phone_or_the_row_found() {
    let mounted = parse_phones(&live());
    let phones = [mounted[0].clone()];
    let pixel_key = "mtp://Google_Pixel_7_1A2B/";
    assert_eq!(route(RailAction::UnmountPhone, SAMSUNG_KEY, &phones, &[]), Route::ReleasePhone(SAMSUNG_KEY.to_string()), "unmountPhone hands the key to the sidebar");
    assert_eq!(route(RailAction::MountPhone, pixel_key, &phones, &[]), Route::OpenPhone(pixel_key.to_string()), "mountPhone hands its own key to the same row's activation");
    assert_eq!(route(RailAction::Open, pixel_key, &phones, &[]), Route::OpenPhone(pixel_key.to_string()), "Open runs the row's own activation too");
    let volume = RailEntry { group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), ..RailEntry::default() };
    let share = RailEntry { group: Group::Network, kind: Kind::Share, uri: "smb://nas/isos/".into(), ..RailEntry::default() };
    assert_eq!(route(RailAction::Open, "/dev/sda1", std::slice::from_ref(&volume), std::slice::from_ref(&share)), Route::Activate(0), "Open on a volume reaches devices.activate");
    // The navigation pane is the rail host's to pass with OpenShare at the join.
    assert_eq!(route(RailAction::Open, "smb://nas/isos/", std::slice::from_ref(&volume), std::slice::from_ref(&share)), Route::OpenShare(0), "Open on a share reaches mounts.activate with the navigation pane");
}

#[test]
#[rustfmt::skip]
fn unchanged_poll_compares_equal_and_a_flip_does_not() {
    let rows = parse_phones(IDLE);
    let mounted = parse_phones(&live());
    assert!(same_entries(&rows, &parse_phones(IDLE)), "an unchanged poll compares equal");
    assert!(!(same_entries(&rows, &mounted)), "a mount-state flip does not");
}

#[test]
#[rustfmt::skip]
fn glyph_names_the_transport_not_the_brand() {
    assert_eq!(parse_phones(IDLE)[0].glyph, "smartphone", "an MTP volume draws the phone");
    let lens = "Volume(0): Canon EOS R6\n  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n  activation_root=gphoto2://usb%3A001%2C014/\n  can_mount=1\n";
    let lens_rows = parse_phones(lens);
    assert_eq!(format!("{}|{}", lens_rows.len(), lens_rows[0].glyph), "1|camera", "a GPhoto2 volume draws the camera");
    assert_eq!((lens_rows[0].group, lens_rows[0].kind, lens_rows[0].uri.as_str()), (Group::Device, Kind::Phone, "gphoto2://usb%3A001%2C014/"), "and it is a phone row in every other respect");
}

#[test]
#[rustfmt::skip]
fn gvfs_mount_folders_cannot_trash() {
    let cases = [
        ("/run/user/1000/gvfs/mtp:host=SAMSUNG_RQGL705T0NR/DCIM", "an MTP folder cannot trash"),
        ("/run/user/1000/gvfs/gphoto2:host=usb%3A001%2C014/store", "a PTP folder cannot trash"),
        ("/run/user/1000/gvfs/smb-share:server=192.168.21.25,share=data", "a share folder cannot trash either"),
        ("/run/user/1000/gvfs/sftp:host=box,user=gm/home/gm", "nor an sftp one"),
        ("/run/user/1000/gvfs/afc:host=00008130-001641411883401C/DCIM", "nor the iPhone's AFC files"),
        ("/run/user/1000/gvfs/dav:host=slot,ssl=true/notes", "nor WebDAV"),
    ];
    for (path, name) in cases {
        assert!(!trashable(path, None), "{name}");
    }
}

#[test]
#[rustfmt::skip]
fn ordinary_folders_still_trash() {
    let cases = [
        ("/home/gm/Downloads", "every ordinary path still can"),
        ("/home/gm/src/gvfs/daemon", "a checkout of gvfs itself included"),
        ("/home/u/mirror/gvfs/smb-share:server=h,share=d/a.txt", "a plain folder named gvfs holding a scheme-shaped folder still trashes"),
        ("/run/user/1000/home/gvfs/mtp:host=X/DCIM", "a gvfs folder inside the run folder is no mount root"),
    ];
    for (path, name) in cases {
        assert!(trashable(path, None), "{name}");
    }
}

#[test]
#[rustfmt::skip]
fn iphone_on_two_monitors_is_one_phone_row() {
    let rows = parse_phones(&iphone());
    assert_eq!(rows.len(), 1, "an iPhone on two monitors is one row");
    assert_eq!(rows[0].glyph, "smartphone", "and it is the phone, not the camera store");
    assert_eq!(rows[0].uri, "afc://00008130-001641411883401C/", "the row's uri is the AFC root, never the documents volume");
    assert_eq!(rows[0].label, "GM’s iPhone", "the label is the phone's own, with the documents share taken off the front");
    assert!(!rows[0].mounted, "an unmounted iPhone reads as unmounted");
    assert_eq!((rows[0].group, rows[0].kind), (Group::Device, Kind::Phone), "and it rides DEVICES like any other phone");
}

#[test]
#[rustfmt::skip]
fn mounted_iphone_is_marked_by_its_root_mount_even_when_not_remountable() {
    let live = [&iphone(), "Mount(2): GM’s iPhone -> afc://00008130-001641411883401C/\n", "  Type: GDaemonMount\n"].concat();
    let live_rows = parse_phones(&live);
    assert_eq!(live_rows.len(), 1, "a mounted iPhone is still one row");
    assert!(live_rows[0].mounted, "the root mount is what says it is mounted");
    // gvfs answers can_mount=0 once the root is mounted, so the mount, not can_mount, must keep the row.
    let no_remount = iphone().replace("  activation_root=afc://00008130-001641411883401C:3/\n  can_mount=1\n", "  activation_root=afc://00008130-001641411883401C:3/\n  can_mount=0\n") + "Mount(2): GM’s iPhone -> afc://00008130-001641411883401C/\n";
    let kept = parse_phones(&no_remount);
    assert_eq!(kept.len(), 1, "a mounted iPhone gvfs will not remount is still a row");
    assert!(kept[0].mounted, "and it reads as mounted");
}

#[test]
#[rustfmt::skip]
fn documents_volume_mount_is_not_the_phones_mount() {
    let docs_only = iphone().replace("  activation_root=afc://00008130-001641411883401C:3/\n", "  activation_root=afc://00008130-001641411883401C:3/\n  Mount(1): Documents on GM’s iPhone -> afc://00008130-001641411883401C:3/\n");
    let docs = parse_phones(&docs_only);
    assert_eq!(docs.len(), 1, "the documents volume's own mount is not the phone's");
    assert!(!docs[0].mounted, "so the row still offers its mount rather than an unmount");
}

#[test]
#[rustfmt::skip]
fn afc_without_uuid_leaves_the_camera_leg_not_the_share() {
    let no_uuid = iphone().replace("  uuid=00008130-001641411883401C\n", "");
    let rows = parse_phones(&no_uuid);
    assert_eq!(rows.len(), 1, "an AFC volume with no uuid is not a row");
    assert_eq!(format!("{}|{}", rows[0].glyph, rows[0].uri), "camera|gphoto2://Apple_Inc._iPhone_00008130001641411883401C/", "and what is left is the camera leg, not the documents share");
}

#[test]
#[rustfmt::skip]
fn automounted_documents_share_does_not_decide_the_phone_row() {
    let automounted = iphone().replace("  activation_root=afc://00008130-001641411883401C:3/\n  can_mount=1\n", "  activation_root=afc://00008130-001641411883401C:3/\n  can_mount=0\n  Mount(1): Documents on GM’s iPhone -> afc://00008130-001641411883401C:3/\n");
    let rows = parse_phones(&automounted);
    assert_eq!(rows[0].uri, "afc://00008130-001641411883401C/", "an automounted documents share does not decide the phone's row");
    assert_eq!(rows[0].glyph, "smartphone", "the row is still the phone");
    assert!(!rows[0].mounted, "and it still offers its mount rather than an unmount");
}

#[test]
#[rustfmt::skip]
fn two_afc_volumes_on_one_uuid_are_one_row() {
    let twice = [&iphone(), "Volume(2): Documents on GM’s iPhone\n  Type: GProxyVolume (GProxyVolumeMonitorAfc)\n  uuid=00008130-001641411883401C\n  activation_root=afc://00008130-001641411883401C:4/\n  can_mount=1\n"].concat();
    assert_eq!(parse_phones(&twice).len(), 1, "two AFC volumes on one uuid are one row");
}

#[test]
#[rustfmt::skip]
fn camera_beside_iphone_keeps_its_own_row() {
    let both = [&iphone(), "Volume(2): Canon EOS R6\n  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n  activation_root=gphoto2://usb%3A001%2C014/\n  can_mount=1\n"].concat();
    let rows = parse_phones(&both);
    assert_eq!(rows.len(), 2, "a camera plugged in beside the iPhone is its own row");
    assert_eq!(format!("{}|{}", rows[1].glyph, rows[1].label), "camera|Canon EOS R6", "and it is still the camera");
}

#[test]
#[rustfmt::skip]
fn iphone_without_afc_is_one_camera_row() {
    let ptp_only = "Volume(0): iPhone\n  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n  activation_root=gphoto2://Apple_Inc._iPhone_00008130001641411883401C/\n  can_mount=1\n";
    let rows = parse_phones(ptp_only);
    assert_eq!(format!("{}|{}", rows.len(), rows[0].glyph), "1|camera", "without gvfs-afc the iPhone is one camera row");
}

#[test]
#[rustfmt::skip]
fn iphone_root_mount_is_not_a_network_share() {
    let shares = parse_mounts("Mount(2): GM’s iPhone -> afc://00008130-001641411883401C/\nMount(3): isos on nas -> smb://nas/isos/\n");
    assert_eq!(shares.len(), 1, "the iPhone's mount is not a network share");
    assert_eq!(shares[0].uri, "smb://nas/isos/", "and the real share beside it still is");
}
