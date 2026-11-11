//! Visible rail geometry, measured from prepared text before paint.
use super::*;

#[cfg(test)]
#[path = "visual_geometry_tests.rs"]
mod visual_geometry_tests;

// HIG row text sits one logical pixel above the centred font box, independent of its icon and menu anchor.
const ROW_TEXT_LIFT: f32 = 1.;

/// Lays out every heading and row once per paint and publishes the boxes the IPC readers answer with.
pub(super) fn layout(
    sections: &[DrawnSection],
    dimensions: (f32, f32),
    typeface: &Typeface,
    detail_face: &Typeface,
    model: &RowsModel,
    window: &Window,
    cache: &mut text::Cache,
) -> Laid {
    let (width, height) = dimensions;
    let metrics = RailMetrics::for_typeface(typeface);
    let mut laid = Laid {
        painted: true,
        width,
        height,
        row_gap: metrics.gap,
        ..Laid::default()
    };
    let mut y = metrics.top - model.scroll;
    let mut index = 0usize;
    for (slide, section) in sections.iter().enumerate() {
        let first = slide == 0;
        if section.heading.is_empty() {
            if !first {
                y += metrics.panel_gap;
            }
        } else {
            let group_gap = if first { 0. } else { metrics.panel_gap };
            let tracking = if section.caseless {
                0.
            } else {
                HEADING_TRACKING
            };
            let run = cache.get(
                &section.heading,
                typeface.caption,
                width - 2. * metrics.pad,
                tracking,
                typeface,
                window,
            );
            // The board's heading pitch keeps both rounded font extents; row text is positioned independently.
            let text_height = run.ascent.ceil() + (run.height - run.ascent).ceil();
            let (top_padding, height) = if let Some(heading) = metrics.heading {
                (
                    group_gap + (heading - text_height) / 2.,
                    group_gap + heading,
                )
            } else {
                let lead = (f32::from(typeface.caption) * HEADING_LEAD).ceil() + group_gap;
                (lead, lead + text_height + metrics.gap)
            };
            let text_width = run.width;
            let rect = Rect::new(metrics.pad, y, text_width, height);
            let network = section
                .rows
                .first()
                .is_some_and(|row| row.group == flea::ui_logic_rail_places::rail::Group::Network);
            let add = network.then(|| {
                Rect::new(
                    width - metrics.pad - metrics.slot,
                    y + top_padding + (text_height - metrics.slot) / 2.,
                    metrics.slot,
                    metrics.slot,
                )
            });
            laid.headers.push(LaidHeader {
                text: section.heading.to_string(),
                rect,
                top_padding,
                text_height,
                font_size: f32::from(typeface.caption),
                tracking,
                add,
            });
            y += rect.height;
        }
        for row in &section.rows {
            let top = y;
            if Some(index) == model.cursor {
                laid.cursor_y = Some(top);
            }
            if top + metrics.row <= 0. || top >= height {
                index += 1;
                y += metrics.row;
                continue;
            }
            // SidebarRow.qml: a drop cue replaces the trailing detail and the slot, and reserves its caption's width.
            let cue = model
                .drop_target
                .as_ref()
                .filter(|(target, _)| *target == index)
                .map(|(_, label)| label.to_string());
            let dropbox = row.kind == flea::ui_logic_rail_places::rail::Kind::Dropbox;
            let plugin_state = row.kind == flea::ui_logic_rail_places::rail::Kind::PluginRow
                && row.mounted.is_some();
            let dot =
                cue.is_none() && !dropbox && (indicator_slot(row.group, row.kind) || plugin_state);
            // RailDetails: a drive size ends on one x for every device row, the slot kept after it with or without a square.
            let size = cue.is_none()
                && model.drive_size_shown
                && row.group == flea::ui_logic_rail_places::rail::Group::Device
                && !row.size.is_empty();
            let slot_width = if dot || size { metrics.slot } else { 0. };
            let detail = if let Some(cue) = cue {
                cue
            } else if row.kind == flea::ui_logic_rail_places::rail::Kind::Trash
                && model.trash_count_shown
                && model.trash_count > 0
            {
                model.trash_count.to_string()
            } else if size {
                row.size.to_string()
            } else {
                String::new()
            };
            let detail_run =
                cache.get(&detail, detail_face.caption, width, 0., detail_face, window);
            let detail_width = if detail.is_empty() {
                0.
            } else {
                detail_run.width
            };
            // SidebarRow.qml: the label ends one gap before the detail, and the detail one gap before a slot.
            let slot_gap = if slot_width > 0. { metrics.gap } else { 0. };
            let detail_gap = if detail_width > 0. { metrics.gap } else { 0. };
            let right = width - metrics.pad - slot_width - slot_gap - detail_width - detail_gap;
            let label_left =
                metrics.pad + metrics.icon * if dropbox { 1.18 } else { 1. } + metrics.gap;
            let label_width = (right - label_left).max(0.);
            let label_run = cache.get(&row.label, typeface.body, label_width, 0., typeface, window);
            let needs = label_run.width;
            let label_height = label_run.height;
            let label_rect = Rect::new(
                label_left,
                top + (metrics.row - label_height) / 2. - ROW_TEXT_LIFT,
                label_width,
                label_height,
            );
            let detail_rect = Rect::new(
                width - metrics.pad - slot_width - slot_gap - detail_width,
                top + (metrics.row - detail_run.height) / 2. - ROW_TEXT_LIFT,
                detail_width,
                detail_run.height,
            );
            let indicator_rect = Rect::new(
                width - metrics.pad - slot_width,
                top + (metrics.row - metrics.slot) / 2.,
                slot_width,
                metrics.slot,
            );
            let editor = (model.renaming == Some(index))
                .then(|| Rect::new(label_left, top + 3., label_width, metrics.row - 6.));
            laid.rows.push(LaidRow {
                index,
                label: row.label.to_string(),
                rect: Rect::new(0., top, width, metrics.row),
                label_rect,
                detail_rect,
                indicator_rect,
                indicator_visible: dot,
                eject: dot && row.releasable,
                label_width,
                label_needs: needs,
                elided: needs > label_width,
                detail,
                detail_color: String::new(),
                detail_width,
                font_size: f32::from(typeface.caption),
                glyph: row.glyph,
                owned: row.owned.clone(),
                mark: row.mark,
                error: row.error.is_some(),
                mounted: row.mounted,
                editor,
            });
            index += 1;
            y += metrics.row;
        }
    }
    laid.content_height = y + model.scroll + metrics.top;
    laid
}

impl RailElement {
    pub(super) fn prepaint_rows(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Ready {
        let theme = crate::window::glass::producers::chrome_theme(window, cx);
        let palette = rail_palette(&window.text_style(), theme);
        let metrics = RailMetrics::for_typeface(&self.typeface);
        let mut model = self.model.borrow_mut();
        let mut cache = std::mem::take(&mut model.cache);
        let mut laid = super::layout::layout(
            &model.sections,
            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
            &self.typeface,
            &self.detail_face,
            &model,
            window,
            &mut cache,
        );
        if let Some(top) = laid.cursor_y.filter(|_| model.reveal_cursor) {
            if top < 0. {
                model.scroll = (model.scroll + top).max(0.);
            } else if top + metrics.row > laid.height {
                model.scroll += top + metrics.row - laid.height;
            }
        }
        model.reveal_cursor = false;
        model.scroll = model
            .scroll
            .min((laid.content_height - laid.height).max(0.));
        laid = super::layout::layout(
            &model.sections,
            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
            &self.typeface,
            &self.detail_face,
            &model,
            window,
            &mut cache,
        );
        let (dx, dy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
        for header in &mut laid.headers {
            header.rect.x += dx;
            header.rect.y += dy;
            if let Some(add) = &mut header.add {
                add.x += dx;
                add.y += dy;
            }
        }
        for row in &mut laid.rows {
            row.rect.x += dx;
            row.rect.y += dy;
            row.label_rect.x += dx;
            row.label_rect.y += dy;
            row.detail_rect.x += dx;
            row.detail_rect.y += dy;
            row.indicator_rect.x += dx;
            row.indicator_rect.y += dy;
            if let Some(editor) = &mut row.editor {
                editor.x += dx;
                editor.y += dy;
            }
        }
        let headers = laid
            .headers
            .iter()
            .map(|h| {
                cache.get(
                    &h.text,
                    self.typeface.caption,
                    laid.width - 2. * metrics.pad,
                    h.tracking,
                    &self.typeface,
                    window,
                )
            })
            .collect();
        let labels: Vec<Rc<text::Prepared>> = laid
            .rows
            .iter()
            .map(|row| {
                let text = if model.renaming == Some(row.index) {
                    &model.editor.text
                } else {
                    &row.label
                };
                cache.get(
                    text,
                    self.typeface.body,
                    if model.renaming == Some(row.index) {
                        f32::INFINITY
                    } else {
                        row.label_width
                    },
                    0.,
                    &self.typeface,
                    window,
                )
            })
            .collect();
        model.editor_line = model
            .renaming
            .and_then(|index| laid.rows.iter().position(|row| row.index == index))
            .map(|at| Rc::clone(&labels[at]));
        if let Some(rect) = model
            .renaming
            .and_then(|index| laid.row(index))
            .and_then(|row| row.editor)
        {
            let room = (rect.width - 8.).max(0.);
            let caret = model.editor_x(model.editor.cursor());
            if caret - model.editor_scroll > room {
                model.editor_scroll = caret - room;
            } else if caret < model.editor_scroll {
                model.editor_scroll = caret;
            }
        }
        let details = laid
            .rows
            .iter_mut()
            .map(|row| {
                let ink = super::paint::detail_ink(&model, row.index, &palette);
                row.detail_color = crate::ipc::rows::colour(ink);
                cache.get(
                    &row.detail,
                    self.detail_face.caption,
                    laid.width,
                    0.,
                    &self.detail_face,
                    window,
                )
            })
            .collect();
        let digit_face = Typeface {
            font: gpui::Font {
                weight: gpui::FontWeight::BOLD,
                ..self.typeface.font.clone()
            },
            ..self.typeface.clone()
        };
        let digits = laid
            .rows
            .iter()
            .map(|row| match row.mark {
                RowMark::Tag(tag) if tag.fill.is_none() => {
                    Some(cache.get(tag.digit, px(10.), TAG, 0., &digit_face, window))
                }
                _ => None,
            })
            .collect();
        cache.bound(((laid.height / metrics.row).ceil() as usize + 2) * 12);
        model.cache = cache;
        *self.laid.borrow_mut() = laid;
        Ready {
            palette,
            headers,
            labels,
            details,
            digits,
        }
    }
}
