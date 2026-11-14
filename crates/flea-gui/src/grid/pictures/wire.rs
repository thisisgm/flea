//! The browser connects its panes to the shared thumbnail demand owner.
use super::{
    demand::Demand,
    surface::{drawn_rows, estimated_rows, Surface},
    Key, Pictures, Policy,
};
use crate::{backend::Event, pane::states::Kind, state::Shell};
use gpui::{Context, Window};
use std::rc::Rc;

impl Shell {
    fn grid_pictures(&self, side: usize) -> Option<Rc<Pictures>> {
        Some(Rc::clone(
            &self.panes[side].as_ref()?.list.borrow().pictures,
        ))
    }

    /// Every frame: the shown surface of each pane follows its listing and settles its asks; a hidden pane lets every picture go.
    pub fn sync_grid_pictures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let policy = Policy::read(&self.launch.settings.data);
        for side in 0..2 {
            let surface = self.picture_surface(side);
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let pictures = Rc::clone(&host.list.borrow().pictures);
            if let Some(grid) = host.grid.as_mut() {
                if surface == Some(Surface::Grid) && grid.decor.pictures().is_none() {
                    // Plain stands in until the first shown frame; from here on the tiles borrow the pane's pictures, and the marks the old decor drew ride inside.
                    pictures.set_clip(Rc::clone(&grid.decor));
                    grid.decor = Rc::clone(&pictures) as Rc<dyn super::Decor>;
                }
            }
            let backend = host.backend.as_ref().filter(|_| !host.stopped);
            let connected = backend.is_some();
            if !connected {
                // A pane with no backend lets its asks go without counting a cancel it cannot send.
                pictures.map.borrow_mut().release();
            }
            let live = connected
                && match surface {
                    Some(Surface::Grid) => host.grid.as_ref().is_some_and(|grid| grid.view.visible),
                    Some(Surface::List) => host.pane.state.kind == Kind::Ready,
                    Some(Surface::Columns) => {
                        host.columns.is_some() && host.pane.state.kind == Kind::Ready
                    }
                    None => false,
                };
            // A List or Columns pane not drawn yet starts its settle from the pane's viewport; the settle reads the drawn rows.
            let want = surface.and_then(|surface| {
                drawn_rows(host, surface).or_else(|| {
                    (surface != Surface::Grid && host.pane.held.total > 0)
                        .then(|| estimated_rows(host))
                })
            });
            let lines = pictures.sync(
                Demand {
                    held: &host.pane.held,
                    path: &host.path,
                    live,
                    busy: host.pane.busy(),
                    want,
                    policy,
                },
                window,
                cx,
                move |shell, key, cx| shell.ask_grid_pictures(side, key, cx),
            );
            {
                let mut map = pictures.map.borrow_mut();
                for trace in map.traces.values() {
                    trace.observe(window, cx);
                }
                super::quality::refine(
                    &mut map,
                    host,
                    live && surface == Some(Surface::Grid),
                    window,
                    cx,
                );
            }
            pictures.flush_retired(window, cx);
            crate::preview_quality::sweep(cx);
            if connected {
                self.send_picture_lines(side, lines, cx);
            }
        }
    }

    fn send_picture_lines(&mut self, side: usize, lines: Vec<String>, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        for line in lines {
            if let Err(error) = host.backend.as_ref().map_or(Ok(()), |b| b.line(line)) {
                host.record_error(error, &self.locale);
                cx.notify();
            }
        }
    }

    // The settle fired: the plan covers the rows the pane draws now.
    fn ask_grid_pictures(&mut self, side: usize, key: Key, cx: &mut Context<Self>) {
        let Some(pictures) = self.grid_pictures(side) else {
            return;
        };
        let Some(surface) = self.picture_surface(side) else {
            return;
        };
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let lines = pictures.ask(
            &host.pane.held,
            host.pane.busy(),
            key,
            drawn_rows(host, surface),
        );
        self.send_picture_lines(side, lines, cx);
    }

    /// One backend event before the Shell routes it: a thumbed line is the Grid's and taken, an fsinfo class is noted and passed on.
    pub fn grid_pictures_event(
        &mut self,
        side: usize,
        event: &Event,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (path, class) = match event {
            Event::FilesystemDetails { facts, class } => (&facts.path, class),
            Event::ListDetails(crate::list::sizes::Reply::Filesystem { path, class }) => {
                (path, class)
            }
            // {"t":"thumbed","row":2,"file":"/home/u/.cache/thumbnails/large/b98fa408.png","ms":75.823}
            Event::Producer(line)
                if flea::json::field_str(line, "t").as_deref() == Some("thumbed") =>
            {
                if let (Some(pictures), Some(host), Ok(document)) = (
                    self.grid_pictures(side),
                    self.panes[side].as_ref(),
                    flea::jsondoc::parse(line),
                ) {
                    if ["thumbToken", "thumbPath", "thumbListing"]
                        .iter()
                        .any(|key| document.get(key).is_some())
                    {
                        return true;
                    }
                    let row = document.get("row").and_then(|row| match row {
                        flea::jsondoc::Json::Num(value) => value.parse::<usize>().ok(),
                        _ => None,
                    });
                    let file = document.get("file").and_then(flea::jsondoc::Json::as_str);
                    if let (Some(row), Some(file)) = (row, file) {
                        if crate::bench_thumbs::enabled() && !file.is_empty() {
                            if let (Some(facts), Some(flea::jsondoc::Json::Num(ms))) =
                                (host.pane.held.row(row), document.get("ms"))
                            {
                                if let Ok(ms) = ms.parse::<f64>() {
                                    crate::bench_thumbs::worker(
                                        &crate::navigation::history::join(&host.path, &facts.name),
                                        ms,
                                    );
                                }
                            }
                        }
                        pictures.thumbed(host.pane.held.listing, &host.path, row, file, cx);
                    }
                }
                return true;
            }
            _ => return false,
        };
        if let Some(pictures) = self.grid_pictures(side) {
            pictures.class(path, class);
            cx.notify();
        }
        false
    }
}
