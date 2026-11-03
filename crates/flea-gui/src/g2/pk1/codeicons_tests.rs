use super::*;
use crate::{
    row::marks::{Language, Mark},
    window::settings::store::{Update, Worker},
};

#[gpui::test]
fn codeicons_language_picker_settings_update_drawn_rows(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let (send, updates) = async_channel::unbounded();
    let (writes, _queue) = std::sync::mpsc::channel();
    f.update(|picker, cx| picker.watch_settings(Worker { writes, updates }, cx));
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":1,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"main.rs","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    let listing = f.read(|picker| picker.browse.listing.clone());
    for (document, expected) in [
        ("{}", Mark::Language(Language::for_file("main.rs").unwrap())),
        (r#"{"langMarks":false}"#, Mark::FileText),
        ("{}", Mark::Language(Language::for_file("main.rs").unwrap())),
    ] {
        send.try_send(Update::Loaded {
            document: flea::uistate::from_file(document),
            through: 0,
            saved: false,
        })
        .unwrap();
        f.cx.run_until_parked();
        f.cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            assert_eq!(
                listing.read(cx).drawn_marks(),
                [expected],
                "picker setting {document}"
            );
        });
    }
}
