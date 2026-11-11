//! Native HTML lifecycle fixture using the production component, backend and IPC server.
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::Json,
};
use flea_gui::{
    backend::{Backend, Event as BackendEvent},
    g2::doc_preview::{
        self, code,
        html::{HtmlHost, HtmlView, Metrics, SourceParts},
    },
    ipc::server::{Answer, Event, Server},
    palette::Theme,
};
use gpui::{
    div, prelude::*, px, size, App, AppContext, Context, Entity, Render, Task, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};
use std::{cell::Cell, path::PathBuf, rc::Rc, time::Duration};

#[derive(Clone)]
struct Host {
    backend: Rc<Backend>,
    serial: Rc<Cell<u64>>,
}
impl HtmlHost for Host {
    fn ask_html(&self, slot: &str, path: &str, width: u32, height: u32) -> u64 {
        let token = self.serial.get() + 1;
        self.serial.set(token);
        self.backend
            .operation(flea::ui_logic_doc_preview::html::ask_line(
                token, slot, path, width, height,
            ))
            .expect("real HTML request");
        token
    }
    fn cancel_html(&self, slot: &str, width: u32) {
        self.backend
            .operation(format!(
                r#"{{"c":"html","slot":"{slot}","path":"","width":{width},"cancel":true}}"#
            ))
            .expect("real HTML cancellation");
    }
}

struct Fixture {
    view: Entity<HtmlView>,
    _jobs: Vec<Task<()>>,
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default();
        div().size_full().bg(theme.background).child(
            div()
                .absolute()
                .left(px(40.))
                .top(px(40.))
                .w(px(300.))
                .h(px(200.))
                .bg(theme.background)
                .rounded(px(12.))
                .child(self.view.clone()),
        )
    }
}

fn parsed_reply(doc: &Json) -> flea::ui_logic_doc_preview::html::Reply {
    let text = |key| doc.get(key).and_then(Json::as_str).unwrap_or("").to_owned();
    let number = |key| doc.get(key).and_then(Json::as_f64).unwrap_or(0.) as u64;
    flea::ui_logic_doc_preview::html::Reply {
        token: number("token"),
        slot: text("slot"),
        state: text("state"),
        file: text("file"),
        total: number("total") as usize,
        width: number("width") as u32,
        title: text("title"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("an owned HTML file is required")?,
    );
    let parent = path
        .parent()
        .ok_or("the HTML file must have a parent")?
        .to_owned();
    assert!(path.is_absolute() && parent.join(".flea-test-sandbox").is_file());
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("initial HTML metadata {}: {error}", path.display()))?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("initial HTML mtime {}: {error}", path.display()))?;
    let stamp = (metadata.len(), modified, metadata.permissions());
    let backend = Rc::new(
        Backend::start(parent.to_string_lossy().into(), None, 1)
            .map_err(|error| format!("matching backend for {}: {error}", parent.display()))?,
    );
    let drained = backend.drained();
    let host = Host {
        backend: backend.clone(),
        serial: Rc::default(),
    };
    let runtime =
        flea::ipc::endpoint::runtime().map_err(|error| format!("private runtime: {error}"))?;
    let server = Server::bind(&runtime)
        .map_err(|error| format!("fixture IPC in {}: {error}", runtime.display()))?;
    let events = server.events.clone();
    let replies = backend.replies.clone();
    gpui_platform::application().run(move |cx: &mut App| {
        let options = WindowOptions {
            app_id: Some("com.thisisgm.flea".into()),
            window_bounds: Some(WindowBounds::centered(size(px(380.), px(280.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Flea".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        cx.open_window(options, move |window, cx| {
            cx.new(move |cx| {
                let theme = Theme::default();
                let foreground = theme.foreground;
                let view = cx.new(|cx| {
                    let parts = SourceParts {
                        typeface: code::Typeface {
                            font: gpui::font("monospace"),
                            body: px(13.),
                            caption: px(12.),
                        },
                        palette: code::Palette {
                            foreground,
                            muted: foreground,
                            muted_frame: foreground,
                            gutter: foreground,
                            inks: [foreground; 7],
                        },
                        metrics: code::Metrics {
                            inset: 14.,
                            top: 14.,
                            right: 20.,
                            row: 20.,
                            gap: 12.,
                            state_mark: 32.,
                        },
                    };
                    let mut view = HtmlView::new(
                        "column",
                        doc_preview::Palette {
                            background: theme.background,
                        },
                        Metrics {
                            row_height: 20.,
                            radius: 12.,
                            round_top: true,
                        },
                        parts,
                        flea_gui::row::marks::Mark::Code.svg(),
                        cx,
                    );
                    view.set_hosts(Some(Box::new(host)), None);
                    view.set_catalog(
                        std::sync::Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
                            omarchy: "/usr/share/omarchy".into(),
                            language_file: parent.join("language"),
                            catalogs: vec!["/usr/share/flea/i18n".into()],
                        })),
                        cx,
                    );
                    view.set_path(&path.to_string_lossy(), metadata.len(), false, cx);
                    view.set_active(true, cx);
                    view
                });
                cx.observe(&view, |_, _, cx| cx.notify()).detach();
                let html = view.clone();
                let backend = backend.clone();
                let ipc = cx.spawn_in(window, async move |_, cx| {
                    while let Ok(event) = events.recv().await {
                        let _ = cx.update(|window, cx| match event {
                            Event::Call { request, answer } => {
                                let (reply, quit) = match request {
                                    Request::Call {
                                        target, function, ..
                                    } if target == "flea" && function == "htmlState" => (
                                        Reply::value(
                                            flea::jsondoc::render(
                                                &html.read(cx).capture_state(window, cx),
                                            )
                                            .trim_end(),
                                        ),
                                        false,
                                    ),
                                    Request::Call {
                                        target, function, ..
                                    } if target == "flea" && function == "ready" => {
                                        (Reply::value("true"), false)
                                    }
                                    Request::Call {
                                        target, function, ..
                                    } if target == "flea" && function == "quit" => {
                                        (Reply::value(""), true)
                                    }
                                    _ => (flea::ipc::registry::no_target(), false),
                                };
                                let _ = answer.try_send(Answer { reply, quit });
                            }
                            Event::Quit => {
                                backend.quit();
                                cx.quit();
                            }
                        });
                    }
                });
                let html = view.clone();
                let producer = cx.spawn(async move |_, cx| {
                    while let Ok(event) = replies.recv().await {
                        if let BackendEvent::Producer(line) = event {
                            if let Ok(doc) = flea::jsondoc::parse(&line) {
                                if doc.get("t").and_then(Json::as_str) == Some("html") {
                                    html.update(cx, |view, cx| view.reply(parsed_reply(&doc), cx));
                                }
                            }
                        }
                    }
                });
                let html = view.clone();
                let metadata_job = cx.spawn(async move |_, cx| {
                    let mut previous = stamp;
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(50))
                            .await;
                        let file = path.clone();
                        let metadata = cx
                            .background_executor()
                            .spawn(async move { std::fs::metadata(file) })
                            .await;
                        let Ok(metadata) = metadata else { continue };
                        let Ok(modified) = metadata.modified() else {
                            continue;
                        };
                        let current = (metadata.len(), modified, metadata.permissions());
                        if current == previous {
                            continue;
                        }
                        previous = current;
                        html.update(cx, |view, cx| {
                            view.doc.size = metadata.len();
                            if view.doc.over_size() {
                                let now = view.now(cx);
                                view.doc.schedule(now);
                                cx.notify();
                            } else {
                                view.reload(cx);
                            }
                        });
                    }
                });
                Fixture {
                    view,
                    _jobs: vec![ipc, producer, metadata_job],
                }
            })
        })
        .expect("native fixture window");
        cx.activate(true);
    });
    let _ = drained.recv_blocking();
    Ok(())
}
