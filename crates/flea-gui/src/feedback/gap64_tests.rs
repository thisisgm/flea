//! The settled all-spinner Omi ruling applies to real listing and Columns loading marks.
use super::spinner_tests::{frame, load, open, shell};
use crate::{feedback::spinner::Mode, list::Palette, palette::Theme};
use gpui::TestAppContext;

#[gpui::test]
fn sol_gap64_loading_panes_use_the_accent_omi_player(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, true);
    let mark = load(cx, handle);
    frame(cx, handle);
    shell(cx, handle, |_, window, cx| {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        assert!(
            !mark.is_spiral(),
            "GM's only original-spinner exception is About"
        );
        assert_eq!(mark.mode(), Mode::Thinking);
        assert_eq!(mark.ink(), palette.accent);
        assert!(mark.counters.frames.get() > 0);
    });
}

#[test]
fn sol_gap64_columns_loading_uses_the_accent_omi_player() {
    use crate::{columns::geometry::Slot, window::columns_tests::host};
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::frame(&mut cx, handle);
    host::advance(
        &mut cx,
        handle,
        std::time::Duration::from_millis(crate::pane::swap::CAP_MS + 20),
    );
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        let marks = shell.column_loading_marks(window, cx);
        let mark = marks[Slot::Child.index()]
            .as_ref()
            .expect("the held child's mark");
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        assert!(
            !mark.is_spiral(),
            "Columns loading follows the same Omi ruling"
        );
        assert_eq!(mark.mode(), Mode::Thinking);
        assert_eq!(mark.ink(), palette.accent);
        assert!(mark.slot().size.width > gpui::px(0.));
    });
}
