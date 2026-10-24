//! Visible rail geometry, measured from prepared text before paint.
use super::*;

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
    let mut y = metrics.pad - model.scroll;
    let mut index = 0usize;
    for (slide, section) in sections.iter().enumerate() {
        let first = slide == 0;
        if section.heading.is_empty() {
            if !first {
                y += metrics.panel_gap;
            }
        } else {
            let top_padding = (f32::from(typeface.caption) * HEADING_LEAD).ceil()
                + if first { 0. } else { metrics.panel_gap };
            let run = cache.get(
                &section.heading,
                typeface.caption,
                width - 2. * metrics.pad,
                HEADING_TRACKING,
                typeface,
                window,
            );
            // Keep the shaped run; only the heading's line box uses integer ascent and descent.
            let text_height = run.ascent.ceil() + (run.height - run.ascent).ceil();
            let text_width = run.width;
            let rect = Rect::new(
                metrics.pad,
                y,
                text_width,
                top_padding + text_height + metrics.gap,
            );
            let network = section
                .rows
                .first()
                .is_some_and(|row| row.group == flea::ui_logic_pl4::rail::Group::Network);
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
                font_size: f32::from(typeface.caption),
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
            let dot = indicator_slot(row.group, row.kind);
            let slot_width = if dot { metrics.slot } else { 0. };
            let detail = if row.kind == flea::ui_logic_pl4::rail::Kind::Trash
                && model.trash_count_shown
                && model.trash_count > 0
            {
                model.trash_count.to_string()
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
            let right = width
                - metrics.pad
                - slot_width
                - detail_width
                - if slot_width > 0. || detail_width > 0. {
                    metrics.gap
                } else {
                    0.
                };
            let label_left = metrics.pad + metrics.icon + metrics.gap;
            let label_width = (right - label_left).max(0.);
            let label_run = cache.get(&row.label, typeface.body, label_width, 0., typeface, window);
            let needs = label_run.width;
            let label_height = label_run.height;
            let label_rect = Rect::new(
                label_left,
                top + (metrics.row - label_height) / 2.,
                label_width,
                label_height,
            );
            let detail_rect = Rect::new(
                width - metrics.pad - detail_width,
                top + (metrics.row - detail_run.height) / 2.,
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
                label_width,
                label_needs: needs,
                elided: needs > label_width,
                detail,
                detail_color: String::new(),
                detail_width,
                font_size: f32::from(typeface.caption),
                glyph: row.glyph,
                mark: row.mark,
                error: row.error.is_some(),
                mounted: row.mounted,
                editor,
            });
            index += 1;
            y += metrics.row;
        }
    }
    laid.content_height = y + model.scroll + metrics.pad;
    laid
}
