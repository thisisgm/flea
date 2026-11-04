//! Compress keeps its released field pitch inside the shared card rhythm.
use super::*;
use crate::g2::op1::dialog_editor::Slot;
use gpui::{px, size, Bounds, Pixels};

fn painted_field(cx: &mut TestAppContext, handle: Handle, expected: Slot) -> Bounds<Pixels> {
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.rename.ime.owner, Some(expected));
        let drawn = shell.rename.ime.drawn.borrow();
        let paint = drawn.paint.as_ref().expect("the production field painted");
        assert_eq!(paint.input_bounds, Some(paint.bounds));
        paint.bounds
    })
}

#[test]
fn compress_confirm_focus_keeps_nested_field_geometry_and_controls() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    cx.simulate_window_resize(handle.into(), size(px(1100.), px(700.)));
    edit(&mut cx, handle, |shell, _, _| {
        shell.view = View::List;
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow_mut()
            .typeface
            .body = px(14.);
    });
    choose(
        &mut cx,
        handle,
        72,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t tab");
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t");
    assert!(call(&mut cx, handle, "archiveCardsState").contains("\"canCompress\":true"));
    let confirm = painted_field(&mut cx, handle, Slot::ArchiveConfirm);
    keys(&mut cx, handle, "shift-tab");
    let password = painted_field(&mut cx, handle, Slot::ArchivePassword);
    keys(&mut cx, handle, "shift-tab");
    let name = painted_field(&mut cx, handle, Slot::ArchiveName);
    keys(&mut cx, handle, "tab tab");
    assert_eq!(
        painted_field(&mut cx, handle, Slot::ArchiveConfirm),
        confirm
    );
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    let card = visual.debug_bounds("op2-card").expect("Compress mounted");
    let encrypt = visual.debug_bounds("op2-encrypt").expect("Encrypt mounted");
    let cancel = visual.debug_bounds("op2-button-0").expect("Cancel mounted");
    let compress = visual
        .debug_bounds("op2-button-1")
        .expect("Compress mounted");
    let measured = [password.top() - name.top(), confirm.top() - password.top()];
    assert_eq!(
        measured,
        [px(65.), px(65.)],
        "released field pitch; card={card:?}"
    );
    assert_eq!(card.size.width, px(590.));
    assert!(
        (f32::from(card.size.height) - 417.2).abs() <= 1.,
        "{card:?}"
    );
    assert!((f32::from(encrypt.top() - confirm.bottom()) - 26.).abs() <= 1.);
    assert!((f32::from(cancel.top() - encrypt.bottom()) - 22.).abs() <= 1.);
    assert_eq!(cancel.top(), compress.top());
    assert_eq!(card.bottom() - cancel.bottom(), px(17.));
    keys(&mut cx, handle, "tab space");
    assert!(call(&mut cx, handle, "archiveCardsState").contains("\"encrypt\":false"));
    keys(&mut cx, handle, "tab enter");
    assert!(!edit(&mut cx, handle, |shell, _, _| shell
        .operations
        .archive
        .card_open()));
    assert!(drain(&mut requests)
        .iter()
        .all(|line| !line.contains("\"c\":\"archive\"")));
}
