//! A mirrored chrome path with a Git label: each crumb answers the pointer where it is painted.
use super::super::{
    preview_tests::draw,
    test_shell::{land, open_root_at, row},
};
use super::arabic;
use gpui::{point, px, Modifiers, TestAppContext};

#[gpui::test]
fn i50_rtl_a_git_labelled_path_hovers_the_crumb_painted_under_the_pointer(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_at(cx, "/files/beta");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 1);
    arabic(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().git.paint =
                std::rc::Rc::new(crate::window::git_status::Paint {
                    label: "main · 3 changed".into(),
                    ..Default::default()
                });
            shell.sync_chrome(window, cx);
        })
    });
    draw(cx);
    let path = shell.read_with(cx, |shell, cx| {
        shell.chrome.view.as_ref().unwrap().read(cx).path.clone()
    });
    // The unit is moved whole: its logical left lands at the window width minus its logical right.
    let (centres, row) = cx.update(|window, cx| {
        let view = path.read(cx);
        assert_eq!(view.inputs.git_label, "main · 3 changed");
        let mut paint = view.paint.borrow_mut();
        let bounds = paint.strips[0].expect("a painted chrome strip");
        let left =
            f32::from(bounds.origin.x) + crate::path::paint::strip_pad(false, &paint.scale());
        let label = paint.shape(
            &view.inputs.git_label,
            crate::path::paint::Size::Caption,
            false,
            window,
        );
        let strip = &view.strips[0];
        let unit = strip.width() + crate::path::paint::GIT_GAP + f32::from(label.line().width);
        let width = f32::from(window.viewport_size().width);
        let centres: Vec<f32> = (0..strip.crumbs().len())
            .map(|index| {
                let (offset, crumb) = strip.extent(index).unwrap();
                width - (left + unit) + offset + crumb / 2.
            })
            .collect();
        (
            centres,
            f32::from(bounds.origin.y + bounds.size.height / 2.),
        )
    });
    assert!(centres.len() >= 2, "a root and a leaf crumb");
    for (index, x) in centres.into_iter().enumerate() {
        cx.simulate_mouse_move(point(px(x), px(row)), None, Modifiers::default());
        cx.run_until_parked();
        let hovered = path.read_with(cx, |view, _| view.strips[0].hovered);
        assert_eq!(hovered, Some(index), "crumb {index} painted at x {x}");
    }
}
