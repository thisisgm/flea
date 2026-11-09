use crate::{
    backend::Backend,
    g2::status_feedback::{line::DETAIL_LINE, observe::Text},
    list::Typeface,
    presentation::{Fonts, Snapshot},
    window::root::Root,
};
use gpui::{
    canvas, prelude::*, px, App, AppContext, Bounds, Div, Pixels, SharedString, TestAppContext,
    TextRun, WindowHandle,
};
use std::cell::RefCell;

const ERROR: &str = concat!(
    "Permission denied while copying a long file name into the destination directory. ",
    "The destination rejected this write and the source file has been preserved. ",
    "Choose another destination or change the destination permissions before retrying."
);

#[derive(Clone)]
struct Paint {
    draw: Option<u64>,
    bounds: Bounds<Pixels>,
    family: SharedString,
    size: Pixels,
    width: Pixels,
    expected: Typeface,
    expected_width: Pixels,
    expected_height: Pixels,
    viewport_width: Pixels,
    lane: Option<Text>,
}

#[derive(Default)]
struct Paints(RefCell<Vec<Paint>>);
impl gpui::Global for Paints {}

pub(in crate::g2::status_feedback) fn watch(div: Div, text: SharedString, cx: &App) -> Div {
    if cx.try_global::<Paints>().is_none() {
        return div;
    }
    div.child(
        canvas(
            move |bounds, window, cx| {
                let style = window.text_style();
                let size = style.font_size.to_pixels(window.rem_size());
                let width = window
                    .text_system()
                    .layout_line(&text, size, &[style.to_run(text.len())], None)
                    .width;
                let expected = Typeface::for_window(window, cx);
                let run = TextRun {
                    len: text.len(),
                    font: expected.font.clone(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let expected_width = window
                    .text_system()
                    .layout_line(&text, expected.caption, std::slice::from_ref(&run), None)
                    .width;
                let rows: usize = window
                    .text_system()
                    .shape_text(
                        text.clone(),
                        expected.caption,
                        &[run],
                        Some(bounds.size.width),
                        None,
                    )
                    .unwrap()
                    .iter()
                    .map(|line| line.wrap_boundaries().len() + 1)
                    .sum();
                Paint {
                    draw: window.current_draw_id(),
                    bounds,
                    family: style.font_family,
                    size,
                    width,
                    expected,
                    expected_width,
                    expected_height: px(rows as f32 * DETAIL_LINE),
                    viewport_width: window.viewport_size().width,
                    lane: None,
                }
            },
            |_, mut paint, window, cx| {
                let root = window.root::<Root>().flatten().expect("mounted Root");
                let shell = root.read(cx).shell.read(cx);
                let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
                paint.lane = footer.message_line().read(cx).painted.get("primary");
                cx.global::<Paints>().0.borrow_mut().push(paint);
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .size_full(),
    )
}

fn snapshot(base: u32, family: &str) -> Snapshot {
    let (theme, parsed) = crate::palette::Theme::from_text_observed("");
    Snapshot {
        theme,
        parsed,
        fonts: Fonts::from_sources(&format!("[font]\nbase-size={base}"), "", family.into()),
    }
}

fn open(cx: &mut TestAppContext, base: u32, family: &str, backend: Backend) -> WindowHandle<Root> {
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        cx.set_global(Paints::default());
        snapshot(base, family).install(cx);
    });
    cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.status_messages(cx, |messages, now| {
                messages.say(
                    ERROR,
                    true,
                    ERROR,
                    flea::ui_logic_status_feedback::status::Hint::None,
                    now,
                )
            });
            shell
        }),
    })
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn first(cx: &mut TestAppContext, body: u32, phase: &str) -> Paint {
    let paint = cx.update(|cx| {
        cx.global::<Paints>()
            .0
            .borrow()
            .iter()
            .find(|paint| paint.expected.body == px(body as f32))
            .cloned()
            .expect("the first actual detail paint at the accepted size")
    });
    let lane = paint
        .lane
        .as_ref()
        .expect("the actual message lane painted");
    eprintln!(
        "F1_DETAIL_PAINT phase={phase} body={body} expected={} detail={} lane={} detail_width={} lane_width={} expected_width={} height={} expected_height={} draw={:?}",
        f32::from(paint.expected.caption),
        f32::from(paint.size),
        lane.font_size,
        f32::from(paint.width),
        lane.implicit_width,
        f32::from(paint.expected_width),
        f32::from(paint.bounds.size.height),
        f32::from(paint.expected_height),
        paint.draw,
    );
    assert_eq!(
        paint.size, paint.expected.caption,
        "F1_DETAIL_MISMATCH phase={phase}"
    );
    assert_eq!(
        paint.family, paint.expected.font.family,
        "F1_DETAIL_FAMILY phase={phase}"
    );
    assert_eq!(lane.font_size, f32::from(paint.expected.caption));
    assert_eq!(lane.text.as_ref(), ERROR);
    assert!((f32::from(paint.width - paint.expected_width)).abs() < 0.01);
    assert!((lane.implicit_width - f32::from(paint.expected_width)).abs() < 0.01);
    assert!((f32::from(paint.bounds.size.height - paint.expected_height)).abs() <= 1.);
    assert!((f32::from(paint.bounds.size.width - paint.viewport_width) + 28.).abs() <= 1.);
    paint
}

#[gpui::test]
fn detail_first_root_draw_uses_accepted_nondefault_font(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = open(cx, 18, "DejaVu Sans", backend);
    draw(cx, handle);
    first(cx, 18, "first-18");
}

#[gpui::test]
fn detail_live_18_to_14_uses_the_first_accepted_draw(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = open(cx, 18, "DejaVu Sans", backend);
    draw(cx, handle);
    handle
        .update(cx, |root, window, cx| {
            let shell = root.shell.read(cx);
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            assert_eq!(
                footer
                    .message_line()
                    .read(cx)
                    .painted
                    .get("primary")
                    .unwrap()
                    .font_size,
                15.
            );
            cx.global::<Paints>().0.borrow_mut().clear();
            root.shell.update(cx, |shell, cx| {
                shell.apply_presentation(snapshot(14, "DejaVu Sans"), window, cx);
            });
            cx.notify();
        })
        .unwrap();
    draw(cx, handle);
    first(cx, 14, "live-14");
}

#[gpui::test]
fn detail_first_default_14_draw_keeps_existing_geometry(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = open(cx, 14, "JetBrainsMono Nerd Font", backend);
    draw(cx, handle);
    let paint = first(cx, 14, "default-14");
    assert_eq!(paint.size, px(12.));
}
