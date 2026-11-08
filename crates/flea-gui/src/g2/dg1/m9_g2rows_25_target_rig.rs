//! Marked filesystem fixtures and the real painted target handlers, without a native probe.
use super::m9_g2rows_25_rig::{edit, frame, Handle};
use crate::{columns::geometry::Slot, state::Shell, window::startup::View};
use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext};
use std::path::{Path, PathBuf};

pub(super) struct Fixture(PathBuf);

impl Fixture {
    pub fn new() -> Self {
        let made = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-carry-targets.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
        assert!(root.is_absolute() && root.starts_with("/tmp") && root != Path::new("/tmp"));
        std::fs::write(root.join(".flea-test-root"), b"").unwrap();
        for dir in ["src/folder", "dest/folder/nested"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["src/a.txt", "dest/a.txt", "dest/folder/a.txt"] {
            std::fs::write(root.join(file), b"fixture").unwrap();
        }
        Self(root)
    }

    pub fn path(&self, relative: &str) -> String {
        // An empty name is the root itself; join("") would add a trailing separator no folder path carries.
        let path = if relative.is_empty() {
            self.0.clone()
        } else {
            self.0.join(relative)
        };
        path.to_str().unwrap().to_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty());
        assert!(self.0.starts_with("/tmp") && self.0 != Path::new("/tmp"));
        assert!(self.0.join(".flea-test-root").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn view(cx: &mut TestAppContext, handle: Handle, view: View) {
    edit(cx, handle, |shell, window, cx| {
        shell.panes[0].as_ref().unwrap().list.borrow_mut().typeface =
            crate::list::bench::bench_typeface();
        shell.set_view(view, window, cx);
    });
    frame(cx, handle);
    if view == View::Columns {
        edit(cx, handle, |shell, window, cx| {
            let paths = shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area
                .drawn
                .clone();
            for path in paths {
                peek(
                    shell,
                    &path,
                    &[("nested", true), ("a.txt", false)],
                    window,
                    cx,
                );
            }
        });
        frame(cx, handle);
    }
}

pub(super) fn peek(
    shell: &mut Shell,
    path: &str,
    rows: &[(&str, bool)],
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
) {
    use flea::jsondoc::Json;
    let host = shell.panes[0].as_ref().unwrap();
    let folders = host.nav.folders.borrow();
    let message = Json::Obj(vec![
        ("t".into(), Json::Str("peeked".into())),
        ("path".into(), Json::Str(path.into())),
        (
            "first".into(),
            Json::Num(host.pane.first_count().to_string()),
        ),
        ("n".into(), Json::Num(rows.len().to_string())),
        ("hidden".into(), Json::Bool(folders.hidden)),
        ("hiddenLast".into(), Json::Bool(folders.hidden_last)),
        ("failed".into(), Json::Bool(false)),
        ("mode".into(), Json::Num("0".into())),
        (
            "rows".into(),
            Json::Arr(
                rows.iter()
                    .map(|(name, dir)| {
                        Json::Obj(vec![
                            ("n".into(), Json::Str((*name).into())),
                            ("d".into(), Json::Bool(*dir)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    drop(folders);
    shell.receive(0, crate::backend::Event::Peeked(message), window, cx);
}

pub(super) fn target(
    cx: &mut TestAppContext,
    handle: Handle,
    row: Option<usize>,
    slot: Slot,
) -> Point<Pixels> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match shell.view {
            View::Grid => {
                let state = host.grid.as_ref().unwrap().state.borrow();
                let local = row.map_or_else(
                    || point(px(20.), state.bounds.size.height - px(20.)),
                    |row| {
                        let tile = state.drawn_row(row).expect("painted Grid row");
                        point(
                            px(tile.slot.x + tile.slot.width / 2.),
                            px(tile.slot.y + tile.slot.height / 2.),
                        )
                    },
                );
                state.bounds.origin + local
            }
            View::Columns => {
                let columns = host.columns.as_ref().unwrap();
                let paint = columns.painters.get(slot).unwrap().borrow();
                let drawn = paint.drawn.as_ref().expect("painted Columns track");
                drawn.bounds.origin
                    + point(
                        drawn.bounds.size.width / 2.,
                        px(
                            row.map_or(300., |row| (row as f32 + 0.5) * drawn.row_height)
                                - drawn.scroll,
                        ),
                    )
            }
            _ => {
                let input = &host.input;
                let bounds = input.frame.expect("painted List").bounds;
                let x = bounds.left() + px(60.);
                let y = row.map_or(bounds.bottom() - px(20.), |row| {
                    (0..f32::from(bounds.size.height) as usize)
                        .map(|dy| bounds.top() + px(dy as f32))
                        .find(|y| input.hit(point(x, *y)) == Some(row))
                        .expect("painted List row")
                        + px(4.)
                });
                point(x, y)
            }
        }
    })
}

pub(super) fn down(
    shell: &mut Shell,
    at: Point<Pixels>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
) {
    let event = MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        click_count: 1,
        ..Default::default()
    };
    match shell.view {
        View::Grid => shell.grid_press(0, &event, window, cx),
        View::Columns => shell.columns_pointer(0, &event, window, cx),
        _ => shell.pointer_down(0, &event, window, cx),
    }
}
