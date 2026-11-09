//! P2A-NETWORK: the NETWORK rail readers report the drawn rows, the build state and the plus geometry.
use super::*;
use crate::{backend::Backend, ipc::server, state::tests::shell_with};
use flea::{
    ipc::wire,
    ui_logic_device_rail::{Group as Group1, Kind as Kind1, RailEntry as Row1},
    ui_logic_rail_places::rail::Group as Group4,
};
use gpui::{AnyView, Entity, TestAppContext, VisualTestContext};

fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(crate::g2::device_rail::source::fixture::Services {
            runner: crate::window::network::default_runner(),
            hostname: "testhost".into(),
        })
    });
    let (backend, receiver) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        cx.on_release(move |_, _| drop(receiver)).detach();
        let join = mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            shell.focus_side,
            cx,
        );
        cx.subscribe(&join.host, |shell, _, event, cx| {
            shell.rail_host(*event, cx)
        })
        .detach();
        shell.slots.rail = Some(AnyView::from(join.host.clone()));
        shell.rail = Some(join);
        shell
    });
    cx.run_until_parked();
    frame(cx);
    (shell, cx)
}

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn ipc(
    shell: &Entity<Shell>,
    name: &str,
    args: &[&str],
    cx: &mut VisualTestContext,
) -> wire::Reply {
    let (answer, receiver) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                server::Event::Call {
                    request: wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: args.iter().map(|v| (*v).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    receiver.try_recv().expect("a synchronous IPC answer").reply
}

fn share(label: &str, mounted: bool) -> Row1 {
    Row1 {
        uri: format!("smb://stub/{label}"),
        label: label.into(),
        group: Group1::Network,
        kind: Kind1::Share,
        glyph: "server".into(),
        mounted,
        ..Row1::default()
    }
}

fn publish(shell: &Entity<Shell>, rows: Vec<Row1>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, cx| shell.network_publish(Some(rows), cx));
    frame(cx);
}

fn answer(shell: &Entity<Shell>, rows: Vec<Row1>, cx: &mut VisualTestContext) {
    reads(shell, true, true, rows, cx);
}

/// Sets which of the listing and the mountinfo read have answered, then publishes the rows.
fn reads(
    shell: &Entity<Shell>,
    listed: bool,
    mountinfo: bool,
    rows: Vec<Row1>,
    cx: &mut VisualTestContext,
) {
    shell.update(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .devices
            .update(cx, |source, cx| {
                source.fixture_mount_listing().answered = listed;
                cx.notify();
            });
        let network = shell
            .rail
            .as_mut()
            .and_then(|join| join.network.as_mut())
            .expect("the first frame builds the network join");
        network.listing.answered = listed;
        network.mountinfo_ready = mountinfo;
    });
    publish(shell, rows, cx);
}

// Math.round(): half rounds up; window coordinates are never negative here.
fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

// FleaWindow.boxOf(): "x width centre", the centre kept at three decimals.
fn box_of(x: f32, width: f32) -> String {
    format!("{} {} {:.3}", round(x), round(width), x + width / 2.)
}

#[gpui::test]
fn network_entries_reports_the_drawn_network_rows(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(ipc(&shell, "networkEntries", &[], cx).stdout.trim(), "");
    publish(&shell, vec![share("Live", true), share("Saved", false)], cx);
    assert_eq!(
        ipc(&shell, "networkEntries", &[], cx).stdout.trim(),
        "Live|network|share|true\nSaved|network|share|false"
    );
}

#[gpui::test]
fn network_built_needs_a_listing_and_a_mountinfo_read(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(ipc(&shell, "networkBuilt", &[], cx).stdout.trim(), "false");
    publish(&shell, vec![share("Live", true)], cx);
    assert_eq!(
        ipc(&shell, "networkBuilt", &[], cx).stdout.trim(),
        "false",
        "rows without an answered listing and mountinfo read are not built"
    );
    for (listed, mountinfo) in [(true, false), (false, true)] {
        reads(&shell, listed, mountinfo, vec![share("Live", true)], cx);
        assert_eq!(
            ipc(&shell, "networkBuilt", &[], cx).stdout.trim(),
            "false",
            "listing {listed} and mountinfo {mountinfo} is not built"
        );
    }
    answer(&shell, vec![share("Live", true)], cx);
    assert_eq!(ipc(&shell, "networkBuilt", &[], cx).stdout.trim(), "true");
}

#[gpui::test]
fn network_mark_geometry_reports_the_ink_hit_target_and_dot(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(
        ipc(&shell, "networkMarkGeometry", &[], cx).stdout.trim(),
        ""
    );
    publish(&shell, vec![share("Live", true)], cx);
    let (add, dot) = shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.read_with(cx, |producer, _| {
            let laid = producer.laid.borrow();
            let add = laid
                .headers
                .iter()
                .find_map(|header| header.add)
                .expect("the network heading draws its plus");
            let index = producer
                .entries()
                .iter()
                .position(|entry| entry.group == Group4::Network)
                .expect("the published share is a drawn rail row");
            let row = laid.row(index).expect("the network row is laid").clone();
            (add, row.indicator_rect)
        })
    });
    let side = add.width.max(24.);
    let hit = (
        add.x + (add.width - side) / 2.,
        add.y + (add.height - side) / 2.,
        side,
    );
    let expected = format!(
        "{}|{}|{}",
        box_of(add.x, add.width),
        box_of(hit.0, hit.2),
        box_of(dot.x, dot.width)
    );
    let geometry = ipc(&shell, "networkMarkGeometry", &[], cx).stdout;
    assert_eq!(geometry.trim(), expected);
    let mut parts = geometry.trim().split('|');
    let glyph: Vec<_> = parts.next().unwrap().split_whitespace().collect();
    let target: Vec<_> = parts.next().unwrap().split_whitespace().collect();
    let dot_part: Vec<_> = parts.next().unwrap().split_whitespace().collect();
    assert_eq!(glyph[2], dot_part[2], "the ink shares the dot's x centre");
    assert_eq!(
        target[2], dot_part[2],
        "the hit target shares the dot's x centre"
    );
    assert!(
        target[1].parse::<f32>().unwrap() >= 24.,
        "the hit target keeps its minimum width"
    );
}

#[gpui::test]
fn network_mark_centre_is_the_hit_target_middle(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(ipc(&shell, "networkMarkCentre", &[], cx).stdout.trim(), "");
    publish(&shell, vec![share("Live", true)], cx);
    let add = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .read_with(cx, |producer, _| {
                producer
                    .laid
                    .borrow()
                    .headers
                    .iter()
                    .find_map(|header| header.add)
                    .expect("the network heading draws its plus")
            })
    });
    let side = add.width.max(24.);
    let (x, y) = (
        add.x + (add.width - side) / 2.,
        add.y + (add.height - side) / 2.,
    );
    assert_eq!(
        ipc(&shell, "networkMarkCentre", &[], cx).stdout.trim(),
        format!("{} {}", round(x + side / 2.), round(y + side / 2.))
    );
}
