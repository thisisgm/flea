//! EY-337 and EY-338: the DEVICES rows of minipc's listing, a stick, the system disk and a locked NVMe partition.
use super::*;
use crate::g2::rail_places::rows::LaidRow;

const MINIPC: &str = r#"{"blockdevices":[
{"name":"nvme1n1","path":"/dev/nvme1n1","type":"disk","size":512110190592,"children":[{"name":"nvme1n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda","path":"/dev/sda","type":"disk","tran":"usb","rm":true,"model":"USB Flash Disk","size":124623257600,"children":[
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":["/run/media/gm/128GB"],"fstype":"vfat","rm":true,"size":124622209024}]},
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","tran":"nvme","rm":false,"model":"KBG40ZNS256G NVMe KIOXIA 256GB","size":256060514304,"children":[
{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","type":"part","fstype":"crypto_LUKS","mountpoints":[null],"size":253910581248}]}
]}"#;

const STICK: &str = "128GB";
const LOCKED: &str = "KBG40ZNS256G NVMe KIOXIA 256GB";

fn device_rows(drive_size: bool, cx: &mut TestAppContext) -> (Vec<LaidRow>, f32) {
    let places = format!(
        r#"{{"rail":"shown","sidebarWidth":192,"showUnmounted":true,"driveSize":{drive_size}}}"#
    );
    let (shell, _service, cx) = root_with(cx, &places, MINIPC);
    frame(cx);
    let producer = producer(&shell, cx);
    let (rows, width) = producer.read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let rows = laid
            .rows
            .iter()
            .filter(|row| [STICK, LOCKED, "minipc"].contains(&row.label.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        (rows, laid.row_gap)
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    assert_eq!(
        rows.len(),
        3,
        "the rail paints the disk, the stick and the locked drive"
    );
    (rows, width)
}

fn named<'a>(rows: &'a [LaidRow], label: &str) -> &'a LaidRow {
    rows.iter().find(|row| row.label == label).unwrap()
}

#[gpui::test]
fn ey337_drive_size_prints_each_device_size_on_one_right_edge(cx: &mut TestAppContext) {
    let (rows, gap) = device_rows(true, cx);
    let details = rows
        .iter()
        .map(|row| row.detail.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        details,
        ["512.1 GB", "124.6 GB", "253.9 GB"],
        "SidebarRow.qml detail: Format.size of each device"
    );
    // RailDetails rules 1 and 3: every size ends on one x, a caption slot and a gap before the padding, square or none.
    let slot = named(&rows, STICK).indicator_rect;
    let edge = slot.x - gap;
    for row in &rows {
        let right = row.detail_rect.x + row.detail_rect.width;
        assert!(
            (right - edge).abs() < 0.01,
            "{} ends its size at {right}, not {edge}",
            row.label
        );
        assert_eq!(
            row.indicator_rect.x, slot.x,
            "{} keeps the caption slot after its size",
            row.label
        );
        assert!(
            row.label_rect.x + row.label_rect.width <= row.detail_rect.x - gap + 0.01,
            "{} keeps one gap before its size",
            row.label
        );
    }
    assert!(
        !named(&rows, "minipc").indicator_visible,
        "the system disk draws no square"
    );
    assert!(
        named(&rows, STICK).indicator_visible,
        "the stick keeps its eject mark"
    );
}

#[gpui::test]
fn ey337_drive_size_off_prints_no_size(cx: &mut TestAppContext) {
    let (rows, _) = device_rows(false, cx);
    assert!(
        rows.iter().all(|row| row.detail.is_empty()),
        "places.driveSize off draws no size"
    );
    let disk = named(&rows, "minipc");
    let slot = named(&rows, STICK).indicator_rect;
    assert_eq!(
        disk.indicator_rect.width, 0.,
        "a disk row with no size reserves no slot"
    );
    let right = disk.label_rect.x + disk.label_rect.width;
    assert!(
        (right - (slot.x + slot.width)).abs() < 0.01,
        "the disk label runs to the padding: {right}"
    );
}

#[gpui::test]
fn ey338_locked_drive_row_draws_its_lock_mark_in_the_icon_column(cx: &mut TestAppContext) {
    let (rows, _) = device_rows(true, cx);
    let locked = named(&rows, LOCKED);
    assert_eq!(
        locked.glyph, "lock",
        "DeviceMounts.qml: a locked container's row carries the lock glyph"
    );
    assert!(
        crate::g2::rail_places::rows::glyph::svg(locked.glyph).is_some(),
        "the rail has a mark to paint for the lock glyph, as Icons.js does"
    );
    let stick = named(&rows, STICK);
    assert_eq!(
        locked.label_rect.x, stick.label_rect.x,
        "nothing is indented: the labels share one column"
    );
}
