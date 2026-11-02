//! The Markdown view as a real GPUI entity in a test window, its parse, layout and picture threads real, its figure helper a script in the root.
use super::Root;
use flea::i18n::{I18n, Paths};
use flea_gui::g2::pv2::markdown::{figures, Faces, MarkdownView, Palette, Tokens};
use gpui::{AppContext as _, TestAppContext, WindowHandle};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const TOKENS: Tokens = Tokens {
    body: 14.,
    theme_body: 14.,
    caption: 12.,
    gap: 8.,
    hairline: 1.,
    row_pad_x: 12.,
    row_pad_y: 6.,
    mark: 16.,
    state_mark: 37.,
    source_line: None,
    column: false,
};

pub fn palette() -> Palette {
    let theme = flea_gui::palette::Theme::default();
    Palette {
        background: theme.background,
        code: theme.background,
        surface: theme.surface,
        foreground: theme.foreground,
        bright: theme.foreground_bright,
        figure_muted: flea_gui::palette::Theme::from_text("foreground='#b3b3b3'").foreground,
        muted: flea_gui::palette::Theme::from_text("foreground='#999999'").foreground,
        muted_frame: theme.metadata_frame,
        accent_frame: theme.accent_frame,
        inks: theme.code_inks,
    }
}

#[test]
fn ci_green4_port_markdown_roles_remain_distinct() {
    let palette = palette();
    assert_ne!(
        palette.code, palette.surface,
        "code blocks must differ from card surfaces"
    );
    assert_ne!(
        palette.figure_muted, palette.muted,
        "figure and body captions must differ"
    );
}

/// The English catalog from the repository, whatever the box's language.
pub fn catalog(root: &Root) -> I18n {
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

/// How the stand-in helper answers a figure.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    Draws,
    Fails,
    Silent,
}

/// A stand-in for `flea --figure-helper` and `flea --figure-store`: every figure a 240 by 120 picture, an error or no answer, every store lookup a miss.
pub fn helper(root: &Root, mode: Helper) -> PathBuf {
    let path = root.join("flea-figures");
    let answer = if mode == Helper::Silent {
        ":"
    } else if mode == Helper::Fails {
        r#"printf '{"id":%s,"error":"render failed"}\n' "$id""#
    } else {
        r##"printf '{"id":%s,"svg":"<svg xmlns=\\"http://www.w3.org/2000/svg\\" width=\\"240\\" height=\\"120\\" viewBox=\\"0 0 240 120\\"><rect width=\\"240\\" height=\\"120\\" fill=\\"#888\\"/></svg>"}\n' "$id""##
    };
    let script = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/^[^0-9]*"id":\([0-9]*\).*/\1/p')
  [ -n "$id" ] || continue
  case "$1" in
    --figure-store) printf '{"id":%s}\n' "$id" ;;
    *) ANSWER ;;
  esac
done
"#
    .replace("ANSWER", answer);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

// The parse, layout and picture threads are real, so their wakes come from outside the deterministic scheduler.
pub fn open(cx: &mut TestAppContext, root: &Root, mode: Helper) -> WindowHandle<MarkdownView> {
    open_with(cx, root, mode, TOKENS)
}

pub fn open_with(
    cx: &mut TestAppContext,
    root: &Root,
    mode: Helper,
    tokens: Tokens,
) -> WindowHandle<MarkdownView> {
    cx.executor().allow_parking();
    let script = helper(root, mode);
    cx.update(|cx| {
        let host = figures::host(cx);
        host.update(cx, |host, _| host.binary = script);
    });
    let i18n = catalog(root);
    cx.add_window(|_, cx| {
        MarkdownView::new(
            palette(),
            tokens,
            Faces::omarchy_default(),
            "look",
            &i18n,
            cx,
        )
    })
}

pub fn with<R>(
    cx: &mut TestAppContext,
    view: WindowHandle<MarkdownView>,
    f: impl FnOnce(&mut MarkdownView) -> R,
) -> R {
    view.update(cx, |view, _, _| f(view)).unwrap()
}

pub fn draw(cx: &mut TestAppContext, view: WindowHandle<MarkdownView>) {
    cx.update_window(view.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
}

/// Draws until the condition holds; `each` sees the view after every frame.
pub fn until(
    cx: &mut TestAppContext,
    view: WindowHandle<MarkdownView>,
    what: &str,
    mut each: impl FnMut(&MarkdownView),
    done: impl Fn(&MarkdownView) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        draw(cx, view);
        let finished = with(cx, view, |v| {
            each(v);
            done(v)
        });
        if finished {
            return;
        }
        assert!(Instant::now() < deadline, "{what} within 20 s");
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub fn show(cx: &mut TestAppContext, view: WindowHandle<MarkdownView>, path: &Path) {
    let path = path.to_path_buf();
    view.update(cx, |v, _, cx| {
        v.set_active(true, cx);
        v.set_path(Some(path), cx);
    })
    .unwrap();
}

/// Shows a document and draws until its first screen; every frame before it must have drawn nothing.
pub fn first_screen(cx: &mut TestAppContext, view: WindowHandle<MarkdownView>, path: &Path) {
    show(cx, view, path);
    let mut early = 0;
    until(
        cx,
        view,
        "the first screen",
        |v| {
            if v.counts.first.is_none() {
                early = early.max(v.counts.items_frame);
            }
        },
        |v| v.counts.first.is_some(),
    );
    assert_eq!(early, 0, "a held frame draws nothing of the document");
}
