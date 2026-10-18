//! Native probe, test-only: one window animating for 15 s, printing `Window::presented_frames` each second.
use std::{cell::Cell, rc::Rc, time::Duration};

use gpui::{
    div, prelude::*, px, size, App, AsyncApp, Bounds, Context, Render, Window, WindowBounds,
    WindowOptions,
};

const SAMPLES: u32 = 15;

struct Probe {
    frames: u64,
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Every frame asks for the next one, so a hidden window keeps a frame request pending.
        self.frames += 1;
        window.request_animation_frame();
        div().size_full().child(self.frames.to_string())
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(480.), px(320.)),
                cx,
            ))),
            ..Default::default()
        };
        let window = cx
            .open_window(options, |_, cx| cx.new(|_| Probe { frames: 0 }))
            .unwrap_or_else(|error| {
                eprintln!("presented-probe: {error}");
                std::process::exit(2);
            });
        let observed = Rc::new(Cell::new(0u64));
        let counter = observed.clone();
        if let Err(error) = window.update(cx, |_, window, cx| {
            window
                .observe_frame_presented(cx, move |_, _| counter.set(counter.get() + 1))
                .detach();
        }) {
            eprintln!("presented-probe: the presented observer did not register: {error}");
            std::process::exit(2);
        }
        cx.spawn(async move |cx: &mut AsyncApp| {
            for second in 1..=SAMPLES {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(frames) = window.update(cx, |_, window, _| window.presented_frames()) else {
                    eprintln!("presented-probe: the window closed at t={second}");
                    std::process::exit(2);
                };
                match frames {
                    Some(frames) => println!(
                        "PROBE t={second} committed={} presented={} observed={}",
                        frames.committed,
                        frames.presented,
                        observed.get()
                    ),
                    None => println!(
                        "PROBE t={second} presented=None observed={}",
                        observed.get()
                    ),
                }
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
        cx.activate(true);
    });
}
