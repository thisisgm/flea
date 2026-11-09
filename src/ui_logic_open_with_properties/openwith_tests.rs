// tests/js/openwith.js and opener-ask.qml's exit routing.
use super::english;
use super::opener::{Opened, Opener};
use super::openwith::*;

fn app(id: &str, label: &str, default: bool) -> App {
    App { id: id.into(), label: label.into(), icon: id.into(), default }
}

fn catalogue() -> Catalogue {
    Catalogue {
        path: "/fx/v/work/notes.txt".into(),
        kind: "PNG image".into(),
        mime: "image/png".into(),
        handlers: vec![app("viewer.desktop", "Image Viewer", true), app("chromium.desktop", "Chromium", false), app("pinta.desktop", "Pinta", false)],
        installed: vec![app("1password.desktop", "1Password", false), app("chromium.desktop", "Chromium", false), app("pinta.desktop", "Pinta", false), app("viewer.desktop", "Image Viewer", false)],
    }
}

fn labels(cat: &Catalogue, rows: &[Row]) -> String {
    let i18n = english();
    rows.iter().map(|row| match row {
        Row::Eyebrow { group, .. } => format!("[{}]", eyebrow(&i18n, *group, &cat.kind)),
        Row::App { app, group, .. } => cat.app(*group, *app).unwrap().label.clone(),
    }).collect::<Vec<_>>().join(",")
}

#[test]
fn both_groups_draw_under_their_own_eyebrow() {
    let cat = catalogue();
    let full = rows(&cat, "");
    assert_eq!(labels(&cat, &full), "[Registered for PNG image],Image Viewer,Chromium,Pinta,[All applications],1Password,Chromium,Pinta,Image Viewer");
    assert_eq!(full.iter().filter(|r| matches!(r, Row::Eyebrow { rule: true, .. })).count(), 1, "only the second eyebrow carries the rule");
    assert_eq!(applications(&full), 7, "the cursor counts applications and never an eyebrow");
    let seats: Vec<usize> = full.iter().filter_map(|r| match r { Row::App { at, .. } => Some(*at), _ => None }).collect();
    assert_eq!(seats, (0..7).collect::<Vec<_>>(), "every application row knows the seat it holds");
    let chromium: Vec<usize> = full.iter().filter_map(|r| match r { Row::App { app, group, at } if cat.app(*group, *app).unwrap().id == "chromium.desktop" => Some(*at), _ => None }).collect();
    assert_eq!(chromium, vec![1, 4], "a registered application drawn twice takes two seats");
    assert_eq!(row_of(&full, 3), Some(5), "the seat maps back to the row it was drawn in");
    assert_eq!(row_of(&full, 7), None);
    assert_eq!(chosen(&cat, &full, 0).map(|a| a.id.as_str()), Some("viewer.desktop"));
}

#[test]
fn the_search_filters_both_groups_and_drops_empty_eyebrows() {
    let cat = catalogue();
    assert_eq!(labels(&cat, &rows(&cat, "pin")), "[Registered for PNG image],Pinta,[All applications],Pinta");
    let installed_only = rows(&cat, "1pass");
    assert_eq!(labels(&cat, &installed_only), "[All applications],1Password");
    assert_eq!(installed_only[0], Row::Eyebrow { group: Group::All, rule: false }, "the surviving eyebrow drops the rule");
    let bare = Catalogue { handlers: Vec::new(), ..catalogue() };
    assert_eq!(labels(&bare, &rows(&bare, "  CHROM ")), "[All applications],Chromium", "the search ignores case and surrounding space");
    assert!(rows(&cat, "zzq").is_empty(), "nothing matched leaves no row at all");
    assert_eq!(no_match(&english(), " zzq "), "No application matches \u{201c}zzq\u{201d}. Clear the search to see every installed application.");
}

#[test]
fn seven_applications_and_their_eyebrows_set_the_viewport() {
    let cat = catalogue();
    let full = rows(&cat, "");
    assert_eq!(viewport_height(&full, 7, 37., 28.), 7. * 37. + 2. * 28.);
    assert_eq!(viewport_height(&full, 3, 37., 28.), 3. * 37. + 28.);
    let short = Catalogue { handlers: Vec::new(), installed: vec![app("a", "A", false)], ..catalogue() };
    assert_eq!(viewport_height(&rows(&short, ""), 7, 37., 28.), 7. * 37. + 28., "a short catalogue still fills it");
    assert_eq!(viewport_height(&[], 7, 37., 28.), 7. * 37., "rule 6 keeps that height when nothing matches");
}

#[test]
fn the_title_and_checkbox_read_as_the_board_writes_them() {
    let i18n = english();
    assert_eq!(title(&i18n, "notes.txt"), "Open notes.txt With");
    assert_eq!(title(&i18n, ""), "Open With");
    assert_eq!(always_label(&i18n, "Plain text document"), "Always use this application for plain text document files");
    assert_eq!(always_label(&i18n, "PNG image"), "Always use this application for PNG image files", "an initialism keeps its capitals");
    assert_eq!(always_label(&i18n, ""), "Always use this application for this file type");
    assert_eq!(eyebrow(&i18n, Group::Registered, "Plain text document"), "Registered for Plain text document");
}

#[test]
fn the_reply_parses_and_a_malformed_row_is_dropped() {
    let reply = crate::jsondoc::parse(r#"{"t":"menuaction","id":4,"op":"applications","ok":true,"applications":[{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true},{"label":"no id"}],"installed":[{"id":"a.desktop","label":"A","icon":"","default":false}],"mime":"text/plain","kind":"Plain text document","path":"/fx/v/work/notes.txt"}"#).unwrap();
    let cat = Catalogue::from_reply(&reply);
    assert_eq!(cat.handlers, vec![App { icon: "nvim".into(), ..app("nvim.desktop", "Neovim", true) }]);
    assert_eq!(cat.installed.len(), 1);
    assert_eq!((cat.kind.as_str(), cat.mime.as_str()), ("Plain text document", "text/plain"));
    assert_eq!(open_request(4, "a\"b.desktop", true), r#"{"c":"menuaction","op":"openWith","id":4,"application":"a\"b.desktop","always":true}"#);
    assert_eq!(applications_request(4), r#"{"c":"menuaction","op":"applications","id":4,"installed":true}"#);
    assert_eq!(close_request(4), r#"{"c":"menuaction","op":"close","id":4}"#);
}

#[test]
fn the_opener_routes_each_exit_and_drops_a_second_open() {
    let mut opener = Opener::default();
    let argv = opener.start("flea", &["/ask/tool.sh".into()]).unwrap();
    assert_eq!(argv, vec!["flea", "--open", "/ask/tool.sh"]);
    assert_eq!(opener.start("flea", &["/slow/second.sh".into()]), None, "a second open while one is pending is dropped");
    assert_eq!(opener.exited(Some(4), "{\"ask\":\"run\"}"), Some(Opened::AskRun { path: "/ask/tool.sh".into(), text: "{\"ask\":\"run\"}".into() }));
    assert_eq!(opener.exited(Some(4), "late"), None, "a stale exit lands nowhere");
    let cycle = |opener: &mut Opener, path: &str, status: Option<i32>, out: &str| {
        opener.start("flea", &[path.to_string()]).unwrap();
        opener.exited(status, out).unwrap()
    };
    assert_eq!(cycle(&mut opener, "/with/data.bin", Some(5), ""), Opened::AskOpenWith("/with/data.bin".into()));
    assert_eq!(cycle(&mut opener, "/missing/tool.sh", Some(2), ""), Opened::Failed("/missing/tool.sh".into()));
    assert_eq!(cycle(&mut opener, "/ok/file.txt", Some(0), ""), Opened::Done);
    assert_eq!(cycle(&mut opener, "/noline/tool.sh", Some(4), ""), Opened::Failed("/noline/tool.sh".into()), "an ask without its line is the plain failure");
    assert_eq!(cycle(&mut opener, "/ask/never.sh", None, ""), Opened::Failed("/ask/never.sh".into()), "a launch that never started fails");
    assert!(!opener.pending(), "and releases the lane");
    assert_eq!(cycle(&mut opener, "/dir/sub", Some(3), ""), Opened::Directory("/dir/sub".into()));
    let gallery = opener.start("flea", &["/i/b.png".into(), "/i/a.png".into()]).unwrap();
    assert_eq!(gallery, vec!["flea", "--open", "/i/b.png", "/i/a.png"], "a gallery keeps its order and boundaries");
}
