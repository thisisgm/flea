//! The mounted unlock input never exposes surrounding text or submits a marked passphrase.
use super::{
    process::{Handle, Job, Runner},
    view::UnlockView,
};
use gpui::{EntityInputHandler, TestAppContext};
use std::{cell::Cell, rc::Rc, sync::Arc};
#[derive(Default)]
struct Record(Cell<usize>);
impl Runner for Record {
    fn start(&self, _: Job) -> Handle {
        self.0.set(self.0.get() + 1);
        let (handle, _, _) = Handle::probed();
        handle
    }
}
#[gpui::test]
fn i50_dialog_password_private_unlock(cx: &mut TestAppContext) {
    let runner = Rc::new(Record::default());
    let probe = runner.clone();
    let (view, cx) = cx.add_window_view(|_, cx| {
        UnlockView::new(Arc::new(crate::test_catalogs::english()), runner, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(cx, |v, w, cx| {
        v.open_for("/dev/test", "Test", 1024, "LUKS", w, cx)
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    view.update_in(cx, |v, w, cx| {
        v.replace_and_mark_text_in_range(None, "\u{1d11e}猫", Some(2..3), w, cx);
        assert_eq!(v.marked_text_range(w, cx), Some(0..3));
        let mut adjusted = Some(7..8);
        assert!(v.text_for_range(0..3, &mut adjusted, w, cx).is_none());
        assert_eq!(adjusted, Some(7..8));
        assert!(v
            .host
            .card
            .as_ref()
            .unwrap()
            .field
            .entry
            .copyable()
            .is_none());
    });
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    view.update_in(cx, |v, w, cx| {
        let bounds = v.drawn.borrow().bounds.unwrap();
        let candidate = v.bounds_for_range(0..3, bounds, w, cx).unwrap();
        assert!(candidate.size.width > gpui::px(0.));
        assert!(v.drawn.borrow().paint.as_ref().unwrap().marked.is_some());
        assert!(v
            .drawn
            .borrow()
            .paint
            .as_ref()
            .unwrap()
            .text
            .chars()
            .all(|c| c == '\u{2022}'));
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(probe.0.get(), 0);
    view.update_in(cx, |v, w, cx| v.replace_text_in_range(None, "zz", w, cx));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("ab");
    assert!(
        view.read_with(cx, |v, _| v.host.card.as_ref().unwrap().field.text.expose()
            == b"ab")
    );
    cx.simulate_keystrokes("tab");
    view.update_in(cx, |v, w, cx| {
        v.replace_text_in_range(None, "late", w, cx);
        assert!(v.selected_text_range(false, w, cx).is_none());
        assert_eq!(v.host.card.as_ref().unwrap().field.dots(), 2);
        v.close(cx);
        v.replace_and_mark_text_in_range(None, "late", None, w, cx);
        assert!(v.selected_text_range(false, w, cx).is_none());
    });
    assert_eq!(probe.0.get(), 0);
}
