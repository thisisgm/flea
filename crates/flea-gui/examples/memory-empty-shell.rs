//! Rendered framework floor, not a substitute for full-window acceptance or a matched binary.
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Render, Window, WindowBounds, WindowOptions,
};

struct Empty;

impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1100.), px(700.)),
                cx,
            ))),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, |_, cx| cx.new(|_| Empty)) {
            eprintln!("memory-empty-shell: {error}");
            cx.quit();
        }
        cx.activate(true);
    });
}
