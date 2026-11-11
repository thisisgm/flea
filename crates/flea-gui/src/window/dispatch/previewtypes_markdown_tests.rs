//! Markdown fence replies and the column surface roles.
use super::*;

#[gpui::test]
fn a_markdown_fence_colour_reply_reaches_the_column_and_releases_the_first_screen(
    cx: &mut TestAppContext,
) {
    // The markdown parse thread is real, so its wake comes from outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("mdfence");
    let dir = root.0.to_string_lossy().into_owned();
    let text = "# Fence\n\n```python\ndef sweep():\n    return 1\n```\n";
    std::fs::write(root.0.join("fence.md"), text).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file(
        "fence.md",
        "text-markdown",
        "Markdown document",
        0o100644,
    )];
    super::super::test_shell::land(&shell, cx, &dir, rows, 1);
    let mut lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","lines":6"#);
    // The fence asks once the parse thread hands back its blocks, under its own slot; the reply echoes it (EY-34).
    for _ in 0..40 {
        lines.extend(pump(cx));
        if lines.iter().any(|line| line.contains(r#""c":"code""#)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let fence = ask(&lines, "code").to_owned();
    assert!(fence.contains(r#""slot":"column:fence:0""#), "{fence}");
    let state = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            let preview = shell.selection_preview(0).expect("the column is mounted");
            let preview = preview.read(cx);
            match preview
                .pending
                .as_ref()
                .or(preview.drawn.as_ref())
                .map(|frame| &frame.mounted)
            {
                Some(Mounted::Doc(Doc::Markdown(view))) => {
                    let doc = &view.read(cx).doc;
                    (doc.fences.get(&0).is_some_and(|f| f.done), doc.shown)
                }
                _ => panic!("the markdown document is mounted"),
            }
        })
    };
    // A named fence holds the first screen with no time limit, so only its reply can release it.
    assert_eq!(
        state(cx),
        (false, false),
        "the first screen holds for the fence"
    );
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column:fence:0","state":"styled","first":1,"total":2,"lines":[[5,"def sweep():"],[0,"    return 1"]]}}"#,
            field(&fence, "token")
        ),
    );
    // The release also waits on the visible blocks' layout, a worker reply that can land after one pump.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let (done, shown) = loop {
        pump(cx);
        let now = state(cx);
        if now.1 || std::time::Instant::now() >= until {
            break now;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert!(done, "the column takes the fence's colour reply");
    assert!(
        shown,
        "the first screen stops holding once its fence is coloured"
    );
}

#[gpui::test]
fn the_column_markdown_keeps_its_chips_on_the_surface_and_its_page_on_the_window_ground(
    cx: &mut TestAppContext,
) {
    // The Markdown parse thread is real, so its wake comes from outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("mdground");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("notes.md"), "# Notes\n\nPress `Ctrl` + `C`.\n").unwrap();
    let theme = crate::palette::Theme::from_text("background='#111c18'\ndark_background='#0c1512'");
    assert_ne!(
        theme.background, theme.surface,
        "distinct grounds expose a swap"
    );
    cx.update(|cx| cx.set_global(theme));
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file(
        "notes.md",
        "text-markdown",
        "Markdown document",
        0o100644,
    )];
    super::super::test_shell::land(&shell, cx, &dir, rows, 1);
    local(&shell, cx, &dir);
    pump(cx);
    let (code, background) = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let preview = preview.read(cx);
        let frame = preview.pending.as_ref().or(preview.drawn.as_ref());
        let Some(Mounted::Doc(Doc::Markdown(view))) = frame.map(|frame| &frame.mounted) else {
            panic!("the Markdown document is mounted")
        };
        let view = view.read(cx);
        (view.palette.code, view.palette.background)
    });
    assert_eq!(
        code, theme.surface,
        "the column's chips and fences stay on the surface"
    );
    assert_eq!(
        background, theme.background,
        "the column's page stays on the window ground"
    );
}
