//! The typed-path route on the picker: a committed line is peeked, a folder opens, a file is located then activated.
use super::picker::Picker;
use crate::g2::pk2::route::{self, Located, Peeked, RouteHost};
use crate::path::view::Out;
use crate::path::{answer, Said};
use flea::json::escape;
use flea::jsondoc::Json;
use flea::ui_logic_pk1 as logic;
use gpui::Context;

// The route's answers, carried out on the picker with its context.
struct Host<'a, 'b> {
    picker: &'a mut Picker,
    cx: &'a mut Context<'b, Picker>,
}

impl RouteHost for Host<'_, '_> {
    fn open_folder(&mut self, path: String) {
        self.picker.open_folder(&path, self.cx);
    }
    fn locate_file(&mut self, path: String) {
        self.picker
            .send_listing(&format!(r#"{{"c":"locate","path":"{}"}}"#, escape(&path)));
    }
    fn move_cursor(&mut self, index: usize) {
        self.picker.move_cursor(index, self.cx);
    }
    fn activate_row(&mut self, index: usize) {
        self.picker.activate_row(index, self.cx);
    }
    fn say_route(&mut self, said: Said, loud: bool) {
        self.picker.chrome.reopen_path = true;
        let text = said.text(&self.picker.locale.catalog);
        self.picker.say(text, loud, self.cx);
    }
}

fn number(document: &Json, key: &str) -> Option<i64> {
    match document.get(key) {
        Some(Json::Num(value)) => value.parse().ok(),
        _ => None,
    }
}

impl Picker {
    fn host<'a, 'b>(&'a mut self, cx: &'a mut Context<'b, Self>) -> Host<'a, 'b> {
        Host { picker: self, cx }
    }

    // Every listing of the leaf's folder renumbers its rows, so the leaf is located afresh.
    pub(super) fn relocate(&mut self) {
        let route = &mut self.browse.route;
        if route.leaf.is_empty() {
            return;
        }
        if route.leaf_parent != self.listing.location {
            route.leaf.clear();
            return;
        }
        route.leaf_index = -1;
        let line = format!(r#"{{"c":"locate","path":"{}"}}"#, escape(&route.leaf));
        self.send_listing(&line);
    }

    pub(super) fn take_leaf(&mut self, cx: &mut Context<Self>) {
        let route = &self.browse.route;
        if self.listing.failed {
            self.browse.route.leaf = route::leaf_on_listing_failed(&route.leaf, true).to_owned();
            return;
        }
        let held = self.browse.listing.read(cx).listing().held();
        let index = usize::try_from(route.leaf_index).ok();
        let holds = index.is_some_and(|at| at >= held.start && at < held.start + held.rows.len());
        if let Some(index) = route::take_row(&route.leaf, route.leaf_index, holds) {
            self.browse.route.leaf.clear();
            self.browse.route.leaf_index = -1;
            self.host(cx).activate_row(index);
        }
    }

    // A count-zero peek answers a committed line; a peek with rows is Tab's, and goes back to the shared entry.
    pub(super) fn peeked(&mut self, document: &Json, cx: &mut Context<Self>) {
        if number(document, "first").unwrap_or(0) > 0 {
            let (Some(answer), Some(window)) = (answer(document), self.browse.window) else {
                return;
            };
            let view = self.browse.path_view(cx);
            let _ = window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    if view.claims(&answer) {
                        view.receive(answer, window, cx);
                    }
                })
            });
            return;
        }
        let path = document
            .get("path")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let failed = document.get("failed").and_then(Json::as_bool) == Some(true);
        let mode = number(document, "mode")
            .and_then(|mode| u32::try_from(mode).ok())
            .unwrap_or(0);
        let answer = route::peeked(
            &self.browse.route.asked,
            path,
            failed,
            mode,
            logic::saving(&self.req),
            logic::folder_mode(&self.req),
        );
        if answer == Peeked::Stale {
            return;
        }
        self.browse.route.asked.clear();
        match answer {
            Peeked::Stale => {}
            Peeked::OpenFolder(path) => self.host(cx).open_folder(path),
            // A file where a folder answers is the backend's own refusal, said through the mark's answer.
            Peeked::ProbeMark(path) => self.accept_mark(&path, true, false),
            Peeked::LocateFile { parent, leaf } => {
                let route = &mut self.browse.route;
                route.leaf = leaf.clone();
                route.leaf_parent = parent.clone();
                route.leaf_index = -1;
                if parent != self.listing.location {
                    self.host(cx).open_folder(parent);
                } else if self.listing.failed {
                    self.browse.route.leaf.clear();
                } else if self.browse.in_flight == 0 {
                    self.host(cx).locate_file(leaf);
                }
            }
            Peeked::Say(said, loud) => self.host(cx).say_route(said, loud),
        }
    }

    pub(super) fn located(&mut self, document: &Json, cx: &mut Context<Self>) {
        let directory = document
            .get("directory")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let path = document
            .get("path")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let index = number(document, "index").unwrap_or(-1);
        let route = &self.browse.route;
        match route::located(&route.leaf, &route.leaf_parent, directory, path, index) {
            Located::Stale => {}
            Located::Filtered(name) => {
                self.browse.route.leaf.clear();
                self.host(cx).say_route(Said::Filtered(name), true);
            }
            Located::Row(row) => {
                self.browse.route.leaf_index = row as i64;
                self.host(cx).move_cursor(row);
                self.take_leaf(cx);
            }
        }
    }

    // What the shared path entry asks: a committed folder or file is peeked first, Tab's peek goes to the child.
    pub(super) fn path_out(&mut self, out: Out, cx: &mut Context<Self>) {
        if self.answered() {
            return;
        }
        match out {
            // PathBar.js hands a typed smb:// line back untouched, and the chooser peeks it as it peeks a path.
            Out::Open { path, .. } | Out::Network(path) => {
                let view = self.browse.path_view(cx);
                self.chrome.restore = Some(view.read(cx).focus.clone());
                self.chrome.restore_path = Some(view.read(cx).entry.text.clone());
                self.chrome.reopen_path = false;
                self.browse.route.asked = path.clone();
                self.browse.route.leaf.clear();
                self.send_listing(&crate::path::peek_line(&path, 0, self.show_hidden()));
            }
            Out::Send { line, .. } => {
                self.send_listing(&line);
            }
            Out::Said(text) => self.say(text, false, cx),
            Out::Closed => {
                self.browse.route.asked.clear();
                self.browse.route.leaf.clear();
                self.browse.focus_filename = self.browse.filename.is_some();
                cx.notify();
            }
            Out::ReadHistory => self.read_jump_history(cx),
            Out::Focus(_) => {}
        }
    }
}
