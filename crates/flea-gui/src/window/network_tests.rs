//! The live rail row reaches PL2's service, and its mount reply navigates only the pane that asked.
use super::*;
use crate::backend::Backend;
use crate::g2::pl1::process::{Event as Proc, Handle, Job, Outcome};
use crate::state::tests::shell_with;
use crate::window::rail::{mount, mount_with_source};
use gpui::{AnyView, TestAppContext, VisualTestContext};
use std::time::Duration;

#[path = "network_alias_tests.rs"]
mod alias_tests;
#[path = "network_bfx_net_tests.rs"]
mod bfx_net_tests;
#[path = "network_vf3_tests.rs"]
mod vf3_tests;

#[path = "network_handoff_tests.rs"]
mod handoff_tests;
#[path = "network_native11_tests.rs"]
mod native11_tests;
#[path = "network_nearby_tests.rs"]
mod nearby_tests;

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
    fn find(&self, word: &str) -> Vec<usize> {
        self.jobs
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, job)| job.argv.iter().any(|arg| arg == word))
            .map(|(at, _)| at)
            .collect()
    }
    /// Mount requests only, never the rail's own gio mount -li listing.
    fn mounts(&self) -> Vec<usize> {
        self.find("mount")
            .into_iter()
            .filter(|at| !self.jobs.borrow()[*at].argv.iter().any(|arg| arg == "-li"))
            .collect()
    }
    fn answer(&self, at: usize, code: i32, stdout: &str) {
        self.answers.borrow()[at]
            .try_send(Proc::Exited(Outcome {
                code,
                stdout: stdout.into(),
                stderr: String::new(),
            }))
            .unwrap();
    }
}

type Lines = Box<dyn FnMut() -> Option<String>>;

fn open(
    recorder: Rc<Recorder>,
    cx: &mut TestAppContext,
) -> (Entity<Shell>, &mut VisualTestContext, [Lines; 2]) {
    let (left, left_lines) = Backend::test_lines();
    let (right, right_lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(left, cx);
        shell.panes[1] = shell_with(right, cx).panes[0].take();
        shell.view = crate::window::startup::View::Dual;
        window.focus(&shell.focus, cx);
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
    (shell, cx, [Box::new(left_lines), Box::new(right_lines)])
}

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn uish_misc_network_and_phone_rows_share_one_mount_listing(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _) = open(recorder.clone(), cx);
    let listings = recorder.find("-li");
    assert_eq!(
        listings.len(),
        1,
        "a window must not race two mount-list producers"
    );
    let text = "Mount(0): NAS -> smb://host/share/\nVolume(0): Phone\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://phone/\n  can_mount=1\n  Mount(1): Phone -> mtp://phone/\n";
    recorder.answer(listings[0], 0, text);
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let join = shell.rail.as_ref().unwrap();
        assert_eq!(join.devices.read(cx).mount_snapshot().1.text, text);
        assert_eq!(join.network.as_ref().unwrap().listing.text, text);
        assert_eq!(join.devices.read(cx).phones.entries.len(), 1);
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn drain(lines: &mut Lines) -> Vec<String> {
    std::iter::from_fn(lines).collect()
}

fn share() -> RailEntry {
    RailEntry {
        uri: "sftp://user@host/folder".into(),
        label: "NAS".into(),
        group: flea::ui_logic_pl1::Group::Network,
        kind: flea::ui_logic_pl1::Kind::Share,
        glyph: "server".into(),
        ..RailEntry::default()
    }
}

/// Publishes one unmounted share and activates its drawn rail row from the given side, as a press does.
fn press_share(shell: &Entity<Shell>, side: usize, cx: &mut VisualTestContext) {
    let producer = cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().places.clone());
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_publish(Some(vec![share()]), cx)
        });
        producer.update(cx, |producer, cx| {
            let index = producer
                .entries()
                .iter()
                .position(|entry| entry.group == Group::Network)
                .expect("the published share is a drawn rail row");
            producer.side = side;
            producer.activate(index, cx);
        });
    });
    frame(cx);
}

fn answer_mount(recorder: &Recorder, path: &str, cx: &mut VisualTestContext) {
    let mount = *recorder
        .mounts()
        .last()
        .expect("the row's press ran gio mount");
    recorder.answer(mount, 0, "");
    cx.run_until_parked();
    let info = *recorder
        .find("info")
        .last()
        .expect("a landed mount asks gio info");
    recorder.answer(info, 0, &format!("local path: {path}\n"));
    cx.run_until_parked();
    frame(cx);
}

#[gpui::test]
fn m9_integ_network_mount_reply_opens_only_originating_pane(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder.clone(), cx);
    assert!(
        cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().network.is_some()),
        "the first frame builds the network join"
    );
    drain(&mut lines[0]);
    drain(&mut lines[1]);
    press_share(&shell, 1, cx);
    assert!(
        recorder.find("info").is_empty() && recorder.mounts().len() == 1,
        "the rail row's Activate must reach the service as one gio mount"
    );
    answer_mount(&recorder, "/fixture/mnt", cx);
    let (left, right) = (drain(&mut lines[0]), drain(&mut lines[1]));
    assert!(
        right.iter().any(|line| line.contains("/fixture/mnt")),
        "the landed mount must list in the originating pane: {right:?}"
    );
    assert!(
        !left.iter().any(|line| line.contains("/fixture/mnt")),
        "the other pane must not move: {left:?}"
    );

    // Negative control: the originating pane is replaced before the reply lands, so the reply is stale.
    press_share(&shell, 1, cx);
    let (fresh, fresh_lines) = Backend::test_lines();
    let mut fresh_lines: Lines = Box::new(fresh_lines);
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[1] = shell_with(fresh, cx).panes[0].take();
        })
    });
    drain(&mut fresh_lines);
    answer_mount(&recorder, "/fixture/stale", cx);
    let [left, right] = &mut lines;
    let landed: Vec<String> = [left, right, &mut fresh_lines]
        .into_iter()
        .flat_map(drain)
        .collect();
    assert!(
        !landed.iter().any(|line| line.contains("/fixture/stale")),
        "a reply for a pane that is gone must navigate nowhere, the pane opened in its place included: {landed:?}"
    );
}

#[gpui::test]
fn m9_integ_network_unmount_releases_the_rows_own_identity(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    let fuse = RailEntry {
        path: "/mnt/rclone".into(),
        uri: "/mnt/rclone".into(),
        fstype: "fuse.rclone".into(),
        mounted: true,
        ..share()
    };
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_publish(Some(vec![fuse.clone()]), cx);
            let origin = shell.network_origin(0);
            let row = shell
                .network_row(&section(std::slice::from_ref(&fuse), true)[0].rows[0])
                .expect("the drawn row maps back to the join's row");
            shell.network_unmount(&row, origin, cx);
        })
    });
    cx.run_until_parked();
    let jobs = recorder.jobs.borrow();
    let release = jobs
        .iter()
        .find(|job| job.argv.first().map(String::as_str) == Some("fusermount3"))
        .expect("Unmount on a FUSE row runs fusermount3");
    assert_eq!(release.argv, ["fusermount3", "-u", "--", "/mnt/rclone"]);
}

#[gpui::test]
fn m9_integ_add_network_key_and_rail_plus_open_the_card(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder, cx);
    let card = cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .clone()
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_add(0, window, cx);
            assert!(
                shell.card_holds_keyboard(cx),
                "an open card holds the keyboard"
            );
        })
    });
    assert!(cx.update(|_, cx| card.read(cx).model.opened));
    cx.update(|_, cx| card.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    assert!(!cx.update(|_, cx| card.read(cx).model.opened));
    let producer = cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().places.clone());
    cx.update(|_, cx| producer.update(cx, |p, _| p.model.borrow_mut().add_presses += 1));
    frame(cx);
    assert!(
        cx.update(|_, cx| card.read(cx).model.opened),
        "a press on the NETWORK heading's plus opens the card"
    );
}

/// Points the join's bookmarks writer at a fresh marked sandbox and returns that sandbox.
pub(crate) fn sandbox_bookmarks(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> PathBuf {
    let sandbox = std::env::temp_dir().join(format!(
        "flea-network-bookmarks-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir_all(&sandbox).unwrap();
    std::fs::write(sandbox.join(".flea-test-sandbox"), "").unwrap();
    let marks = sandbox.join("bookmarks");
    shell.update(cx, |shell, _| {
        shell
            .network()
            .expect("the first frame built the network join")
            .bookmarks = marks
    });
    sandbox
}

/// Waits for the writer's own thread to land the sandbox file, then removes the marked sandbox.
pub(crate) fn written_bookmarks(sandbox: PathBuf, cx: &mut VisualTestContext) -> String {
    let mut written = String::new();
    for _ in 0..200 {
        cx.run_until_parked();
        written = std::fs::read_to_string(sandbox.join("bookmarks")).unwrap_or_default();
        if !written.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(sandbox.is_absolute() && sandbox.join(".flea-test-sandbox").exists());
    std::fs::remove_dir_all(&sandbox).unwrap();
    written
}

#[path = "network_release_tests.rs"]
mod release;

#[path = "network_result_tests.rs"]
mod result_tests;

#[path = "network_ipc_tests.rs"]
mod ipc_tests;

#[path = "network_mountinfo_tests.rs"]
mod mountinfo_tests;

#[path = "network_typing_tests.rs"]
mod typing;

#[path = "network_parity_tests.rs"]
mod parity_tests;

#[path = "network_cloud_tests.rs"]
mod cloud_tests;

#[path = "network_motion_tests.rs"]
mod motion_tests;

#[path = "network_muted_tests.rs"]
mod muted_tests;
#[path = "network_open_focus_tests.rs"]
mod open_focus_tests;

#[path = "network_native11_view_tests.rs"]
mod native11_view_tests;

#[path = "network_native12_tests.rs"]
mod native12_tests;

#[path = "network_bridge_line_tests.rs"]
mod bridge_line_tests;

#[path = "network_connect_footer_tests.rs"]
mod connect_footer;
#[path = "network_s3_tests.rs"]
mod s3_tests;

#[path = "network_navigation_tests.rs"]
mod navigation_tests;
