//! A native window for Quick Look. The browser keeps its size, cursor and pane state.
use crate::{g2::pv1::QuickLook, state::Shell};
use gpui::{
    AppContext, Context, Entity, FocusHandle, Render, Subscription, TitlebarOptions, WeakEntity,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};

pub(crate) struct Popout {
    owner: WeakEntity<Shell>,
    view: Entity<QuickLook>,
    focus: FocusHandle,
    _watch: Subscription,
}
impl Popout {
    fn new(
        owner: WeakEntity<Shell>,
        view: Entity<QuickLook>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let watch = cx.observe(&view, |_, _, cx| cx.notify());
        let weak = owner.clone();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = weak.update(cx, |shell, cx| shell.quick_look_tab_switch(window, cx));
            true
        });
        Self {
            owner,
            view,
            focus,
            _watch: watch,
        }
    }
}
impl Render for Popout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fullscreen = window.is_fullscreen();
        let view = self.view.read(cx);
        if view
            .custom
            .as_ref()
            .is_some_and(|c| c.fullscreen != fullscreen)
        {
            self.view.update(cx, |view, cx| {
                view.custom.as_mut().unwrap().fullscreen = fullscreen;
                view.custom_reveal(cx);
            });
        }
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        div()
            .relative()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(crate::list::Typeface::omarchy_default().font.family.clone())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let plain = !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform;
                let action = if plain {
                    match event.keystroke.key.as_str() {
                        "i" => Some("toggleInfo"),
                        "t" => Some("toggleFilmstrip"),
                        "enter" => Some("openDefault"),
                        "f11" => Some("fullscreen"),
                        "left" => Some("previousFile"),
                        "right" => Some("nextFile"),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(action) = action {
                    this.view.update(cx, |view, cx| {
                        view.custom_action(action, cx);
                    });
                } else {
                    let _ = this.owner.update(cx, |shell, cx| {
                        shell.quick_look_key(event, window, cx);
                    });
                }
                cx.stop_propagation();
            }))
            .on_mouse_move(
                cx.listener(|this, _, _, cx| {
                    this.view.update(cx, |view, cx| view.custom_reveal(cx))
                }),
            )
            .child(self.view.clone())
    }
}

impl Shell {
    pub(super) fn custom_preview_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &mut self.quick_look else {
            return;
        };
        if let Some(handle) = host.custom.window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return;
        }
        let display = window.display(cx);
        let room = display
            .as_ref()
            .map(|d| d.bounds().size)
            .unwrap_or(window.viewport_size());
        let span = size(
            px((f32::from(room.width) * 0.8).max(480.)),
            px((f32::from(room.height) * 0.8).max(360.)),
        );
        let bounds = gpui::Bounds::centered(display.as_ref().map(|d| d.id()), span, cx);
        let options = WindowOptions {
            app_id: Some(crate::window::APP_ID.into()),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Flea Preview".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let owner = cx.entity().downgrade();
        let view = host.view.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| Popout::new(owner, view, window, cx))
        }) {
            Ok(handle) => {
                host.custom.window = Some(handle);
                host.custom.detached = true;
            }
            Err(error) => {
                host.custom.detached = false;
                eprintln!("flea: cannot open preview window: {error}");
            }
        }
    }
    pub(super) fn custom_preview_fullscreen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = self.quick_look.as_ref().and_then(|h| h.custom.window) {
            let _ = handle.update(cx, |_, window, _| window.toggle_fullscreen());
        } else {
            window.toggle_fullscreen();
        }
    }
    pub(crate) fn custom_preview_end(&mut self, cx: &mut Context<Self>) {
        if let Some(host) = &mut self.quick_look {
            host.custom.folder = None;
            host.custom.path.clear();
            host.view.update(cx, |view, _| view.custom = None);
            if let Some(window) = host.custom.window.take() {
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                });
            }
        }
    }
}
