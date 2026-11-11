//! Every language Omarchy ships, switched live in turn: each rail row kind and heading reads its catalog word, data rows keep their names.
use super::*;
use flea::ui_logic_rail_places::{
    order::{Heading, RailSection},
    rail::{Group, RailEntry},
};
use std::{fs, path::PathBuf, time::Duration};

struct Omarchy(PathBuf);

impl Omarchy {
    // The pinned roster, and an Omarchy keybindings catalog per language so the NETWORK heading's reference resolves in each.
    fn new() -> Self {
        let made = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-rail-every-language.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        fs::write(root.join(".flea-test-sandbox"), "rail every language").unwrap();
        let roster = include_str!("../../../../src/i18n/fixtures/locales.json");
        fs::create_dir_all(root.join("default/i18n")).unwrap();
        fs::write(root.join("default/i18n/locales.json"), roster).unwrap();
        fs::write(root.join("language"), "en\n").unwrap();
        for code in codes(roster) {
            let directory = root.join("default/i18n").join(&code);
            fs::create_dir_all(&directory).unwrap();
            let word = if code == "en" {
                "Network".to_owned()
            } else {
                format!("Network {code}")
            };
            fs::write(
                directory.join("keybindings.json"),
                format!(r#"{{"Network":"{word}"}}"#),
            )
            .unwrap();
        }
        Self(root)
    }
}

impl Drop for Omarchy {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with("/tmp"));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn codes(roster: &str) -> Vec<String> {
    match flea::jsondoc::parse(roster).unwrap() {
        flea::jsondoc::Json::Obj(members) => members.into_iter().map(|(code, _)| code).collect(),
        _ => panic!("locales.json is an object"),
    }
}

fn snapshot(updates: &async_channel::Receiver<Snapshot>) -> Snapshot {
    let updates = updates.clone();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(updates.recv_blocking());
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("live catalog worker")
        .unwrap()
}

// Languages whose catalogs once kept the English XDG place words, as upstream xdg-user-dirs leaves them untranslated.
const SEVEN: [&str; 7] = ["bn", "fil", "ms", "si", "sw", "ur", "uz"];
const XDG_ENGLISH: [(&str, &str); 9] = [
    ("places.row.desktop", "Desktop"),
    ("places.row.documents", "Documents"),
    ("places.row.downloads", "Downloads"),
    ("places.row.music", "Music"),
    ("places.row.pictures", "Pictures"),
    ("places.row.videos", "Videos"),
    ("places.row.templates", "Templates"),
    ("places.row.public", "Public"),
    ("places.row.projects", "Projects"),
];

// The drawn order the fixture below produces, each row with the key it reads or the data name it keeps.
fn expected() -> Vec<(&'static str, Result<&'static str, &'static str>)> {
    let mut rows = vec![
        ("home", Ok("places.row.home")),
        ("recent", Ok("places.row.recent")),
        ("desktop", Ok("places.row.desktop")),
        ("documents", Ok("places.row.documents")),
        ("downloads", Ok("places.row.downloads")),
        ("music", Ok("places.row.music")),
        ("pictures", Ok("places.row.pictures")),
        ("videos", Ok("places.row.videos")),
        ("templates", Ok("places.row.templates")),
        ("public", Ok("places.row.public")),
        ("projects", Ok("places.row.projects")),
        ("custom folder", Err("Office records")),
        ("trash", Ok("places.row.trash")),
        ("favourite", Err("Projects")),
        ("invalid favourite", Ok("places.favorite.invalid_label")),
    ];
    rows.extend(
        flea::ui_logic_rail_places::tags::NAMES
            .into_iter()
            .map(|name| ("tag", Ok(flea::ui_logic_rail_places::tags::label_key(name)))),
    );
    rows.extend([
        ("share", Err("NAS")),
        ("dropbox", Ok("plugins.builtin.dropbox.name")),
        ("volume", Err("Projects")),
    ]);
    rows
}

#[gpui::test]
fn every_language_switch_relabels_every_rail_row_kind(cx: &mut TestAppContext) {
    let omarchy = Omarchy::new();
    let roster = include_str!("../../../../src/i18n/fixtures/locales.json");
    let (_service, updates) = crate::window::locale::Service::start(flea::i18n::Paths {
        omarchy: omarchy.0.clone(),
        language_file: omarchy.0.join("language"),
        catalogs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../i18n")],
    })
    .unwrap();
    assert_eq!(snapshot(&updates).catalog.language(), "en");
    let (shell, cx) = open(false, "list", cx);
    shell.update(cx, |shell, cx| {
        let mut state = shell.launch.settings.data.clone();
        crate::g2::settings_pages::seams::merge(
            &mut state,
            &flea::jsondoc::parse(r#"{"places":{"showRecent":true,"showTags":true,"favourites":[{"label":"Projects","path":"/saved/Projects"},{"label":"","path":"relative"}]}}"#).unwrap(),
        );
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.update(cx, |producer, cx| {
            producer.adopt_state(&state, cx);
            producer.fixture_home(
                "/fixture",
                concat!(
                    "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\n",
                    "XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n",
                    "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n",
                    "XDG_MUSIC_DIR=\"$HOME/Music\"\n",
                    "XDG_PICTURES_DIR=\"$HOME/Pictures\"\n",
                    "XDG_VIDEOS_DIR=\"$HOME/Videos\"\n",
                    "XDG_TEMPLATES_DIR=\"$HOME/Templates\"\n",
                    "XDG_PUBLICSHARE_DIR=\"$HOME/Public\"\n",
                    "XDG_PROJECTS_DIR=\"$HOME/Projects\"\n",
                    "XDG_OFFICE_DIR=\"$HOME/Office records\"\n",
                ),
                cx,
            );
            producer.tags_seen(&[true; flea::ui_logic_rail_places::tags::NAMES.len()], cx);
            producer.set_tail(
                vec![
                    RailSection {
                        heading: Heading::Key("places.heading.network"),
                        rows: vec![
                            RailEntry { group: Group::Network, kind: Kind::Share, label: "NAS".into(), ..Default::default() },
                            RailEntry {
                                group: Group::Network,
                                kind: Kind::Dropbox,
                                label: "Dropbox".into(),
                                label_key: "plugins.builtin.dropbox.name",
                                ..Default::default()
                            },
                        ],
                    },
                    RailSection {
                        heading: Heading::Key("settings.places.devices"),
                        rows: vec![RailEntry { group: Group::Device, kind: Kind::Volume, label: "Projects".into(), ..Default::default() }],
                    },
                ],
                cx,
            );
        });
    });
    frame(cx);
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    let languages = codes(roster);
    assert_eq!(languages.len(), 50, "every language Omarchy ships");
    let mut translated_projects = 0;
    for (step, language) in languages
        .iter()
        .filter(|code| *code != "en")
        .chain(std::iter::once(&"en".to_owned()))
        .enumerate()
    {
        fs::write(omarchy.0.join("language"), format!("{language}\n")).unwrap();
        let translated = snapshot(&updates);
        let catalog = translated.catalog.clone();
        assert_eq!(catalog.language(), language);
        assert_eq!(
            translated.revision,
            step as u64 + 2,
            "{language}: one snapshot per switch"
        );
        shell.update(cx, |shell, cx| shell.apply_locale(translated, cx));
        frame(cx);
        producer.read_with(cx, |producer, _| {
            let labels = producer.shown_entries();
            let rows = expected();
            assert_eq!(labels.len(), rows.len(), "{language}: {labels:?}");
            for (index, (kind, want)) in rows.iter().enumerate() {
                let want = match want {
                    Ok(key) => catalog.tr(key),
                    Err(name) => (*name).to_owned(),
                };
                assert_eq!(labels[index].as_ref(), want, "{language}: {kind} row");
            }
            // Filipino keeps the loanword Desktop, as its Firefox localisation does; every other word here is translated.
            if SEVEN.contains(&language.as_str()) {
                for (key, english) in XDG_ENGLISH {
                    let index = rows.iter().position(|(_, want)| *want == Ok(key)).unwrap();
                    if (language.as_str(), english) != ("fil", "Desktop") {
                        assert_ne!(
                            labels[index].as_ref(),
                            english,
                            "{language}: {english} reads its own word"
                        );
                    }
                }
            }
            let projects = catalog.tr("places.row.projects");
            assert_ne!(
                projects, "places.row.projects",
                "{language}: the catalog holds the Projects key"
            );
            // Read beside the loader, so a fallback to English cannot stand in for this language's word.
            let file = fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../i18n/{language}/flea.json")),
            )
            .unwrap();
            assert!(
                file.contains(&format!("\"places.row.projects\": \"{projects}\"")),
                "{language}: {projects:?} is not this catalog's Projects word"
            );
            if projects != "Projects" {
                translated_projects += 1;
            }
            assert_eq!(
                producer.provider.drawn.len(),
                5,
                "{language}: five sections"
            );
            for (section, key) in producer.provider.drawn.iter().zip([
                "places.heading.places",
                "places.heading.favorites",
                "places.heading.tags",
                "places.heading.network",
                "settings.places.devices",
            ]) {
                assert_eq!(
                    section.heading,
                    crate::g2::rail_places::content::heading_text(&catalog, &Heading::Key(key)),
                    "{language}: {key}"
                );
            }
            let network = &producer.provider.drawn[3].heading;
            let word = if language == "en" {
                "Network".to_owned()
            } else {
                format!("Network {language}")
            };
            assert!(
                network.to_lowercase().contains(&word.to_lowercase()),
                "{language}: NETWORK reads Omarchy's word, got {network}"
            );
            let painted = producer.laid.borrow();
            assert!(
                painted.painted && !painted.rows.is_empty(),
                "{language}: rows painted"
            );
            for row in &painted.rows {
                assert_eq!(
                    row.label,
                    labels[row.index].as_ref(),
                    "{language}: painted row {}",
                    row.index
                );
            }
        });
    }
    // Every catalog but English translates Projects: 42 from xdg-user-dirs 0.20, the seven above from their own desktop sources.
    assert_eq!(translated_projects, 49);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
