//! Painted regression for HIG locked-menu, distinct from an enabled-menu prerequisite.
use super::*;
use flea::jsondoc::Json;

fn number(value: &Json, key: &str) -> f64 {
    value.get(key).unwrap().as_f64().unwrap()
}

#[gpui::test]
fn notice_layout_hidden_rows_keep_the_complete_hig_sentence(cx: &mut TestAppContext) {
    let handle = locked(cx, Some(super::HIDDEN_0_4_0));
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    let footer = json(cx, handle, "statusFooterState");
    println!(
        "locked-menu painted footer: {}",
        flea::jsondoc::render_at(&footer, 0)
    );
    let primary = footer.get("centre").unwrap();
    let hint = footer.get("secondary").unwrap();
    assert_eq!(footer.get("frame").unwrap().as_str(), Some("0 673 1100 27"));
    handle
        .update(cx, |shell, _, _| {
            let face = shell.active_typeface();
            assert_eq!(f32::from(face.body), 14.);
            assert!(!shell.menu_opened());
        })
        .unwrap();
    assert_eq!(
        primary.get("text").unwrap().as_str(),
        Some("Open in Terminal, Permissions and Copy As are hidden in Settings > Menus.")
    );
    assert_eq!(hint.get("text").unwrap().as_str(), Some(" · esc dismisses"));
    assert_eq!(number(primary, "fontSize"), 12.);
    assert_eq!(
        primary.get("truncated").unwrap().as_bool(),
        Some(false),
        "HIG locked-menu requires the painted sentence in full: {primary:?}"
    );
    assert!(number(primary, "width") >= number(primary, "implicitWidth") - 1.);
    assert!(number(hint, "width") >= number(hint, "implicitWidth") - 1.);
    assert!(number(hint, "x") >= number(primary, "x") + number(primary, "width") - 1.);
    let left = footer.get("left").unwrap();
    let disk = footer.get("disk").unwrap();
    assert!(number(primary, "x") >= number(left, "x") + number(left, "implicitWidth") + 14. - 1.);
    assert!(
        (number(hint, "x") + number(hint, "width") + 14.
            - (number(disk, "x") + number(disk, "width") - number(disk, "implicitWidth")))
        .abs()
            <= 1.
    );
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
    assert_eq!(primary.get("color"), left.get("color"));
    assert_ne!(hint.get("color"), primary.get("color"));
}

#[gpui::test]
fn notice_layout_long_counts_and_utf8_errors_keep_their_own_space(cx: &mut TestAppContext) {
    let handle = open(cx);
    let sentence = format!(
        "Permission denied: /資料/{}résumé.txt",
        "long-folder/".repeat(40)
    );
    handle
        .update(cx, |shell, _, cx| {
            let pane = shell.panes[0].as_mut().unwrap();
            let held = &pane.pane.held;
            pane.pane.held = std::rc::Rc::new(crate::list::pane::Held {
                total: usize::MAX,
                listing: held.listing,
                start: held.start,
                rows: held.rows.clone(),
            });
            pane.selected = (0..1000).collect();
            shell.status_messages(cx, |messages, now| {
                messages.say(
                    &sentence,
                    true,
                    "",
                    flea::ui_logic_status_feedback::status::Hint::None,
                    now,
                );
            });
        })
        .unwrap();
    for width in [600., 900.] {
        cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(width), gpui::px(700.)));
        frame(cx, handle);
        let footer = json(cx, handle, "statusFooterState");
        println!(
            "long-count UTF-8 painted footer: {}",
            flea::jsondoc::render_at(&footer, 0)
        );
        let left = footer.get("left").unwrap();
        let primary = footer.get("centre").unwrap();
        let hint = footer.get("secondary").unwrap();
        let lane = footer.get("lane").unwrap();
        assert!(
            number(left, "implicitWidth") > number(left, "width"),
            "count must exercise its cap"
        );
        assert!(number(lane, "x") >= number(left, "x") + number(left, "width") + 14. - 1.);
        assert_eq!(
            primary.get("text").unwrap().as_str(),
            Some(sentence.as_str())
        );
        assert_eq!(primary.get("truncated").unwrap().as_bool(), Some(true));
        assert_eq!(hint.get("text").unwrap().as_str(), Some(" · esc dismisses"));
        assert!(number(hint, "width") >= number(hint, "implicitWidth") - 1.);
        assert!(number(hint, "x") >= number(lane, "x") - 1.);
        assert!(
            number(hint, "x") + number(hint, "width")
                <= number(lane, "x") + number(lane, "width") + 1.
        );
        assert_ne!(primary.get("color"), left.get("color"));
        assert_eq!(read(cx, handle, "lastMessage"), Reply::value(&sentence));
        assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    }
}

#[gpui::test]
fn notice_layout_localized_explanation_preserves_text_and_roles(cx: &mut TestAppContext) {
    let handle = open(cx);
    let sentence = "无法打开此文件夹。Les actions sont masquées dans Paramètres > Menus.";
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    handle
        .update(cx, |shell, _, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.explain(
                    sentence,
                    flea::ui_logic_status_feedback::status::Hint::Dismiss,
                    now,
                );
            });
        })
        .unwrap();
    frame(cx, handle);
    let footer = json(cx, handle, "statusFooterState");
    let primary = footer.get("centre").unwrap();
    let hint = footer.get("secondary").unwrap();
    assert_eq!(primary.get("text").unwrap().as_str(), Some(sentence));
    assert_eq!(primary.get("truncated").unwrap().as_bool(), Some(false));
    assert_eq!(
        primary.get("color"),
        footer.get("left").unwrap().get("color")
    );
    assert_ne!(hint.get("color"), primary.get("color"));
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value(sentence));
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
}
