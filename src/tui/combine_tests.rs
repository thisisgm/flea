//! The five acceptance checks drive real menu, rendering, sandbox and publication paths.
use super::render::frame_tests::{entry, grid, model};
use super::{
    actions, combine,
    input::{Key, Pointer},
    keymap::Map,
    model::Model,
    render,
    wire::{self, Wire},
};
use std::time::Duration;

pub fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
pub fn pointer(m: &mut Model, w: &mut Wire, x: usize, y: usize, motion: bool, released: bool) {
    let mut key = Key::named("Pointer", "");
    key.pointer = Some(Pointer {
        button: 0,
        x,
        y,
        motion,
        released,
    });
    press(m, w, key);
}
pub fn choose(m: &mut Model, w: &mut Wire, focus: usize, choice: usize) {
    let g = combine::geometry(m);
    let field = g
        .fields
        .iter()
        .find(|field| field.focus == focus && field.choice == choice)
        .unwrap();
    pointer(m, w, field.rect.x + 1, field.rect.y + 1, false, false);
}
pub fn frame(m: &Model) -> Vec<String> {
    grid(
        &render::frame(
            m,
            &super::theme::Theme::from_text(""),
            &Map::load(),
            m.columns,
            m.height + 4,
            Duration::ZERO,
        ),
        m.columns,
        m.height + 4,
    )
}
#[test]
fn tui_combine_order_focus_and_hit_boxes_agree() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("order");
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    for (i, name) in [(7, "photo.png"), (8, "sunset.jpg")] {
        m.rows.insert(i, entry(name, false, 99));
    }
    m.selected = [7, 8].into_iter().collect();
    m.remember_selection();
    actions::act(&mut m, "menu", &mut w).unwrap();
    let rows = render::menu_layout::entries(&m, &Map::load());
    let index = rows
        .iter()
        .position(|row| row.as_ref().is_some_and(|row| row.id == "combine"))
        .expect("menu must offer Combine Images");
    m.menu_view.hints = true;
    assert!(render::menu_layout::entries(&m, &Map::load())[index]
        .as_ref()
        .unwrap()
        .trail
        .is_empty());
    m.menu_ready = true;
    m.menu_cursor = index;
    press(&mut m, &mut w, Key::named("Return", ""));
    assert!(m.combining.card.is_some());
    assert!(frame(&m).join("\n").contains("Combine 2 Images"));
    super::action_key_tests::drain(&mut w);
    let id = m.action_id;
    press(&mut m, &mut w, Key::named("Escape", ""));
    m.receive(crate::jsondoc::parse(&format!(r#"{{"t":"menuaction","op":"validate","action":"combine","id":{id},"ok":true,"paths":["/work/a.png","/work/b.png"]}}"#)).unwrap(), &mut w).unwrap();
    assert!(
        m.combining.card.is_none(),
        "late validation must not reopen cancelled card"
    );
    super::action_key_tests::drain(&mut w);
    wire::finish(w, reader);
}
#[test]
fn tui_combine_layout_dimensions_match_output() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("dimensions");
}
#[test]
fn tui_combine_half_block_mode_uses_numbered_boxes() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("graphics");
}

#[test]
fn tui_combine_fontconfig_failure_still_renders_numbered_previews() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("font-fallback");
}
#[test]
fn tui_combine_jpeg_and_background_constraints_hold() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("jpeg");
}
#[test]
fn tui_combine_cancel_and_collision_preserve_sources() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("safe");
}

#[test]
fn tui_combine_menu_requires_supported_marked_images_without_new_shortcuts() {
    let mut m = model();
    assert!(!combine::offered(&m));
    m.selected = [7, 8].into_iter().collect();
    m.rows.get_mut(&7).unwrap().name = "one.png".into();
    m.rows.get_mut(&8).unwrap().name = "two.jpg".into();
    assert!(combine::offered(&m));
    m.rows.get_mut(&8).unwrap().directory = true;
    assert!(!combine::offered(&m));
    m.rows.get_mut(&8).unwrap().directory = false;
    m.rows.get_mut(&8).unwrap().name = "text.txt".into();
    assert!(!combine::offered(&m));
    assert_ne!(
        Map::load().action(&Key::character('c', ""), "default"),
        "combine"
    );
}

#[test]
fn tui_combine_cancel_before_start_and_shared_busy_namespace_are_correlated() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    for (i, name) in [(7, "photo.png"), (8, "sunset.jpg")] {
        m.rows.insert(i, entry(name, false, 99));
    }
    m.selected = [7, 8].into_iter().collect();
    m.remember_selection();
    for busy in [false, true] {
        actions::act(&mut m, "combine", &mut w).unwrap();
        let id = m.action_id;
        for line in [
            r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
            r#"{"t":"menuaction","op":"validate","action":"combine","id":ID,"ok":true,"paths":["/work/a.png","/work/b.png"]}"#,
        ] {
            m.receive(
                crate::jsondoc::parse(&line.replace("ID", &id.to_string())).unwrap(),
                &mut w,
            )
            .unwrap();
        }
        press(&mut m, &mut w, Key::named("Return", ""));
        assert!(m.combining.run.is_some());
        super::action_key_tests::drain(&mut w);
        if busy {
            m.receive(
                crate::jsondoc::parse(r#"{"t":"error","where":"convert","msg":"source busy"}"#)
                    .unwrap(),
                &mut w,
            )
            .unwrap();
            assert!(m.combining.run.is_none());
            assert_eq!(m.error, "source busy");
        } else {
            press(&mut m, &mut w, Key::named("Escape", ""));
            m.receive(
                crate::jsondoc::parse(r#"{"t":"combinestarted","requestId":1,"id":42}"#).unwrap(),
                &mut w,
            )
            .unwrap();
            let sent = super::action_key_tests::drain(&mut w);
            assert!(sent
                .iter()
                .any(|v| super::wire::text(v, "c") == "transfercancel"
                    && super::wire::count(v, "id") == 42));
            m.receive(crate::jsondoc::parse(r#"{"t":"combinedone","requestId":1,"id":42,"source":"/work/wrong.png","ok":true}"#).unwrap(), &mut w).unwrap();
            assert!(m.combining.run.is_some());
            m.receive(crate::jsondoc::parse(r#"{"t":"combinedone","requestId":1,"id":42,"source":"/work/a.png","ok":false,"err":"cancelled","paths":[]}"#).unwrap(), &mut w).unwrap();
            assert!(m.combining.run.is_none());
        }
        super::action_key_tests::drain(&mut w);
    }
    wire::finish(w, reader);
}

#[test]
fn tui_combine_compiler_identity_is_retained() {
    let output = std::process::Command::new("/usr/bin/timeout")
        .arg("10")
        .arg(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("-vV")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    eprintln!(
        "TUI11 compiler identity: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn tui_combine_private_staging_and_live_locale_epoch() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("privacy");
}

#[test]
fn tui_combine_review_top_row_navigation_preserves_source_identity() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("review-navigation");
}

#[test]
fn tui_combine_review_painted_number_owns_pointer_and_drag() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("review-pointer");
}

#[test]
fn tui_combine_review_short_terminal_reveals_focused_filename() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("review-short");
}

#[test]
fn tui_combine_review_oversized_columns_does_not_panic_on_up() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("review-columns");
}

#[test]
fn tui_combine_review_source_revision_is_coherent() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("review-revision");
}

#[test]
fn tui_combine_cache_revision_uses_held_source_and_reuses_snapshot() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    super::combine_process_tests::scenario("cache-revision");
}

// Only a plain Space presses the focused button; Ctrl+Space presses nothing.
#[test]
fn tui_combine_space_presses_only_without_a_modifier() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    for (i, name) in [(7, "photo.png"), (8, "sunset.jpg")] {
        m.rows.insert(i, entry(name, false, 99));
    }
    m.selected = [7, 8].into_iter().collect();
    m.remember_selection();
    for (focus, pressed) in [(7, "cancels"), (0, "starts")] {
        actions::act(&mut m, "combine", &mut w).unwrap();
        let id = m.action_id;
        for line in [
            r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
            r#"{"t":"menuaction","op":"validate","action":"combine","id":ID,"ok":true,"paths":["/work/a.png","/work/b.png"]}"#,
        ] {
            m.receive(
                crate::jsondoc::parse(&line.replace("ID", &id.to_string())).unwrap(),
                &mut w,
            )
            .unwrap();
        }
        m.combining.card.as_mut().unwrap().focus = focus;
        press(&mut m, &mut w, Key::named("Space", "ctrl"));
        assert!(
            m.combining.card.is_some() && m.combining.run.is_none(),
            "Ctrl+Space on focus {focus} presses nothing"
        );
        press(&mut m, &mut w, Key::named("Space", ""));
        if focus == 7 {
            assert!(m.combining.card.is_none(), "Space {pressed}");
        } else {
            assert!(m.combining.run.is_some(), "Space {pressed}");
            m.receive(
                crate::jsondoc::parse(r#"{"t":"error","where":"convert","msg":"source busy"}"#)
                    .unwrap(),
                &mut w,
            )
            .unwrap();
        }
        super::action_key_tests::drain(&mut w);
    }
    wire::finish(w, reader);
}
