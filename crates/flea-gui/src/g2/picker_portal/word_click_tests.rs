//! A double-click in the chooser's name and New Folder fields selects the word under the pointer, a third click the whole line.
use super::filename::Filename;
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use super::text::EntryHost;
use gpui::{Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

fn seeded<'a>(cx: &'a mut TestAppContext, name: &str) -> Fixture<'a> {
    let request = format!(r#"{{"mode":"save","folder":"/d","name":"{name}"}}"#);
    let f = fixture(cx, &request, None);
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    f
}

// Presses the field 1, 2 and 3 times a few pixels into its first word and returns the selected text after each.
fn presses(f: &mut Fixture, field: Entity<Filename>) -> Vec<String> {
    let at = f.cx.update(|_, cx| {
        let bounds = field
            .read(cx)
            .drawn()
            .borrow()
            .bounds
            .expect("the field drew");
        gpui::point(bounds.left() + gpui::px(6.), bounds.center().y)
    });
    (1..=3)
        .map(|clicks| {
            f.cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::none(),
                click_count: clicks,
                first_mouse: false,
            });
            f.cx.simulate_event(MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::none(),
                click_count: clicks,
            });
            f.cx.run_until_parked();
            f.cx.update(|_, cx| {
                let entry = &field.read(cx).entry;
                entry.text[entry.selected.clone()].to_owned()
            })
        })
        .collect()
}

#[gpui::test]
fn a_double_click_in_the_save_name_selects_one_word(cx: &mut TestAppContext) {
    let mut f = seeded(cx, "alpha beta.txt");
    let picker = f.picker.clone();
    let field =
        f.cx.update(|_, cx| picker.read(cx).browse.filename.clone().unwrap());
    let selected = presses(&mut f, field);
    assert_eq!(
        selected[1], "alpha",
        "a double-click selects the word under the pointer"
    );
    assert_eq!(
        selected[2], "alpha beta.txt",
        "a third click selects the whole name"
    );
}

// Double-clicks `across` of the way over the glyph at byte `at` and returns the selected text.
fn word_at(f: &mut Fixture, field: &Entity<Filename>, at: usize, across: f32) -> String {
    let point = f.cx.update(|_, cx| {
        let drawn = field.read(cx).drawn().borrow();
        let bounds = drawn.bounds.expect("the field drew");
        let (left, right) = (drawn.line.x_for(at), drawn.line.x_for(at + 1));
        let x = bounds.left() - drawn.line.scroll + left + (right - left) * across;
        gpui::point(x, bounds.center().y)
    });
    for clicks in 1..=2 {
        f.cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: point,
            modifiers: Modifiers::none(),
            click_count: clicks,
            first_mouse: false,
        });
        f.cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: point,
            modifiers: Modifiers::none(),
            click_count: clicks,
        });
        f.cx.run_until_parked();
    }
    f.cx.update(|_, cx| {
        let entry = &field.read(cx).entry;
        entry.text[entry.selected.clone()].to_owned()
    })
}

// The far half of a word's last glyph sits nearer the next caret edge; the word is still the glyph's, in both fields.
#[gpui::test]
fn a_double_click_on_either_half_of_a_glyph_takes_that_glyphs_word(cx: &mut TestAppContext) {
    let mut f = seeded(cx, "alpha beta.txt");
    let picker = f.picker.clone();
    let name =
        f.cx.update(|_, cx| picker.read(cx).browse.filename.clone().unwrap());
    assert_eq!(word_at(&mut f, &name, 4, 0.8), "alpha", "far half of a");
    assert_eq!(word_at(&mut f, &name, 5, 0.2), " ", "near half of space");
    assert_eq!(word_at(&mut f, &name, 5, 0.8), " ", "far half of space");
    assert_eq!(word_at(&mut f, &name, 6, 0.2), "beta.txt", "near half of b");
    let at = f.cx.debug_bounds("picker-new-folder").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    let folder =
        f.cx.update(|_, cx| picker.read(cx).chrome.folder.clone().unwrap());
    let text = f.cx.update(|_, cx| folder.read(cx).entry.text.clone());
    let first = text.split(' ').next().unwrap().to_owned();
    let end = first.len() - 1;
    assert_eq!(
        word_at(&mut f, &folder, end, 0.8),
        first,
        "far half: {text}"
    );
    assert_eq!(
        word_at(&mut f, &folder, end + 1, 0.8),
        " ",
        "far half: {text}"
    );
}

#[gpui::test]
fn a_double_click_in_the_new_folder_name_selects_one_word(cx: &mut TestAppContext) {
    let mut f = seeded(cx, "a.txt");
    let at = f.cx.debug_bounds("picker-new-folder").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    let picker = f.picker.clone();
    let field =
        f.cx.update(|_, cx| picker.read(cx).chrome.folder.clone().unwrap());
    let text = f.cx.update(|_, cx| field.read(cx).entry.text.clone());
    let first = text.split(' ').next().unwrap().to_owned();
    assert_ne!(
        first, text,
        "the seeded folder name holds two words: {text}"
    );
    let selected = presses(&mut f, field);
    assert_eq!(
        selected[1], first,
        "a double-click selects the word under the pointer"
    );
    assert_eq!(selected[2], text, "a third click selects the whole name");
}
