//! Ctrl+E's release: the rail cursor or the listing's own drive, and the sentence when there is nothing to eject.
use super::eject::{release, Release};
use super::menu::RailAction;
use super::{Group, Kind, RailEntry};

fn volume(device: &str, path: &str, removable: bool) -> RailEntry {
    RailEntry {
        group: Group::Device,
        kind: Kind::Volume,
        device: device.into(),
        path: path.into(),
        label: path.rsplit('/').next().unwrap_or_default().into(),
        mounted: true,
        removable,
        ..RailEntry::default()
    }
}

fn rail() -> Vec<RailEntry> {
    let disk = RailEntry { group: Group::Device, kind: Kind::Disk, device: "/dev/vda".into(), label: "QEMU HARDDISK".into(), path: "/".into(), mounted: true, ..RailEntry::default() };
    vec![disk, volume("/dev/sdb1", "/run/media/gm/128GB", true), volume("/dev/nvme1n1p1", "/mnt/data", false)]
}

#[test]
fn a_folder_no_drive_holds_says_so_as_the_eject_refusal_board_does() {
    let rows = rail();
    let said = release(Some((&rows, None)), "/fx/v/work", false, false, false);
    let Release::Say(note) = said else { panic!("{said:?}") };
    assert_eq!(note.english(), "This folder is not on a drive Flea can eject.");
}

#[test]
fn a_folder_on_the_system_volume_names_it_unless_it_was_just_released() {
    let rows = rail();
    let Release::Say(note) = release(Some((&rows, None)), "/home/gm", false, true, false) else { panic!() };
    assert_eq!(note.key, "device.system_volume");
    let Release::Say(note) = release(Some((&rows, None)), "/home/gm", false, true, true) else { panic!() };
    assert_eq!(note.key, "device.no_volume");
}

#[test]
fn a_listing_inside_a_stick_ejects_that_stick_by_its_key() {
    let rows = rail();
    let chosen = release(Some((&rows, None)), "/run/media/gm/128GB/photos", false, false, false);
    assert_eq!(chosen, Release::Chosen(RailAction::Eject, "/dev/sdb1".into()));
}

#[test]
fn a_fixed_volume_unmounts_and_the_system_disk_refuses_from_the_rail() {
    let rows = rail();
    assert_eq!(release(Some((&rows, Some(2))), "", true, false, false), Release::Chosen(RailAction::UnmountVolume, "/dev/nvme1n1p1".into()));
    let Release::Say(note) = release(Some((&rows, Some(0))), "", true, false, false) else { panic!() };
    assert_eq!(note.key, "device.system_drive");
}

#[test]
fn a_hidden_rail_hands_off_to_a_one_shot_listing() {
    assert_eq!(release(None, "/run/media/gm/128GB", false, false, false), Release::Hidden);
}
