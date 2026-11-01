//! Background source loading and complete snapshot adoption for the live rail.
use super::*;

impl Producer {
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        if self.polling {
            self.load_places(cx);
        }
    }

    pub(crate) fn load_places(&mut self, cx: &mut Context<Self>) {
        if self.loaded || self.loading {
            return;
        }
        self.loading = true;
        self.directory_reads += 1;
        let home = self.home.clone();
        let read = cx
            .background_executor()
            .spawn(async move { dirs_text(&home) });
        cx.spawn(async move |this, cx| {
            let dirs = read.await;
            let _ = this.update(cx, |producer, cx| {
                producer.dirs = dirs;
                producer.loaded = true;
                producer.loading = false;
                producer.rebuild(cx);
            });
        })
        .detach();
    }

    pub(super) fn rebuild(&mut self, cx: &mut Context<Self>) {
        if !self.loaded || self.state == Json::Null {
            return;
        }
        let saved = saved_places(&self.state);
        let in_use = self.places.read(cx).tags_in_use;
        let rows = home_entries(&self.home, &self.dirs, home_glyph);
        let records = &self.places.read(cx).favourites.records;
        let mut favourites = stored_entries(records, &self.home);
        let statuses = &self.places.read(cx).favourites.statuses;
        for entry in &mut favourites {
            if let Some(status) = entry.favourite_index.and_then(|index| statuses.get(&index)) {
                if !status.is_empty() {
                    entry.error = Some(flea::ui_logic_pl4::rail::FavouriteError::Status(
                        status.clone(),
                    ));
                }
            }
        }
        self.replace_sections(
            rail_sections(
                &rows,
                saved.shown,
                favourites,
                &in_use,
                self.tail
                    .iter()
                    .filter_map(|section| {
                        let mut section = section.clone();
                        section.rows.retain(|row| {
                            let key = match row.group {
                                Group::Device => "showDevices",
                                Group::Network => "showNetwork",
                                _ => return true,
                            };
                            self.state
                                .get("places")
                                .and_then(|places| places.get(key))
                                .and_then(Json::as_bool)
                                != Some(false)
                        });
                        (!section.rows.is_empty()).then_some(section)
                    })
                    .collect(),
                &[],
            ),
            cx,
        );
    }

    /// Inspect only painted favorites, once per visible record set; failed reads retry on the next viewport change.
    pub(crate) fn inspect_visible(&mut self, cx: &mut Context<Self>) {
        if !self.polling || self.places.read(cx).favourites.inspecting {
            return;
        }
        let entries = self.entries();
        let visible: Vec<_> = self
            .laid
            .borrow()
            .rows
            .iter()
            .filter_map(|row| entries.get(row.index)?.favourite_index)
            .filter_map(|index| {
                self.places
                    .read(cx)
                    .favourites
                    .records
                    .get(index)
                    .map(|record| (index, record.clone()))
            })
            .collect();
        if visible != self.inspected_visible {
            let indices: Vec<_> = visible.iter().map(|(index, _)| *index).collect();
            self.inspected_visible = visible;
            self.places
                .update(cx, |places, cx| places.inspect(&indices, cx));
        }
    }

    /// U19 and U20 publish complete device and network sections through this seam.
    pub fn set_tail(&mut self, sections: Vec<RailSection>, cx: &mut Context<Self>) {
        self.tail = sections;
        self.rebuild(cx);
    }

    /// A committed listing adds only the tags it actually carries; the rail keeps their session union.
    pub fn tags_seen(&mut self, seen: &[bool; 7], cx: &mut Context<Self>) {
        if !seen
            .iter()
            .zip(self.places.read(cx).tags_in_use)
            .any(|(seen, used)| *seen && !used)
        {
            return;
        }
        self.places.update(cx, |places, cx| {
            for (used, seen) in places.tags_in_use.iter_mut().zip(seen) {
                *used |= *seen;
            }
            cx.notify();
        });
        self.rebuild(cx);
    }

    #[cfg(test)]
    pub(crate) fn fixture_home(&mut self, home: &str, dirs: &str, cx: &mut Context<Self>) {
        self.home = home.to_owned();
        self.dirs = dirs.to_owned();
        self.loaded = true;
        self.rebuild(cx);
    }

    /// The desktop's recent-files history, named from the environment the way the Recent row reads it.
    pub fn history_file(&self) -> String {
        flea::ui_logic_pl4::recent::history_path(
            &std::env::var("XDG_DATA_HOME").unwrap_or_default(),
            &self.home,
        )
    }

    pub fn read_recent(&mut self, path: String, cx: &mut Context<Self>) {
        // Places accepts only its newest read; a new ask cancels the older pending ticket.
        self.recent_pending = Some(self.request(Event::Recent(Vec::new())));
        self.history_seen = self.places.read(cx).history_reads;
        self.places
            .update(cx, |places, cx| places.reread_history(path, cx));
    }

    /// A navigation on the side whose ticket asked drops that ask, so its late reply finds no ticket.
    pub fn cancel_recent(&mut self, side: usize) {
        if self
            .recent_pending
            .as_ref()
            .is_some_and(|ticket| ticket.side == side)
        {
            self.recent_pending = None;
        }
    }

    pub fn replace_sections(&mut self, sections: Vec<RailSection>, cx: &mut Context<Self>) {
        if sections == self.sections {
            return;
        }
        self.cancel_rename(cx);
        let after = flat(&sections);
        let cursor = flea::ui_logic_pl4::placeview::rail_cursor_after(
            &self.entries(),
            &after,
            self.cursor(),
        )
        .filter(|index| *index < after.len());
        self.sections = sections;
        self.provider.update(
            &self.catalog,
            self.sections.clone(),
            theme_tags(&self.theme),
        );
        let mut model = self.model.borrow_mut();
        model.sections = Rc::new(self.provider.drawn.clone());
        model.reveal_cursor |= cursor.is_some() && model.cursor != cursor;
        model.cursor = cursor;
        *self.laid.borrow_mut() = Laid::default();
        drop(model);
        cx.notify();
    }

    pub(super) fn service_changed(&mut self, cx: &mut Context<Self>) {
        let service = self.places.read(cx);
        let reads = service.history_reads;
        let visits = service.history.clone();
        self.rebuild(cx);
        if reads > self.history_seen {
            self.history_seen = reads;
            if let Some(mut request) = self.recent_pending.take() {
                request.event = Event::Recent(visits);
                self.events.borrow_mut().push(request);
            }
        }
        cx.notify();
    }
}
