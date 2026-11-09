//! PickerPlaces parity: remote rows retain their order and opening route; Recent keeps the caller's filter.
use super::tests::fixture::{fixture, Fixture};
use crate::g2::device_rail::process::{Event, Handle, Job, KillProbe, Outcome, Runner};
use crate::g2::device_rail::source::fixture::Services;
use flea::jsondoc::{self, Json};
use flea::ui_logic_rail_places::{
    order::{Heading, RailSection},
    rail::{Group, Kind, RailEntry},
};
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

fn remote(phone: bool) -> RailEntry {
    RailEntry {
        group: if phone { Group::Device } else { Group::Network },
        kind: if phone { Kind::Phone } else { Kind::Share },
        label: if phone { "Pixel" } else { "NAS" }.into(),
        uri: if phone {
            "mtp://pixel/"
        } else {
            "smb://nas/docs"
        }
        .into(),
        glyph: if phone { "smartphone" } else { "server" },
        mounted: Some(false),
        ..RailEntry::default()
    }
}

#[test]
fn rail2_places_network_drives_phones_keep_v0312_order_and_marks() {
    let favourites =
        jsondoc::parse(r#"[{"label":"Remote favorite","path":"sftp://host/docs"}]"#).unwrap();
    let sections = [
        RailSection {
            heading: Heading::Key("places.heading.network"),
            rows: vec![remote(false)],
        },
        RailSection {
            heading: Heading::Key("places.heading.devices"),
            rows: vec![
                RailEntry {
                    group: Group::Device,
                    kind: Kind::Volume,
                    label: "USB".into(),
                    glyph: "drive",
                    ..RailEntry::default()
                },
                remote(true),
            ],
        },
    ];
    let rows = super::rail::entries(true, favourites.as_array().unwrap(), "/h", "", &sections);
    assert_eq!(
        rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
        [
            Kind::Recent,
            Kind::Favourite,
            Kind::Home,
            Kind::Share,
            Kind::Volume,
            Kind::Phone
        ]
    );
    assert_eq!(
        rows.iter().map(|row| row.glyph).collect::<Vec<_>>(),
        [
            "history",
            "network",
            "house",
            "server",
            "drive",
            "smartphone"
        ]
    );
    assert!(
        rows.iter()
            .all(|row| crate::g2::rail_places::rows::glyph::svg(row.glyph).is_some()),
        "every rail mark must resolve to actual SVG data"
    );
}

type Pending = (Vec<String>, async_channel::Sender<Event>);
#[derive(Default)]
struct Recorder(RefCell<Vec<Pending>>, RefCell<Vec<KillProbe>>);
impl Runner for Recorder {
    fn start(&self, job: Job) -> Handle {
        let (handle, sender, probe) = Handle::probed();
        self.0.borrow_mut().push((job.argv, sender));
        self.1.borrow_mut().push(probe);
        handle
    }
}
impl Recorder {
    fn answer(&self, head: &[&str], code: i32, stdout: &str) {
        let mut jobs = self.0.borrow_mut();
        let index = jobs
            .iter()
            .position(|(argv, _)| {
                (head != ["gio", "mount"] || argv.get(2).is_none_or(|arg| arg != "-li"))
                    && argv
                        .iter()
                        .map(String::as_str)
                        .take(head.len())
                        .eq(head.iter().copied())
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing job {head:?}: {:?}",
                    jobs.iter().map(|job| &job.0).collect::<Vec<_>>()
                )
            });
        let (_, sender) = jobs.remove(index);
        sender
            .try_send(Event::Exited(Outcome {
                code,
                stdout: stdout.into(),
                stderr: String::new(),
            }))
            .unwrap();
    }
}

fn opening(cx: &mut TestAppContext) -> (Fixture<'_>, Rc<Recorder>) {
    let runner = Rc::new(Recorder::default());
    let recording = runner.clone();
    cx.update(|cx| {
        cx.set_global(Services {
            runner: recording,
            hostname: "testhost".into(),
        })
    });
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.update(|picker, cx| picker.start_places(&Json::Null, cx));
    f.cx.run_until_parked();
    runner.answer(&["lsblk"], 0, r#"{"blockdevices":[]}"#);
    runner.answer(&["gio", "mount", "-li"], 0, "");
    f.cx.run_until_parked();
    (f, runner)
}

#[gpui::test]
fn rail2_share_and_phone_mount_then_open_the_resolved_folder(cx: &mut TestAppContext) {
    let (mut f, runner) = opening(cx);
    for phone in [false, true] {
        let row = remote(phone);
        let uri = row.uri.clone();
        f.update(|picker, cx| {
            picker.rail.entries.push(row);
            picker.rail_choose(picker.rail.entries.len() - 1, cx);
        });
        f.cx.run_until_parked();
        assert!(
            runner
                .0
                .borrow()
                .iter()
                .any(|(argv, _)| argv.get(1).is_some_and(|arg| arg == "mount")
                    && argv.last() == Some(&uri)),
            "{uri} must mount before info or listing"
        );
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            if phone { "/fixture/share" } else { "/d" }
        );
        runner.answer(&["gio", "mount"], 0, "");
        f.cx.run_until_parked();
        let path = if phone {
            "/fixture/phone"
        } else {
            "/fixture/share"
        };
        runner.answer(&["gio", "info"], 0, &format!("local path: {path}\n"));
        f.cx.run_until_parked();
        assert_eq!(f.read(|picker| picker.listing.location.clone()), path);
    }
}

#[gpui::test]
fn rail2_saved_shares_and_live_phones_arrive_from_the_shared_poll(cx: &mut TestAppContext) {
    let (mut f, runner) = opening(cx);
    let bookmarks = f.root.0.join("bookmarks");
    std::fs::write(&bookmarks, "smb://nas/docs Saved NAS\n").unwrap();
    f.update(|picker, cx| {
        picker.rail.network.as_mut().unwrap().bookmarks = bookmarks;
        let state =
            jsondoc::parse(r#"{"places":{"showDevices":false,"showNetwork":false}}"#).unwrap();
        picker.rail_favourites(&state, cx);
        picker.refresh_mounts(cx);
    });
    // GIO's MTP volume is not an lsblk device; the root mount confirms its green square.
    runner.answer(&["gio", "mount", "-li"], 0,
        "Mount(0): NAS -> smb://nas/docs\nVolume(0): Pixel\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://pixel/\n  can_mount=1\nMount(1): Pixel -> mtp://pixel/\n");
    f.settle(|f| {
        f.read(|picker| {
            picker
                .rail
                .labels
                .iter()
                .any(|label| label.as_ref() == "Pixel")
        })
    });
    let rows = f.read(|picker| picker.rail.entries.clone());
    let tail = &rows[rows.len() - 2..];
    assert_eq!(
        tail.iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
        ["Saved NAS", "Pixel"]
    );
    assert_eq!(
        tail.iter().map(|row| row.glyph).collect::<Vec<_>>(),
        ["server", "smartphone"]
    );
    assert!(tail
        .iter()
        .all(|row| super::rail_view::dot(row) == Some(true)));
    assert_eq!(
        rows.iter()
            .filter(|row| row.uri == "smb://nas/docs")
            .count(),
        1,
        "a live share and its bookmark merge"
    );
}

#[gpui::test]
fn rail2_failure_says_one_line_and_navigation_retires_a_pending_mount(cx: &mut TestAppContext) {
    let (mut f, runner) = opening(cx);
    f.update(|picker, cx| {
        picker.rail.entries.push(remote(false));
        picker.rail_choose(picker.rail.entries.len() - 1, cx);
    });
    runner.answer(&["gio", "mount"], 1, "");
    f.cx.run_until_parked();
    runner.answer(&["gio", "info"], 1, "");
    f.cx.run_until_parked();
    let message = f.read(|picker| picker.message.clone()).unwrap();
    assert!(message.error && !message.text.is_empty() && message.text.lines().count() == 1);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
    f.update(|picker, cx| {
        picker.rail.entries.push(remote(true));
        picker.rail_choose(picker.rail.entries.len() - 1, cx);
    });
    f.cx.run_until_parked();
    f.update(|picker, cx| picker.open_folder("/srv/elsewhere", cx));
    f.cx.run_until_parked();
    assert!(
        runner.1.borrow().last().unwrap().killed(),
        "a folder change retires the phone mount"
    );
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/srv/elsewhere"
    );
}

fn recent_request(f: &mut Fixture, after: usize) -> String {
    f.settle(|f| {
        f.listing
            .sent()
            .iter()
            .skip(after)
            .any(|line| line.contains("listpaths"))
    });
    f.listing
        .sent()
        .into_iter()
        .skip(after)
        .find(|line| line.contains("listpaths"))
        .unwrap()
}

#[gpui::test]
fn rail2_recent_uses_current_filter_and_reloads_when_changed(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d","filters":[{"label":"Text","globs":["*.txt"]},{"label":"Images","mimes":["image/png"]}],"current":"Images"}"#,
        None,
    );
    let files = ["note.txt", "photo.png", "folder"];
    for name in files {
        let path = f.root.0.join(name);
        if name == "folder" {
            std::fs::create_dir(path).unwrap();
        } else {
            std::fs::write(path, "fixture").unwrap();
        }
    }
    let history = f.root.0.join("recently-used.xbel");
    let bookmarks = files
        .map(|name| {
            format!(
                r#"<bookmark href="file://{}" visited="2026-10-01T10:00:00Z"/>"#,
                f.root.0.join(name).display()
            )
        })
        .join("");
    std::fs::write(&history, format!("<xbel>{bookmarks}</xbel>")).unwrap();
    let start = f.listing.sent().len();
    f.update(|picker, cx| {
        picker.rail.history = history.to_string_lossy().into_owned();
        picker.open_folder(flea::ui_logic_picker_portal::RECENT, cx);
    });
    let first = recent_request(&mut f, start);
    assert_eq!(
        flea::json::field_str_array(&first, "pickerMimes"),
        ["image/png"],
        "current MIME filter: {first}"
    );
    assert_eq!(
        flea::json::field_str_array(&first, "paths").len(),
        3,
        "folders reach the backend's directory-preserving filter"
    );
    let start = f.listing.sent().len();
    f.update(|picker, cx| picker.choose_filter(0, cx));
    let next = recent_request(&mut f, start);
    assert_eq!(
        flea::json::field_str_array(&next, "pickerGlobs"),
        ["*.txt"],
        "changed glob filter applies to Recent: {next}"
    );
    assert!(flea::json::field_str_array(&next, "pickerMimes").is_empty());
    let start = f.listing.sent().len();
    f.update(|picker, cx| picker.choose_filter(-1, cx));
    let all = recent_request(&mut f, start);
    assert!(flea::json::field_str_array(&all, "pickerGlobs").is_empty());
    assert!(flea::json::field_str_array(&all, "pickerMimes").is_empty());
}

#[test]
fn rail2_recent_request_is_allowed_through_the_real_listing_transport() {
    assert!(super::process::LISTING.contains(&"listpaths"));
}

#[path = "fixlogic2_picker_tests.rs"]
mod fixlogic2_picker;
