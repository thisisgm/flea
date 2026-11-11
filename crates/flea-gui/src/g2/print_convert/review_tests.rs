use super::{
    host::OutputView,
    model::{Action, Card, Focus, Model},
    seam::Effect,
    tests::selected,
};
use flea::ui_logic_print_convert::output::Kind;
use gpui::TestAppContext;
use std::sync::Arc;

static GEOMETRY_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn review_op4_partial_print_keeps_final_failure_visible() {
    let catalog = crate::test_catalogs::english();
    let mut model = Model::default();
    assert!(model.open(
        Action::Print,
        selected(Kind::Pdf, &["a.pdf", "b.pdf"]),
        &catalog
    ));
    let id = match &model.card {
        Some(Card::Print(p)) => p.pending.unwrap().request,
        _ => panic!(),
    };
    assert!(model.line(&format!(r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Office","default":true}}],"printer":"Office","duplex":false,"twoSided":false,"color":false,"grayscale":false,"setupArgv":["system-config-printer"]}}"#), &catalog));
    model.submit(&catalog);
    let request = model.run.as_ref().unwrap().ticket.request;
    model.effects.clear();
    for payload in [
        r#""t":"printstarted""#,
        r#""t":"printed","path":"/fixture/a.pdf","ok":false,"message":"a.pdf was not printed: queue rejected the file.""#,
        r#""t":"printed","path":"/fixture/b.pdf","ok":true,"message":"Sent b.pdf to Office""#,
        r#""t":"printdone","ok":false,"sent":1,"message":"Sent b.pdf to Office","err":"a.pdf was not printed: queue rejected the file.""#,
    ] {
        assert!(model.line(
            &format!(r#"{{"id":7,"requestId":{request},{payload}}}"#),
            &catalog
        ));
    }
    let statuses: Vec<_> = model
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Status { error, text } => Some((*error, text.as_str())),
            _ => None,
        })
        .collect();
    eprintln!(
        "partial print statuses: {statuses:?}; model error={:?}",
        model.error
    );
    assert!(!model.busy && model.run.is_none());
    assert!(
        statuses
            .last()
            .is_some_and(|(error, text)| *error && text.contains("a.pdf")),
        "a completed batch with a refusal must leave a failure receipt, got {statuses:?}"
    );
}

#[gpui::test]
fn review_op4_keyboard_format_choice_remains_in_scrolled_body(cx: &mut TestAppContext) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
    view.update_in(vcx, |view, window, cx| {
        assert!(view.open(
            Action::Convert,
            selected(Kind::Image, &["a.png"]),
            window,
            cx
        ));
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down down down down down down down");
    vcx.run_until_parked();
    assert!(view.read_with(
        vcx,
        |v, _| matches!(&v.model.card, Some(Card::Convert(c)) if c.format == "pdf")
    ));
    let body = vcx.debug_bounds("op4-body").unwrap();
    let row = vcx.debug_bounds("review-op4-format-7").unwrap();
    eprintln!("keyboard selected PDF: body={body:?}; selected row={row:?}");
    assert!(
        row.origin.y >= body.origin.y && row.bottom() <= body.bottom(),
        "keyboard-selected PDF is outside the visible scrolling body: row={row:?}; body={body:?}"
    );
    view.update_in(vcx, |view, _, cx| {
        view.body_scroll.handle.set_offset(Default::default());
        cx.notify();
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();
    let row = vcx.debug_bounds("review-op4-format-7").unwrap();
    assert!(
        row.top() >= body.top() && row.bottom() <= body.bottom(),
        "choosing the same boundary format must reveal it again"
    );
}

#[gpui::test]
fn review_op4_failed_convert_reveals_its_format_row(cx: &mut TestAppContext) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
    view.update_in(vcx, |view, window, cx| {
        assert!(view.open(
            Action::Convert,
            selected(Kind::Image, &["a.png"]),
            window,
            cx
        ));
        view.model.choose_format(0, &view.catalog);
        let Some(Card::Convert(c)) = &view.model.card else {
            panic!()
        };
        let ticket = c.pending.unwrap();
        view.model.checked(ticket, Ok(()), &view.catalog);
        view.flush(cx);
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("tab");
    vcx.run_until_parked();
    let body = vcx.debug_bounds("op4-body").unwrap();
    let row = vcx.debug_bounds("review-op4-format-0").unwrap();
    let (focus, offset) =
        view.read_with(vcx, |v, _| (v.model.focus, v.body_scroll.handle.offset()));
    eprintln!(
        "review fail probe: after Tab focus={focus:?} offset={offset:?} body={body:?} row0={row:?}"
    );
    assert_eq!(focus, Some(Focus::Check));
    assert!(
        row.bottom() <= body.top(),
        "row 0 is not scrolled out, the probe needs a smaller window"
    );
    view.update_in(vcx, |view, _, cx| {
        if !view.model.can_submit() {
            view.model.toggle_check(&view.catalog);
        }
        assert!(view.model.can_submit());
        view.model.submit(&view.catalog);
        view.flush(cx);
        let r = view.model.run.as_ref().unwrap().ticket.request;
        assert!(view.line(&format!(r#"{{"t":"convertstarted","requestId":{r},"id":8,"source":"/fixture/a.png"}}"#), cx));
        assert!(view.line(&format!(r#"{{"t":"convertdone","requestId":{r},"id":8,"source":"/fixture/a.png","ok":false,"paths":[],"path":"","err":"disk full"}}"#), cx));
    });
    vcx.run_until_parked();
    let row = vcx.debug_bounds("review-op4-format-0").unwrap();
    let (focus, cursor, offset, error) = view.read_with(vcx, |v, _| {
        let cursor = match &v.model.card {
            Some(Card::Convert(c)) => c.cursor,
            _ => usize::MAX,
        };
        (
            v.model.focus,
            cursor,
            v.body_scroll.handle.offset(),
            v.model.error.clone(),
        )
    });
    eprintln!("review fail probe: after failure focus={focus:?} cursor={cursor} offset={offset:?} row0={row:?} error={error:?}");
    assert_eq!((focus, cursor), (Some(Focus::Formats), 0));
    assert!(
        row.top() >= body.top() && row.bottom() <= body.bottom(),
        "the focused format row stays outside the clamped body after a failed convert"
    );
}

#[test]
fn review_op4_preview_dimensions_match_real_composition_commands() {
    let _serial = GEOMETRY_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    run_real_composition_commands();
}

fn run_real_composition_commands() -> std::path::PathBuf {
    use flea::ui_logic_print_convert::{composition::*, geometry::Composition};
    use std::process::Command;
    struct Sandbox(std::path::PathBuf, std::path::PathBuf);
    impl Drop for Sandbox {
        fn drop(&mut self) {
            if self.0.is_absolute()
                && !self.0.as_os_str().is_empty()
                && self.0.parent() == Some(self.1.as_path())
                && self.0.join(".flea-test-sandbox").is_file()
            {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
    let parent = std::env::temp_dir().canonicalize().unwrap();
    let made = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("flea-geometry.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
    assert!(root.is_absolute() && root.parent() == Some(parent.as_path()));
    std::fs::write(root.join(".flea-test-sandbox"), "geometry").unwrap();
    let _sandbox = Sandbox(root.clone(), parent);
    let cases = [
        (Layout::Row, Spacing::Small, 2, vec![(100, 60), (80, 50)]),
        (Layout::Column, Spacing::Small, 2, vec![(100, 60), (80, 50)]),
        (
            Layout::Grid,
            Spacing::Small,
            2,
            vec![(100, 60), (80, 50), (40, 80)],
        ),
        (Layout::Grid, Spacing::None, 4, vec![(100, 100), (100, 100)]),
        (
            Layout::Grid,
            Spacing::Small,
            4,
            vec![(100, 100), (100, 100)],
        ),
        (Layout::Row, Spacing::None, 2, vec![(1, 10), (10, 1)]),
        (Layout::Column, Spacing::None, 2, vec![(1, 10), (10, 1)]),
        (Layout::Grid, Spacing::None, 2, vec![(1, 100), (100, 1)]),
        (Layout::Row, Spacing::Small, 2, vec![(1, 100), (100, 1)]),
        (Layout::Column, Spacing::Large, 2, vec![(1, 100), (100, 1)]),
        (Layout::Grid, Spacing::Large, 2, vec![(1, 100), (100, 1)]),
    ];
    let mut mismatches = Vec::new();
    let mut disappeared = Vec::new();
    for (case, (layout, spacing, columns, sizes)) in cases.into_iter().enumerate() {
        let settings = Settings {
            layout,
            spacing,
            background: Background::White,
            columns,
        };
        let mut scaled = Vec::new();
        let run = |argv: Vec<String>| {
            let out = Command::new(&argv[0]).args(&argv[1..]).output().unwrap();
            assert!(
                out.status.success(),
                "{argv:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        let shared = shared_size(layout, &sizes).unwrap();
        for (i, (w, h)) in sizes.iter().enumerate() {
            let input = root.join(format!("input-{case}-{i}.png"));
            run(vec![
                "magick".into(),
                "-size".into(),
                format!("{w}x{h}"),
                "xc:white".into(),
                input.to_string_lossy().into_owned(),
            ]);
            let output = root.join(format!("scaled-{case}-{i}.png"));
            run(scale_argv(&input, &output, shared, &settings, i == 0));
            scaled.push(output);
        }
        let output = root.join(format!("output-{case}.png"));
        for command in join_commands(&scaled, &output, shared, &settings) {
            run(command);
        }
        let result = Command::new("magick")
            .args(["identify", "-format", "%w %h", "--"])
            .arg(output)
            .output()
            .unwrap();
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        let dimensions = text
            .split_whitespace()
            .map(|s| s.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        let preview = Composition::new(&sizes, &settings).unwrap();
        eprintln!(
            "case {case} {layout:?}/{spacing:?}/columns={columns}: preview={}x{} actual={text}",
            preview.width, preview.height
        );
        if dimensions != [preview.width, preview.height] {
            mismatches.push((case, preview.width, preview.height, dimensions));
        }
        if preview
            .tiles
            .iter()
            .any(|tile| tile.width < 1. || tile.height < 1.)
        {
            disappeared.push(case);
        }
    }
    assert!(
        mismatches.is_empty(),
        "preview differs from encoded output: {mismatches:?}"
    );
    assert!(
        disappeared.is_empty(),
        "sources disappeared from the preview in cases {disappeared:?}"
    );
    root
}

#[test]
fn review_op4_real_composition_test_leaves_no_sandbox_root() {
    let _serial = GEOMETRY_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = run_real_composition_commands();
    assert!(
        !root.exists(),
        "the real-command geometry test left its root behind: {root:?}"
    );
}
