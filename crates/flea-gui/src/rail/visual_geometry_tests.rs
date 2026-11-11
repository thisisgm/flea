//! HIG rail line boxes, including the extra group and the surfaces without ordinary chrome.
use super::*;
use crate::g2::rail_places::content::DrawnRow;
use flea::ui_logic_rail_places::rail::{Group, Kind};
use gpui::AppContext;

fn section(heading: &'static str, group: Group, labels: &[&str]) -> DrawnSection {
    DrawnSection {
        heading: heading.into(),
        caseless: false,
        rows: labels
            .iter()
            .map(|label| DrawnRow {
                label: label.to_string().into(),
                glyph: "house",
                owned: None,
                mark: RowMark::None,
                group,
                kind: Kind::Home,
                mounted: None,
                releasable: false,
                error: None,
                size: "".into(),
            })
            .collect(),
    }
}

#[test]
fn visual_geometry_rail_metrics_interpolate_and_then_scale() {
    for (base, top, heading, gap, row) in [
        (15, 9., 35., 11., 31.),
        (16, 9., 36., 11., 33.),
        (17, 9., 38., 11., 35.),
        (18, 8., 39., 10., 37.),
        (19, 8., 41., 10., 39.),
        (20, 8., 42., 10., 41.),
        (24, 10., 50., 12., 49.),
        (28, 11., 59., 14., 57.),
    ] {
        let metrics = RailMetrics::for_typeface(&Typeface::omarchy_default().at_base(base));
        let (pad, horizontal_gap) = if base <= 20 {
            (14., 9.)
        } else {
            (
                (14. * base as f32 / 20.).round(),
                (9. * base as f32 / 20.).round(),
            )
        };
        assert_eq!(
            (metrics.pad, metrics.gap),
            (pad, horizontal_gap),
            "size {base}"
        );
        assert_eq!(
            (metrics.top, metrics.heading, metrics.panel_gap, metrics.row),
            (top, Some(heading), gap, row),
            "size {base}"
        );
    }
    for base in [10, 12, 14] {
        let face = Typeface::omarchy_default().at_base(base);
        let list = crate::list::Metrics::for_typeface(&face);
        let metrics = RailMetrics::for_typeface(&face);
        assert_eq!(metrics.top, list.pad);
        assert_eq!(metrics.heading, None);
        assert_eq!((metrics.pad, metrics.gap), (list.pad, list.gap));
        assert_eq!(
            metrics.row,
            (list.nominal_row * RAIL_ROW_SCALE as f32).round()
        );
        assert_eq!(
            metrics.panel_gap,
            (PANEL_GAP * base as f32 / OEM_BASE).round()
        );
    }
}

#[test]
fn visual_geometry_size20_rail_uses_the_compressed_board_bands() {
    let text_system = gpui_platform::current_platform(true).text_system();
    text_system
        .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
            "../../tests/fonts/JetBrainsMono-Regular.ttf"
        ))])
        .expect("the HIG font fixture loads");
    let mut cx = gpui::TestAppContext::build_with_text_system(
        gpui::TestDispatcher::new(0),
        None,
        text_system,
    );
    let handle = cx.add_window(|_, _| gpui::Empty);
    cx.update_window(handle.into(), |_, window, _| {
        let face = Typeface {
            font: gpui::font("JetBrains Mono"),
            ..Typeface::omarchy_default().at_base(20)
        };
        let sections = [
            section("PLACES", Group::Home, &["Home", "Trash"]),
            section("NETWORK", Group::Network, &["NAS"]),
            section("DEVICES", Group::Device, &["QEMU HARDDISK"]),
        ];
        let laid = layout(
            &sections,
            (274., 626.),
            &face,
            &face,
            &RowsModel::default(),
            window,
            &mut text::Cache::default(),
        );
        assert_eq!(laid.headers.len(), 3);
        assert_eq!(laid.rows.len(), 4);
        for header in &laid.headers {
            assert_eq!(header.rect.x, 14., "{} heading inset", header.text);
        }
        for (header, (top, height)) in
            laid.headers
                .iter()
                .zip([(8., 42.), (132., 52.), (225., 52.)])
        {
            assert_eq!(
                (header.rect.y, header.rect.height),
                (top, height),
                "{}",
                header.text
            );
        }
        for (row, top) in laid.rows.iter().zip([50., 91., 184., 277.]) {
            assert_eq!((row.rect.y, row.rect.height), (top, 41.), "{}", row.label);
            assert_eq!(row.label_rect.x, 44., "{} label inset", row.label);
        }
    })
    .unwrap();
}

#[test]
fn visual_geometry_rail_board_boxes_do_not_compress_later_groups() {
    let text_system = gpui_platform::current_platform(true).text_system();
    text_system
        .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
            "../../tests/fonts/JetBrainsMono-Regular.ttf"
        ))])
        .expect("the HIG font fixture loads");
    let mut cx = gpui::TestAppContext::build_with_text_system(
        gpui::TestDispatcher::new(0),
        None,
        text_system,
    );
    let handle = cx.add_window(|_, _| gpui::Empty);
    cx.update_window(handle.into(), |_, window, _| {
        let face = Typeface {
            font: gpui::font("JetBrains Mono"),
            ..Typeface::omarchy_default()
        };
        for favorites in [false, true] {
            let mut sections = vec![section("PLACES", Group::Home, &["Home", "Trash"])];
            if favorites {
                sections.push(section("FAVORITES", Group::Favorite, &["Work"]));
            }
            sections.extend([
                section("NETWORK", Group::Network, &["NAS"]),
                section("DEVICES", Group::Device, &["QEMU HARDDISK"]),
            ]);
            let laid = layout(
                &sections,
                (192., 673.),
                &face,
                &face,
                &RowsModel::default(),
                window,
                &mut text::Cache::default(),
            );
            let headers = if favorites {
                vec![14., 100., 173., 246.]
            } else {
                vec![14., 100., 173.]
            };
            assert_eq!(laid.headers.len(), headers.len());
            for (header, top) in laid.headers.iter().zip(headers) {
                assert_eq!(header.rect.y, top, "{} heading top", header.text);
                assert_eq!(header.rect.height, if top == 14. { 28. } else { 44. });
            }
            let rows = if favorites {
                vec![42., 71., 144., 217., 290.]
            } else {
                vec![42., 71., 144., 217.]
            };
            assert_eq!(laid.rows.len(), rows.len());
            for (row, top) in laid.rows.iter().zip(rows) {
                assert_eq!(row.rect.y, top, "{} row and menu anchor", row.label);
                assert_eq!(row.rect.height, 29.);
            }
            let home = &laid.rows[0];
            let run = text::Cache::default().get("Home", face.body, 150., 0., &face, window);
            assert!((home.label_rect.y - (42. + (29. - run.height) / 2. - 1.)).abs() < 0.001);
        }
    })
    .unwrap();
}
