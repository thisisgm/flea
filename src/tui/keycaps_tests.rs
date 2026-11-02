use super::{Fault, Identity, Keycaps, Status};
use crate::tui::input::Key;
use crate::tui::keymap::{Hints, Map};
use crate::tui::model::Model;
use crate::tui::render::{
    frame_tests::{caps_language, model},
    key_sheet,
};
use flea::keys::display::{KeycapError, KeycapSnapshot};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Answer = Arc<Mutex<VecDeque<Result<Vec<u8>, String>>>>;
type Epoch = (
    Option<(&'static str, &'static str, u32, u64)>,
    HashMap<String, String>,
    String,
);

// A Hyprland stand-in on the production adapter: each readback takes the next queued devices answer.
struct Session {
    answer: Answer,
    streams: Arc<Mutex<VecDeque<UnixStream>>>,
    socket: Option<UnixStream>,
    keycaps: Keycaps,
    m: Model,
    map: Map,
}

fn devices(layout: &str, variant: &str, index: Option<u32>) -> Vec<u8> {
    let index = index.map_or(String::new(), |index| {
        format!(",\"active_layout_index\":{index}")
    });
    format!(
        r#"{{"mice":[],"keyboards":[{{"name":"other","layout":"us","variant":"","main":false}},{{"name":"kb","layout":"{layout}","variant":"{variant}","active_keymap":"ignored"{index},"main":true}}]}}"#
    )
    .into_bytes()
}

// The peer end the test writes socket2 lines into; dropping it closes the subscription.
fn connect(streams: &Mutex<VecDeque<UnixStream>>) -> UnixStream {
    let (ours, theirs) = UnixStream::pair().unwrap();
    streams.lock().unwrap().push_back(theirs);
    ours
}

fn session(answer: Vec<u8>, preset: &str) -> Session {
    let answer: Answer = Arc::new(Mutex::new(VecDeque::from([Ok(answer)])));
    let streams: Arc<Mutex<VecDeque<UnixStream>>> = Arc::default();
    let socket = Some(connect(&streams));
    let (reply, handed) = (answer.clone(), streams.clone());
    let keycaps = Keycaps::at(
        Arc::new(move || {
            let mut queue = reply.lock().unwrap();
            if queue.len() > 1 {
                queue.pop_front();
            }
            queue[0].clone()
        }),
        Box::new(move || handed.lock().unwrap().pop_front()),
    );
    let (mut m, map) = (model(), Map::load());
    m.preset = preset.into();
    m.hints = Hints::new(&map, preset);
    let mut session = Session {
        answer,
        streams,
        socket,
        keycaps,
        m,
        map,
    };
    session
        .keycaps
        .settle(&mut session.m, &session.map, Duration::from_secs(2));
    session
}

impl Session {
    fn send(&mut self, answer: Result<Vec<u8>, String>, line: &str) {
        self.answer.lock().unwrap().push_back(answer);
        if !line.is_empty() {
            let socket = self.socket.as_mut().unwrap();
            socket.write_all(format!("{line}\n").as_bytes()).unwrap();
        }
    }
    // Applies readings the way the loop does until the condition holds, or fails after two seconds.
    fn pump(&mut self, done: impl Fn(&Status, &Model) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !done(&self.keycaps.status, &self.m) {
            assert!(
                Instant::now() < deadline,
                "no reading arrived: {:?}",
                self.keycaps.status
            );
            std::thread::sleep(Duration::from_millis(2));
            self.keycaps.apply(&mut self.m, &self.map);
        }
    }
    fn state(&self) -> Epoch {
        let identity = self
            .m
            .keycaps
            .map(|s| (s.layout(), s.variant(), s.group(), s.generation()));
        (
            identity,
            self.m.keycap_labels.clone(),
            self.m.hints.keys.clone(),
        )
    }
    // The complete epoch the setter itself publishes for the current snapshot.
    fn whole(&self) -> bool {
        let snapshot = self.m.keycaps.unwrap();
        let mut fresh = model();
        fresh.preset = self.m.preset.clone();
        fresh.set_keycaps(snapshot, &self.map).unwrap();
        let sheet = key_sheet::resolve(&fresh, &self.map, &snapshot).unwrap();
        sheet == self.m.keycap_labels
            && fresh.keycap_labels == self.m.keycap_labels
            && fresh.hints.keys == self.m.hints.keys
    }
    fn relabelled(&self) -> usize {
        self.m
            .keycap_labels
            .iter()
            .filter(|(canonical, label)| canonical != label)
            .count()
    }
}

fn group(s: &Session) -> Option<(&'static str, &'static str, u32, u64)> {
    s.state().0
}

// French (Canada) and Colemak relabel, because French, Belgian and Dvorak mark "-" ambiguous for some presets.
#[test]
fn ey_tui_keycaps_startup_publishes_the_active_group_with_its_complete_epoch() {
    let s = session(devices("us,ca", "", Some(1)), "default");
    assert_eq!(s.keycaps.status, Status::Active);
    assert_eq!(group(&s), Some(("ca", "", 1, 1)));
    assert!(s.whole());
    assert!(
        s.relabelled() > 0,
        "French (Canada) moves punctuation caps: {:?}",
        s.m.keycap_labels
    );
}

#[test]
fn ey_tui_keycaps_a_group_or_variant_switch_relabels_the_same_session() {
    let mut s = session(devices("us,ca", ",", Some(0)), "default");
    assert_eq!((group(&s), s.relabelled()), (Some(("us", "", 0, 1)), 0));
    let us = s.m.keycap_labels.clone();
    s.m.cursor = 3;
    s.send(
        Ok(devices("us,ca", ",", Some(1))),
        "activelayout>>kb,French (Canada)",
    );
    s.pump(|_, m| m.keycaps.is_some_and(|k| k.generation() == 2));
    assert_eq!(group(&s), Some(("ca", "", 1, 2)));
    assert!(s.whole() && s.relabelled() > 0);
    let ca = s.m.keycap_labels.clone();
    s.send(
        Ok(devices("de,us", ",colemak", Some(1))),
        "configreloaded>>",
    );
    s.pump(|_, m| m.keycaps.is_some_and(|k| k.generation() == 3));
    assert_eq!(group(&s), Some(("us", "colemak", 1, 3)));
    assert!(s.whole());
    assert!(
        s.m.keycap_labels != us && s.m.keycap_labels != ca,
        "Colemak's caps replace the whole epoch"
    );
    assert_eq!(s.m.cursor, 3, "a relabel never moves the selection");
    let j = Key::character('j', "");
    assert_eq!(
        s.map.action(&j, "default"),
        "cursorDown",
        "canonical keys still dispatch"
    );
}

#[test]
fn ey_tui_keycaps_an_unchanged_group_keeps_its_generation() {
    let named = br#"{"keyboards":[{"name":"kb","layout":"ca","active_keymap":"French (Canada)","main":true}]}"#;
    let mut s = session(named.to_vec(), "default");
    assert_eq!(group(&s), Some(("ca", "", 0, 1)));
    let before = s.state();
    for _ in 0..3 {
        s.send(Ok(named.to_vec()), "activelayout>>kb,French (Canada)");
    }
    let socket = s.socket.as_mut().unwrap();
    socket.write_all(b"monitoradded>>DP-2\n").unwrap();
    // Each readback takes the next queued answer, so the US one is read only after the three unchanged ones.
    s.send(
        Ok(devices("ca,us", ",", Some(1))),
        "activelayout>>kb,English (US)",
    );
    s.pump(|_, m| m.keycaps.is_some_and(|k| k.layout() == "us"));
    assert_eq!(
        group(&s),
        Some(("us", "", 1, 2)),
        "the unchanged readings published nothing"
    );
    assert_ne!(s.state(), before);
}

#[test]
fn ey_tui_keycaps_a_language_change_never_selects_the_layout() {
    let mut s = session(devices("ru", "", Some(0)), "default");
    let before = s.state();
    assert_eq!(before.0, Some(("ru", "", 0, 1)));
    for language in ["tr", "el"] {
        s.m.i18n = caps_language(language);
        crate::tui::preview::refresh_locale(&mut s.m);
        s.keycaps.apply(&mut s.m, &s.map);
        assert_eq!(s.state(), before, "{language}");
    }
}

#[test]
fn ey_tui_keycaps_ambiguous_groups_relabel_and_unknown_groups_keep_the_last_epoch() {
    // French marks "-" ambiguous; the yazi sheet shows it, so that chord keeps its canonical legend while the rest relabel.
    let mut s = session(devices("ca", "", Some(0)), "yazi");
    let before = s.state();
    assert_eq!(before.0, Some(("ca", "", 0, 1)));
    let fr = KeycapSnapshot::new("fr", None, Some(0), 1).unwrap();
    assert!(fr.ambiguous().contains(&"-"));
    s.send(Ok(devices("fr", "", Some(0))), "activelayout>>kb,French");
    s.pump(|_, m| m.keycaps.is_some_and(|k| k.generation() == 2));
    assert_eq!(s.keycaps.status, Status::Active);
    assert_eq!(group(&s), Some(("fr", "", 0, 2)));
    assert!(s.whole() && s.relabelled() > 0);
    let dash = |text: &str| text.split([' ', '/']).any(|part| part == "-");
    assert!(s
        .m
        .keycap_labels
        .iter()
        .any(|(canonical, label)| dash(canonical) && dash(label)));
    let published = s.state();
    s.send(Ok(devices("zz", "", Some(0))), "activelayout>>kb,unknown");
    s.pump(|status, _| {
        matches!(
            status,
            Status::Failed(Fault::Keycap(KeycapError::UnknownLayout { .. }))
        )
    });
    assert_eq!(s.state(), published);
}

#[test]
fn ey_tui_keycaps_malformed_refused_and_disconnected_sources_keep_the_last_epoch() {
    let mut s = session(devices("ca", "", Some(0)), "default");
    let before = s.state();
    assert_eq!(before.0, Some(("ca", "", 0, 1)));
    s.send(
        Ok(b"{\"keyboards\":".to_vec()),
        "activelayout>>kb,French (Canada)",
    );
    s.pump(|status, _| matches!(status, Status::Failed(Fault::Malformed(_))));
    assert_eq!(s.state(), before);
    s.send(
        Err("Connection refused".into()),
        "activelayout>>kb,French (Canada)",
    );
    s.pump(|status, _| matches!(status, Status::Failed(Fault::Query(_))));
    assert_eq!(s.state(), before);
    // The compositor restarts: the subscription closes, then a new one re-reads the same group.
    s.send(Ok(devices("ca", "", Some(0))), "");
    s.socket = None;
    s.pump(|status, _| *status == Status::Failed(Fault::Disconnected));
    assert_eq!(s.state(), before);
    s.socket = Some(connect(&s.streams));
    s.pump(|status, _| *status == Status::Active);
    assert_eq!(
        s.state(),
        before,
        "the same group after a reconnect keeps its generation"
    );
}

#[test]
fn ey_tui_keycaps_resume_reads_the_group_again() {
    let mut s = session(devices("us", "", Some(0)), "default");
    assert_eq!(group(&s), Some(("us", "", 0, 1)));
    // Switched while the terminal was stopped: no socket2 line reaches the loop, only the resume.
    s.send(Ok(devices("us,ca", ",", Some(1))), "");
    s.keycaps.resume();
    s.pump(|_, m| m.keycaps.is_some_and(|k| k.generation() == 2));
    assert_eq!(group(&s), Some(("ca", "", 1, 2)));
    assert!(s.whole());
}

#[test]
fn ey_tui_keycaps_an_unsupported_source_keeps_canonical_labels_without_claiming_a_layout() {
    let map = Map::load();
    for fault in [Fault::Remote, Fault::NoCompositor] {
        let mut keycaps = Keycaps::unsupported(fault.clone());
        let mut m = model();
        let hints = m.hints.keys.clone();
        keycaps.settle(&mut m, &map, Duration::from_secs(2));
        keycaps.resume();
        keycaps.apply(&mut m, &map);
        assert_eq!(
            (m.keycaps, m.keycap_labels.is_empty(), m.hints.keys),
            (None, true, hints)
        );
        assert_eq!(keycaps.status, Status::Failed(fault));
    }
}

#[test]
fn ey_tui_keycaps_devices_answers_name_the_main_keyboards_group() {
    let read = |fields: &str| {
        super::identity(format!(r#"{{"keyboards":[{{"name":"kb",{fields}}}]}}"#).as_bytes())
    };
    let identity = |layout: &str, variant: &str, group| {
        Ok(Identity {
            layout: layout.into(),
            variant: variant.into(),
            group,
        })
    };
    let unknown = |name: &str| Err(Fault::Keycap(KeycapError::UnknownGroupName(name.into())));
    // Without active_layout_index the XKB description picks the configured group.
    let german = r#""layout":"us,de","variant":",","active_keymap":"German""#;
    assert_eq!(read(german), identity("us,de", ",", 1));
    let colemak = r#""layout":"us,us","variant":",colemak","active_keymap":"English (Colemak)""#;
    assert_eq!(read(colemak), identity("us,us", ",colemak", 1));
    assert_eq!(
        read(r#""layout":"il","active_layout_index":0"#),
        identity("il", "", 0)
    );
    let hindi = r#""layout":"us","active_keymap":"Hindi (Bolnagri)""#;
    assert_eq!(read(hindi), unknown("Hindi (Bolnagri)"));
    assert_eq!(
        read(r#""layout":"us","active_keymap":"German""#),
        unknown("German")
    );
    for malformed in [
        r#""layout":"us,fr","active_layout_index":7"#,
        r#""main":true"#,
    ] {
        assert!(
            matches!(read(malformed), Err(Fault::Malformed(_))),
            "{malformed}"
        );
    }
    for answer in [&br#"{"keyboards":[]}"#[..], b"ok", b"\xff"] {
        assert!(matches!(super::identity(answer), Err(Fault::Malformed(_))));
    }
    for line in [&b"activelayout>>kb,French\n"[..], b"configreloaded>>\n"] {
        assert!(super::changes_layout(line));
    }
    for line in [&b"activewindow>>kitty,flea\n"[..], b"configreloaded>>x\n"] {
        assert!(!super::changes_layout(line));
    }
}
