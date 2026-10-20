//! Port of tests/js/mounts.js, first half: gio listings, bookmarks, rail rows, keys, holding and polls.
use super::menu::{rail_menu, MenuRow, RailAction};
use super::mounts::{holding, non_file_bookmarks, normalize, parse_mounts, rail_key, row_by_key, same_entries};
use super::rail::{action_by_id, route, Route};
use super::{Group, Kind, RailEntry};

/// The action ids a rail entry's menu offers, separators skipped.
fn ids(entry: &RailEntry) -> Vec<&'static str> {
    rail_menu(entry).iter().filter_map(MenuRow::action).map(|a| a.id()).collect()
}

/// The glyph of each action a rail entry's menu offers, separators skipped.
fn glyphs(entry: &RailEntry) -> Vec<&'static str> {
    rail_menu(entry).iter().filter_map(MenuRow::action).map(|a| a.glyph()).collect()
}

#[test]
fn parse_mounts_reads_live_gio_output() {
    let live = concat!(
        "Drive(0): KBG40ZNS256G NVMe KIOXIA 256GB\n",
        "  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n",
        "Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n",
        "  Type: GDaemonMount\n",
    );
    let mounts = parse_mounts(live);
    assert_eq!(mounts.len(), 1, "a Drive() line is not a mount");
    assert_eq!(mounts[0].label, "isos", "the on-host suffix is stripped from the label");
    assert_eq!(mounts[0].uri, "smb://192.168.1.10/isos/", "the target uri survives whole");
}

#[test]
fn parse_mounts_ignores_text_without_mount_lines() {
    let drive_only = concat!(
        "Drive(0): KBG40ZNS256G NVMe KIOXIA 256GB\n",
        "  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n",
    );
    assert_eq!(parse_mounts(drive_only).len(), 0, "no Mount() line means no entries");
    assert_eq!(parse_mounts("").len(), 0, "empty gio output parses to nothing");
    assert_eq!(parse_mounts("not gio output at all\n").len(), 0, "garbage gio output parses to nothing");
}

#[test]
fn parse_mounts_excludes_local_file_mounts() {
    let local = "Mount(1): 32GB USB Drive -> file:///run/media/gm/32GB%20USB%20Drive/\n";
    assert_eq!(parse_mounts(local).len(), 0, "a file:// mount is excluded");
}

#[test]
fn parse_mounts_reads_each_mount_line() {
    let multi = concat!(
        "Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n",
        "Mount(1): data on 192.168.1.10 -> smb://192.168.1.10/data/\n",
    );
    let mounts = parse_mounts(multi);
    assert_eq!(mounts.len(), 2, "two Mount() lines parse to two entries");
    assert_eq!(mounts[1].label, "data", "the second entry's label is its own");
}

#[test]
fn non_file_bookmarks_keeps_only_network_uris() {
    let marks = concat!(
        "file:///home/gm/Downloads Downloads\n",
        "file:///home/gm/Projects Projects\n",
        "file:///home/gm/Pictures Pictures\n",
        "file:///home/gm/Videos Videos\n",
        "smb://192.168.1.10/ NAS\n",
    );
    let b = non_file_bookmarks(marks);
    assert_eq!(b.len(), 1, "only the non-file bookmark survives");
    assert_eq!(b[0].uri, "smb://192.168.1.10/", "the uri is the bare server root");
    assert_eq!(b[0].label, "NAS", "the trailing label wins");
    assert_eq!(non_file_bookmarks("").len(), 0, "empty bookmarks parses to nothing");
    assert_eq!(non_file_bookmarks("not a bookmarks file\n").len(), 0, "garbage bookmarks parses to nothing");
}

#[test]
fn non_file_bookmarks_label_falls_back_to_leaf_or_host() {
    let no_label = "smb://192.168.1.10/isos/\n";
    assert_eq!(non_file_bookmarks(no_label)[0].label, "isos", "a bookmark with no label falls back to the share leaf");
    let bare_root = "smb://192.168.1.10/\n";
    assert_eq!(non_file_bookmarks(bare_root)[0].label, "192.168.1.10", "a bare server root with no label falls back to the host");
}

#[test]
fn normalize_gives_one_canonical_uri_per_share() {
    assert_eq!(normalize("smb://h/data/"), "smb://h/data", "a share uri normalizes its trailing slash away");
    assert_eq!(normalize("smb://h/data"), "smb://h/data", "a share uri with no trailing slash is already canonical");
    assert_eq!(normalize("smb://h/data"), normalize("smb://h/data/"), "smb://h/data and smb://h/data/ cannot coexist as two rows");
    assert_eq!(normalize("smb://h/"), "smb://h/", "a bare server root keeps its one trailing slash");
    assert_eq!(normalize("smb://h"), "smb://h/", "a bare server root typed with no slash still canonicalizes to one");
    assert_ne!(normalize("smb://h/data/"), normalize("smb://h/other/"), "two different shares stay distinct after normalizing");
}

#[test]
fn rail_menu_offers_open_beside_releases_for_mounted_volume() {
    let volume = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), mounted: true, removable: true, ..RailEntry::default() };
    assert_eq!(ids(&volume), vec!["open", "unmountVolume", "eject", "flashImage"], "a mounted removable volume opens beside its releases");
    assert_eq!(ids(&volume)[0], "open", "Open runs the row's own activation");
    assert_eq!(glyphs(&volume)[0], "folder-open", "Open takes the listing menu's own mark");
    assert_eq!(ids(&volume)[1], "unmountVolume", "the Unmount row carries the unmount action");
    assert_eq!(glyphs(&volume)[1], "drive", "the Unmount row draws the drive mark");
    assert_eq!(ids(&volume)[2], "eject", "the Eject row carries the eject action");
    assert_eq!(glyphs(&volume)[2], "eject", "the Eject row draws the eject mark");
    assert_eq!(rail_menu(&volume).len(), 6, "a group change draws the listing menu's own separator");
}

#[test]
fn rail_menu_offers_eject_for_mounted_loop_device() {
    // A loop device is never removable at the kernel level, yet still offers Eject.
    let loop_row = RailEntry { label: "OMARCHY_202608".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/loop0p1".into(), mounted: true, loop_device: true, ..RailEntry::default() };
    assert_eq!(ids(&loop_row), vec!["open", "unmountVolume", "eject"], "a mounted loop device opens beside its releases despite removable being false");
    assert_eq!(ids(&loop_row)[2], "eject", "the loop row's Eject carries the eject action");
}

#[test]
fn rail_menu_offers_share_rows_and_their_marks() {
    let share = RailEntry { label: "isos".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/isos/".into(), mounted: true, ..RailEntry::default() };
    assert_eq!(ids(&share), vec!["open", "unmount", "rename", "editPlace", "remove"], "a mounted network share opens beside its release");
    assert_eq!(ids(&share), vec!["open", "unmount", "rename", "editPlace", "remove"], "its Open is the listing menu's own row too");
    assert_eq!(glyphs(&share), vec!["folder-open", "drive", "rename", "sliders", "minus"], "Mount and Unmount share the drive mark, Eject keeps its own");
}

#[test]
fn rail_menu_offers_nothing_or_only_fixed_rows_where_no_release_applies() {
    let internal = RailEntry { label: "nvme0n1".into(), group: Group::Device, kind: Kind::Disk, device: "/dev/nvme0n1".into(), mounted: true, ..RailEntry::default() };
    let fixed = RailEntry { label: "Vault".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sdb1".into(), mounted: true, ..RailEntry::default() };
    let dropbox = RailEntry { label: "Dropbox".into(), group: Group::Network, kind: Kind::Dropbox, mounted: true, ..RailEntry::default() };
    let favourite = RailEntry { label: "Home".into(), group: Group::Favourite, kind: Kind::Favourite, path: "/home/user".into(), ..RailEntry::default() };
    let idle = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), removable: true, ..RailEntry::default() };
    let bookmark = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/".into(), ..RailEntry::default() };
    assert_eq!(rail_menu(&internal).len(), 0, "the internal disk offers nothing, it is the box's own system disk");
    assert_eq!(ids(&fixed), vec!["open", "unmountVolume"], "a fixed internal volume offers Open and Unmount, only Eject stays removable-only");
    assert_eq!(rail_menu(&dropbox).len(), 0, "the Dropbox row offers nothing, it is a local folder the stock service owns");
    assert_eq!(ids(&favourite), vec!["removeFavourite"], "a favourite offers INVENTORY's own Remove from Favorites");
    assert_eq!(ids(&idle), vec!["mountVolume", "flashImage"], "an unmounted removable volume offers the Mount its activation does, and Flash image");
    assert_eq!(ids(&bookmark), vec!["rename", "editPlace", "remove"], "a bookmark nothing has mounted offers no release, only its own rows");
    // Rust has no null entry: RailEntry::default() stands in for the JS null and undefined rows.
    assert_eq!(rail_menu(&RailEntry::default()).len(), 0, "no entry at all offers nothing rather than throwing");
    assert_eq!(rail_menu(&RailEntry::default()).len(), 0, "an undefined entry offers nothing rather than throwing");
}

#[test]
fn eject_reaches_only_mounted_removable_or_loop_volume() {
    let idle = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), removable: true, ..RailEntry::default() };
    let internal = RailEntry { label: "nvme0n1".into(), group: Group::Device, kind: Kind::Disk, device: "/dev/nvme0n1".into(), mounted: true, ..RailEntry::default() };
    let fixed = RailEntry { label: "Vault".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sdb1".into(), mounted: true, ..RailEntry::default() };
    let dropbox = RailEntry { label: "Dropbox".into(), group: Group::Network, kind: Kind::Dropbox, mounted: true, ..RailEntry::default() };
    let favourite = RailEntry { label: "Home".into(), group: Group::Favourite, kind: Kind::Favourite, path: "/home/user".into(), ..RailEntry::default() };
    let bookmark = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/".into(), ..RailEntry::default() };
    let share = RailEntry { label: "isos".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/isos/".into(), mounted: true, ..RailEntry::default() };
    let loop_row = RailEntry { label: "OMARCHY_202608".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/loop0p1".into(), mounted: true, loop_device: true, ..RailEntry::default() };
    let never = [
        idle,
        internal,
        fixed,
        dropbox,
        favourite,
        bookmark,
        share,
        RailEntry::default(),
        RailEntry::default(),
    ];
    assert!(!(never.iter().any(|e| ids(e).contains(&"eject"))), "no row but a mounted removable volume is ever offered eject");
    assert!(ids(&loop_row).contains(&"eject"), "a mounted loop device is the one other row eject reaches");
}

#[test]
fn rail_key_names_each_row_by_its_stable_handle() {
    let volume = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), mounted: true, removable: true, ..RailEntry::default() };
    let share = RailEntry { label: "isos".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/isos/".into(), mounted: true, ..RailEntry::default() };
    let internal = RailEntry { label: "nvme0n1".into(), group: Group::Device, kind: Kind::Disk, device: "/dev/nvme0n1".into(), mounted: true, ..RailEntry::default() };
    let dropbox = RailEntry { label: "Dropbox".into(), group: Group::Network, kind: Kind::Dropbox, mounted: true, ..RailEntry::default() };
    let favourite = RailEntry { label: "Home".into(), group: Group::Favourite, kind: Kind::Favourite, path: "/home/user".into(), ..RailEntry::default() };
    assert_eq!(rail_key(&volume), "/dev/sda1", "a volume's key is its device node");
    assert_eq!(rail_key(&share), "smb://example.com/isos/", "a share's key is its uri");
    assert_eq!(rail_key(&internal), "", "the internal disk has no key");
    assert_eq!(rail_key(&dropbox), "", "the Dropbox row has no key");
    assert_eq!(rail_key(&favourite), "", "a favourite has no key");
    assert_eq!(rail_key(&RailEntry::default()), "", "no entry at all has no key rather than throwing");
}

#[test]
fn row_by_key_resolves_a_key_to_its_position_or_none() {
    let share = RailEntry { label: "isos".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/isos/".into(), mounted: true, ..RailEntry::default() };
    let bookmark = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/".into(), ..RailEntry::default() };
    let dropbox = RailEntry { label: "Dropbox".into(), group: Group::Network, kind: Kind::Dropbox, mounted: true, ..RailEntry::default() };
    let internal = RailEntry { label: "nvme0n1".into(), group: Group::Device, kind: Kind::Disk, device: "/dev/nvme0n1".into(), mounted: true, ..RailEntry::default() };
    let volume = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), mounted: true, removable: true, ..RailEntry::default() };
    let rail = [share.clone(), bookmark.clone(), dropbox.clone()];
    assert_eq!(row_by_key(&rail, "smb://example.com/isos/"), Some(0), "a share's key finds its own row");
    let moved = [bookmark.clone(), dropbox.clone(), share.clone()];
    assert_eq!(row_by_key(&moved, "smb://example.com/isos/"), Some(2), "a key finds the row it named after the list moved");
    let gone = [bookmark.clone(), dropbox.clone()];
    assert_eq!(row_by_key(&gone, "smb://example.com/isos/"), None, "a key naming a row that is gone resolves to nothing");
    assert_eq!(row_by_key(&rail, ""), None, "an empty key never resolves to whatever sits at index 0");
    assert_eq!(row_by_key(&[], "/dev/sda1"), None, "an empty rail resolves nothing");
    // Rust has no null rail: an empty slice stands in for the JS null rail.
    assert_eq!(row_by_key(&[], "/dev/sda1"), None, "no rail at all resolves nothing rather than throwing");
    let volumes = [internal.clone(), volume.clone()];
    assert_eq!(row_by_key(&volumes, "/dev/sda1"), Some(1), "a volume's key finds it past the internal disk");
    assert_eq!(row_by_key(&volumes, "/dev/nvme0n1"), None, "the internal disk's device node is not a key and finds nothing");
}

// NOT PORTED: "<label>'s mark is real, not the silent file fallback" (8 runtime checks: Open, Unmount, Eject, Open, Unmount, Rename, Edit address, Remove from Network): calls Icons.pathFor, owned by the menu primitive unit.

#[test]
fn same_entries_compares_rail_lists_for_an_unchanged_poll() {
    let nas = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/data".into(), glyph: "server".into(), ..RailEntry::default() };
    let again = nas.clone();
    assert!(same_entries(std::slice::from_ref(&nas), std::slice::from_ref(&again)), "two polls that found the same share compare equal");
    let mounted = RailEntry { mounted: true, ..nas.clone() };
    assert!(!same_entries(std::slice::from_ref(&nas), &[mounted]), "a share that has since mounted does not");
    let relabelled = RailEntry { label: "Homelab".into(), ..nas.clone() };
    assert!(!same_entries(std::slice::from_ref(&nas), &[relabelled]), "a relabelled share does not");
    let photos = RailEntry { uri: "smb://example.com/photos".into(), ..nas.clone() };
    assert!(!same_entries(std::slice::from_ref(&nas), &[photos]), "a different share at the same position does not, which is the rebind that emptied an editor");
    assert!(!same_entries(std::slice::from_ref(&nas), &[nas.clone(), again.clone()]), "a share appearing does not");
    assert!(same_entries(&[], &[]), "an empty rail still compares equal to itself");
    let vol = RailEntry { path: "/run/media/user/128GB".into(), label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), mounted: true, glyph: "drive".into(), ..RailEntry::default() };
    assert!(same_entries(std::slice::from_ref(&vol), std::slice::from_ref(&vol)), "a device entry compares on its own fields too");
    // The JS size text is not a byte count, so two distinct placeholder sizes stand in for "116.1G" and "119.2G".
    let capacity = RailEntry { size: Some(1), ..vol.clone() };
    let grown = RailEntry { size: Some(2), ..vol.clone() };
    assert!(!same_entries(&[capacity], &[grown]), "a capacity-only update reaches the live rail");
    let unplugged = RailEntry { path: String::new(), mounted: false, ..vol.clone() };
    assert!(!same_entries(std::slice::from_ref(&vol), &[unplugged]), "and an unplugged volume differs");
    // A null side has no Rust form, so an empty slice against one entry stands in for the JS missing side.
    assert!(!same_entries(&[], std::slice::from_ref(&nas)), "a missing side is never equal, so a first poll always assigns");
}

#[test]
fn holding_names_the_volume_a_listing_sits_inside() {
    let stick = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), path: "/run/media/user/128GB".into(), mounted: true, removable: true, ..RailEntry::default() };
    let pulled = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), removable: true, ..RailEntry::default() };
    let disk = RailEntry { label: "nvme0n1".into(), group: Group::Device, kind: Kind::Disk, device: "/dev/nvme0n1".into(), path: "/".into(), mounted: true, ..RailEntry::default() };
    let houses = [disk.clone(), stick.clone()];
    assert_eq!(holding(&houses, "/run/media/user/128GB/photos"), Some(&stick), "a directory inside the volume names it");
    assert_eq!(holding(&houses, "/run/media/user/128GB"), Some(&stick), "the volume's own root names it");
    assert_eq!(holding(&houses, "/run/media/user/128GB-old"), None, "a sibling that only shares the prefix does not");
    assert_eq!(holding(&houses, "/etc"), None, "the internal disk holds everything and is never the answer");
    let pulled_rail = [disk.clone(), pulled.clone()];
    assert_eq!(holding(&pulled_rail, "/run/media/user/128GB"), None, "an unmounted volume has no path to be inside");
        let pathless = [RailEntry { label: "isos".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/isos/".into(), mounted: true, ..RailEntry::default() }];
    assert_eq!(holding(&pathless, "/run/user/1000/gvfs/x"), None, "a share carries no path, so a listing inside one answers nothing rather than guessing");
    // Rust has no null rail: an empty slice stands in for the JS null rail.
    assert_eq!(holding(&[], "/x"), None, "no rail at all answers nothing rather than throwing");
}

#[test]
fn route_resolves_a_chosen_key_to_its_own_row_only() {
    let disk = RailEntry { label: "nvme0n1".into(), group: Group::Device, kind: Kind::Disk, device: "/dev/nvme0n1".into(), path: "/".into(), mounted: true, ..RailEntry::default() };
    let stick = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), path: "/run/media/user/128GB".into(), mounted: true, removable: true, ..RailEntry::default() };
    let devices = [disk, stick];
        let network = [RailEntry { label: "isos".into(), group: Group::Network, kind: Kind::Share, uri: "smb://x/isos/".into(), mounted: true, ..RailEntry::default() }];
    assert_eq!(route(RailAction::Eject, "/dev/sda1", &devices, &network), Route::Eject(1), "eject resolves the volume's position and never the network Service");
    assert_eq!(route(RailAction::Unmount, "smb://x/isos/", &devices, &network), Route::UnmountShare(0), "unmount resolves the share's position and never the device Service");
    assert_eq!(route(RailAction::Eject, "/dev/sdz9", &devices, &network), Route::Nothing, "a key that no longer names a row releases nothing");
    assert_eq!(action_by_id("forget"), None, "an action that is neither release does nothing");
}
