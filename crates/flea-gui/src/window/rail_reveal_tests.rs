//! The auto-hide reveal draws NETWORK as the docked rail does: the first reveal lists the shares at once.
#[path = "rail_menu_presentation_tests.rs"]
mod menu_presentation;
use super::*;
use crate::backend::Backend;
use crate::g2::device_rail::process::{Event as Proc, Handle, Job, Outcome, Runner};
use crate::rail::geometry::EDGE;
use crate::state::tests::shell_with;
use flea::{
    jsondoc,
    ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    },
};
use gpui::{point, px, AnyView, Entity, Modifiers, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, time::Duration};

const SHARE_OUTPUT: &str = "Mount(0): NAS -> smb://nas/share\n";

#[derive(Default)]
struct Recorder {
    jobs: RefCell<Vec<Job>>,
    answers: RefCell<Vec<async_channel::Sender<Proc>>>,
}

impl Runner for Recorder {
    fn start(&self, job: Job) -> Handle {
        let (handle, send) = Handle::detached();
        if job.argv.first().is_some_and(|program| program == "lsblk") {
            send.try_send(Proc::Exited(Outcome {
                code: 0,
                stdout: "{\"blockdevices\":[]}".into(),
                stderr: String::new(),
            }))
            .unwrap();
        }
        self.jobs.borrow_mut().push(job);
        self.answers.borrow_mut().push(send);
        handle
    }
}

impl Recorder {
    /// The share listing is `gio mount -li`; mount requests carry no `-li`.
    fn listings(&self) -> Vec<usize> {
        self.jobs
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, job)| job.argv.iter().any(|arg| arg == "-li"))
            .map(|(at, _)| at)
            .collect()
    }

    fn answer(&self, at: usize, stdout: &str) {
        self.answers.borrow()[at]
            .try_send(Proc::Exited(Outcome {
                code: 0,
                stdout: stdout.into(),
                stderr: String::new(),
            }))
            .unwrap();
    }
}

fn open<'a>(
    state: &str,
    recorder: Rc<Recorder>,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (backend, receiver) = Backend::test_channel();
    let state = jsondoc::parse(state).unwrap();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        cx.on_release(move |_, _| drop(receiver)).detach();
        shell.launch.settings.data = state.clone();
        let mut join = mount_with_source(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            shell.focus_side,
            recorder.clone(),
            Some("testhost".into()),
            None,
            cx,
        );
        join.network_runner = recorder;
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

/// The pointer on the reveal strip, past the dwell and the open slide: the rail's own reveal path.
fn reveal(cx: &mut VisualTestContext) {
    cx.simulate_mouse_move(point(px(EDGE / 2.), px(100.)), None, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        crate::rail::DWELL_MS + crate::rail::OPEN_MS,
    ));
    frame(cx);
}

fn polling(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    producer.read_with(cx, |producer, _| producer.polling)
}

fn listing_running(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .listing
            .running
    })
}

fn headings(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<Heading> {
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    producer.read_with(cx, |producer, _| {
        producer
            .sections
            .iter()
            .map(|section| section.heading.clone())
            .collect()
    })
}

/// The docked rail's sections once its first share listing has answered `stdout`.
fn docked_headings(stdout: &str, cx: &mut TestAppContext) -> Vec<Heading> {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{}}"#, recorder.clone(), cx);
    let listings = recorder.listings();
    assert_eq!(
        listings.len(),
        1,
        "the docked rail lists the shares once at mount"
    );
    recorder.answer(listings[0], stdout);
    frame(cx);
    assert!(
        !listing_running(&shell, cx),
        "the docked listing has answered"
    );
    assert_eq!(
        recorder.listings().len(),
        1,
        "the docked rail starts no second listing"
    );
    headings(&shell, cx)
}

fn network_section(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<RailSection> {
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    producer.read_with(cx, |producer, _| {
        producer
            .sections
            .iter()
            .find(|section| section.heading == Heading::Key("places.heading.network"))
            .cloned()
    })
}

/// The NETWORK row the docked rail draws for the one share in SHARE_OUTPUT.
fn share_row() -> RailEntry {
    RailEntry {
        group: Group::Network,
        kind: Kind::Share,
        label: "NAS".into(),
        uri: "smb://nas/share".into(),
        mounted: Some(true),
        releasable: true,
        glyph: "server",
        ..RailEntry::default()
    }
}

#[gpui::test]
fn reveal_lists_the_shares_at_once_and_draws_network(cx: &mut TestAppContext) {
    let docked = docked_headings(SHARE_OUTPUT, cx);
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{"autoHide":true}}"#, recorder.clone(), cx);
    shell.update(cx, |shell, cx| shell.network_list(cx));
    frame(cx);
    assert!(
        recorder.listings().is_empty(),
        "an explicit refresh keeps the hidden rail parked until reveal"
    );
    reveal(cx);
    assert!(polling(&shell, cx), "the pointer revealed the rail");
    let listings = recorder.listings();
    assert_eq!(
        listings.len(),
        1,
        "the reveal starts the share listing at once, not on the next poll tick"
    );
    recorder.answer(listings[0], SHARE_OUTPUT);
    frame(cx);
    assert!(
        !listing_running(&shell, cx),
        "the reveal listing has answered"
    );
    let section = network_section(&shell, cx).expect("the revealed rail draws NETWORK");
    assert!(section.rows.contains(&share_row()));
    assert_eq!(
        headings(&shell, cx),
        docked,
        "the revealed rail lists the docked rail's sections in order"
    );
}

#[gpui::test]
fn docked_rail_draws_network_with_the_same_share(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{}}"#, recorder.clone(), cx);
    let listings = recorder.listings();
    assert_eq!(
        listings.len(),
        1,
        "the docked rail lists the shares at mount"
    );
    recorder.answer(listings[0], SHARE_OUTPUT);
    frame(cx);
    assert!(
        !listing_running(&shell, cx),
        "the docked listing has answered"
    );
    let section = network_section(&shell, cx).expect("the docked rail draws NETWORK");
    assert!(section.rows.contains(&share_row()));
}

#[gpui::test]
fn docked_rail_with_no_share_draws_no_network(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{}}"#, recorder.clone(), cx);
    let listings = recorder.listings();
    assert_eq!(
        listings.len(),
        1,
        "the docked rail lists the shares at mount"
    );
    recorder.answer(listings[0], "");
    frame(cx);
    assert!(
        !listing_running(&shell, cx),
        "the docked listing has answered"
    );
    assert!(network_section(&shell, cx).is_none());
}

#[gpui::test]
fn reveal_with_no_share_draws_no_network_as_docked_does(cx: &mut TestAppContext) {
    let docked = docked_headings("", cx);
    let recorder = Rc::new(Recorder::default());
    let (shell, cx) = open(r#"{"places":{"autoHide":true}}"#, recorder.clone(), cx);
    assert!(
        recorder.listings().is_empty(),
        "a hidden rail starts no share listing"
    );
    reveal(cx);
    assert!(polling(&shell, cx), "the pointer revealed the rail");
    let listings = recorder.listings();
    assert_eq!(
        listings.len(),
        1,
        "the reveal starts the share listing at once, not on the next poll tick"
    );
    recorder.answer(listings[0], "");
    frame(cx);
    assert!(
        !listing_running(&shell, cx),
        "the reveal listing has answered"
    );
    assert!(
        network_section(&shell, cx).is_none(),
        "with no share the docked rail draws no NETWORK section, and neither does the reveal"
    );
    assert_eq!(
        headings(&shell, cx),
        docked,
        "the revealed rail lists the docked rail's sections in order"
    );
}

#[path = "rail_reveal_menu_tests.rs"]
mod menu;
