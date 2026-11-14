//! Issue 354: a click on a parent crumb of the chooser's path strip follows Settings > View "Address bar".
use super::pointer_tests::seed;
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{point, px, Modifiers, TestAppContext};

// Issue 354: with a breadcrumb address bar a parent crumb opens its folder on one click; with a path one it opens the entry.
fn check_crumb(cx: &mut TestAppContext, save: bool, address: Option<&str>) {
    let request = if save {
        r#"{"mode":"save","folder":"/d/sub","name":"report.pdf"}"#
    } else {
        r#"{"folder":"/d/sub"}"#
    };
    let mut f = fixture(cx, request, None);
    if let Some(address) = address {
        let settings = flea::jsondoc::parse(&format!(r#"{{"addressBar":"{address}"}}"#)).unwrap();
        f.update(|picker, cx| picker.apply_settings(&settings, cx));
    }
    seed(&mut f);
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let (bounds, crumbs, at) = f.cx.update(|_, cx| {
        let view = path.read(cx);
        let strip = &view.strips[0];
        let crumbs: Vec<String> = strip.crumbs().iter().map(|c| c.path.clone()).collect();
        let at = crumbs.iter().position(|path| path == "/d").unwrap();
        let bounds = view.paint.borrow().strips[0].unwrap();
        (bounds, crumbs, strip.extent(at).unwrap())
    });
    assert_eq!(crumbs, ["/", "/d", "/d/sub"]);
    let sent = f.listing.sent().len();
    let x = bounds.left() + px(at.0 + at.1 / 2.);
    f.cx.simulate_click(point(x, bounds.center().y), Modifiers::none());
    f.cx.run_until_parked();
    if address == Some("path") {
        f.cx.update(|window, cx| {
            let path = path.read(cx);
            assert!(
                path.holds_keyboard(window),
                "a path address bar opens the entry on a parent crumb"
            );
            assert_eq!(path.entry.text, "/d/sub/");
        });
        assert!(
            !f.listing.sent()[sent..]
                .iter()
                .any(|line| line.contains(r#""c":"list""#) || line.contains(r#""c":"peek""#)),
            "a path address bar must not navigate: {:?}",
            &f.listing.sent()[sent..]
        );
        assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/sub");
        assert!(f.read(|picker| picker.chrome.history.is_empty()));
        return;
    }
    assert!(
        !f.cx.update(|_, cx| path.read(cx).editing()),
        "a parent crumb does not open the entry"
    );
    assert!(
        f.listing.sent()[sent..]
            .iter()
            .any(|line| line.contains(r#""c":"list""#) && line.contains(r#""/d""#)),
        "the parent crumb lists its folder: {:?}",
        &f.listing.sent()[sent..]
    );
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
    assert_eq!(f.read(|picker| picker.chrome.history.clone()), ["/d/sub"]);
    assert!(!f.read(|picker| picker.answered()));
    if save {
        assert_eq!(
            f.read(|picker| picker.listing.save_name.clone()),
            "report.pdf"
        );
    }
}

#[gpui::test]
fn picker_open_parent_crumb_navigates(cx: &mut TestAppContext) {
    check_crumb(cx, false, None);
}

#[gpui::test]
fn picker_breadcrumb_address_bar_parent_crumb_navigates(cx: &mut TestAppContext) {
    check_crumb(cx, false, Some("breadcrumb"));
}

#[gpui::test]
fn picker_path_address_bar_parent_crumb_opens_the_entry(cx: &mut TestAppContext) {
    check_crumb(cx, false, Some("path"));
}

#[gpui::test]
fn picker_path_address_bar_save_keeps_the_entry_and_filename(cx: &mut TestAppContext) {
    check_crumb(cx, true, Some("path"));
}

#[gpui::test]
fn picker_save_parent_crumb_navigates_and_keeps_filename(cx: &mut TestAppContext) {
    check_crumb(cx, true, None);
}

// Every crumb's target, and crumb at's centre on the drawn strip.
fn strip(f: &mut Fixture, at: usize) -> (Vec<String>, gpui::Point<gpui::Pixels>) {
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    f.cx.update(|_, cx| {
        let view = path.read(cx);
        let strip = &view.strips[0];
        let crumbs = strip.crumbs().iter().map(|c| c.path.clone()).collect();
        let bounds = view.paint.borrow().strips[0].unwrap();
        let (left, width) = strip.extent(at).unwrap();
        let x = bounds.left() + px(left + width / 2.);
        (crumbs, point(x, bounds.center().y))
    })
}

// A space, punctuation and a non-ASCII name each stay whole in the crumb's target and in the folder it lists.
#[gpui::test]
fn picker_parent_crumb_keeps_spaces_punctuation_and_unicode(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/t/space name/#[draft]%/雪"}"#, None);
    f.cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":0,"path":"/t/space name/#[draft]%/雪"}"#.into(),
    ));
    f.cx.run_until_parked();
    let (crumbs, at) = strip(&mut f, 3);
    assert_eq!(
        crumbs,
        [
            "/",
            "/t",
            "/t/space name",
            "/t/space name/#[draft]%",
            "/t/space name/#[draft]%/雪"
        ]
    );
    let sent = f.listing.sent().len();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/t/space name/#[draft]%"
    );
    assert!(
        f.listing.sent()[sent..]
            .iter()
            .any(|line| line.contains(r#""c":"list""#)
                && line.contains(r#""/t/space name/#[draft]%""#)),
        "the crumb lists its exact folder: {:?}",
        &f.listing.sent()[sent..]
    );
    let (_, at) = strip(&mut f, 2);
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/t/space name"
    );
}

// Recent is a location and never a path: its strip is one label, and a click on it navigates nowhere.
#[gpui::test]
fn picker_recent_label_is_one_crumb_that_never_navigates(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    seed(&mut f);
    f.update(|picker, cx| picker.open_folder(flea::ui_logic_picker_portal::RECENT, cx));
    f.cx.run_until_parked();
    let (crumbs, at) = strip(&mut f, 0);
    assert_eq!(crumbs, [flea::ui_logic_picker_portal::RECENT]);
    let history = f.read(|picker| picker.chrome.history.clone());
    let sent = f.listing.sent().len();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        flea::ui_logic_picker_portal::RECENT
    );
    assert_eq!(f.read(|picker| picker.chrome.history.clone()), history);
    assert!(
        !f.listing.sent()[sent..]
            .iter()
            .any(|line| line.contains(r#""c":"list""#) || line.contains(r#""c":"peek""#)),
        "Recent's label must not navigate: {:?}",
        &f.listing.sent()[sent..]
    );
}
