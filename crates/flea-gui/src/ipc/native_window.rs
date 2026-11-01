//! Native launch observers read the window and its active list without starting producer work.
use super::Adapter;
use crate::g2::pl4::rows::{RAIL_ROW_SCALE, TEXT_LINE_HEIGHT_FACTOR};
use crate::list::{MARK_RATIO, STOCK_BASE};
use crate::{
    list::{columns::Column, ListState},
    palette::{ParsedTheme, Theme},
};
use flea::ipc::wire::Reply;
use gpui::{Pixels, Size};

// Theme.qml mark ratios: state and hero scale bodySmall, chrome is Style.font.icon (14 x 1.167); one 2-unit stroke.
const STATE_MARK_RATIO: f64 = 3.1;
const HERO_MARK_RATIO: f64 = 3.7;
const CHROME_MARK_RATIO: f64 = 1.167;
const STROKE_WIDTH: &str = "2";
// Theme.qml dialogWidthRatio widens the Settings panel (480) and pane (350) board widths by 9 / 8.
const DIALOG_WIDTH_RATIO: f64 = 9. / 8.;
const SETTINGS_PANEL_SPACE: f64 = 480.;
const SETTINGS_PANE_SPACE: f64 = 350.;
// Theme.qml menuWidth, grid minCellWidth and preview fraction, at the stock base.
const MENU_WIDTH_SPACE: f64 = 220.;
const GRID_MIN_CELL_SPACE: f64 = 125.;
const PREVIEW_FRACTION: &str = "0.82";
// Board pixels drawn at base 14, whose bodySmall is 13: pickerDateBaseWidth 80 and the rail's railPaddingY 10.
const BOARD_BODY_SMALL: f64 = 13.;
const PICKER_DATE_BOARD_WIDTH: f64 = 80.;
const SETTINGS_RAIL_PADDING_BOARD_Y: f64 = 10.;

pub struct NativeWindowAdapter<'a> {
    pub list: &'a ListState,
    pub theme: Theme,
    pub parsed: ParsedTheme,
    pub viewport: Size<Pixels>,
    pub corner: f32,
}

impl Adapter for NativeWindowAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            "tokens" => return Some(self.tokens()),
            "wheelLines" => crate::input::WHEEL_LINES.to_string(),
            // Theme.railIconSize, the slot a rail row's icon draws in; the same value tokens names railIconSize.
            "railIconSize" => self.rail_icon_size().to_string(),
            "palette" => {
                if !self.parsed.0 {
                    return Some(Reply::error(
                        3,
                        "FLEA_IPC_UNAVAILABLE flea.palette: theme not loaded",
                    ));
                }
                // Root inherits these roles directly from the mounted theme, without a background override.
                let style = gpui::TextStyle::default();
                let palette = crate::list::Palette::inherited(&style, self.theme);
                let ink = crate::g2::st1::Ink::inherited(&style, self.theme);
                [
                    palette.background,
                    self.theme.surface,
                    palette.foreground,
                    ink.muted,
                    palette.accent,
                    ink.error,
                    palette.symlink,
                    palette.executable,
                    palette.accent_frame,
                    palette.muted_frame,
                ]
                .into_iter()
                .map(super::rows::colour)
                .collect::<Vec<_>>()
                .join(" ")
            }
            "bodyPx" => f32::from(self.list.typeface.body).round().to_string(),
            "metrics" => {
                let metrics = crate::list::Metrics::for_typeface(&self.list.typeface);
                format!(
                    "{} {} {} {}",
                    f32::from(self.list.typeface.body_small).round(),
                    f32::from(self.list.typeface.caption).round(),
                    metrics.pad,
                    metrics.nominal_row
                )
            }
            // Grid and Columns retain stale list paints, so Theme.qml fileRowHeight follows current density.
            "fileRowHeight" => self.list.file_metrics().row.round().to_string(),
            "themeLoaded" => self.parsed.0.to_string(),
            "themeForeground" => super::rows::colour(self.theme.foreground),
            "windowSize" => format!(
                "{} {}",
                f32::from(self.viewport.width).round(),
                f32::from(self.viewport.height).round()
            ),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

impl NativeWindowAdapter<'_> {
    fn rail_icon_size(&self) -> f64 {
        let metrics = crate::list::Metrics::for_typeface(&self.list.typeface);
        let row = f64::from(metrics.nominal_row);
        let padding = f64::from((metrics.nominal_row - metrics.slot) / 2.);
        (row * RAIL_ROW_SCALE).round() - 2. * padding
    }

    fn tokens(&self) -> Reply {
        if !self.parsed.0 {
            return Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.tokens: theme not loaded");
        }
        let Some(columns) = self.list.observed_column_tokens() else {
            return Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.tokens: font not measured");
        };
        let face = &self.list.typeface;
        let metrics = crate::list::Metrics::for_typeface(face);
        let base = f64::from(f32::from(face.base));
        let small = f64::from(f32::from(face.body_small));
        let space = |value: f64| (value * base / f64::from(STOCK_BASE)).round();
        let row = f64::from(metrics.nominal_row);
        let padding = f64::from((metrics.nominal_row - metrics.slot) / 2.);
        let rail_row = (row * RAIL_ROW_SCALE).round();
        let panel = (space(SETTINGS_PANEL_SPACE) * DIALOG_WIDTH_RATIO).round();
        let pane = (space(SETTINGS_PANE_SPACE) * DIALOG_WIDTH_RATIO).round();
        let column = |column| columns.wanted(column, self.list.view.stored).to_string();
        // The ordered 34-key contract is Theme.tokens() and tools/flea-metrics-gate's token table.
        let values = [
            ("family", face.font.family.to_string()),
            ("baseSize", base.to_string()),
            ("body", f32::from(face.body).to_string()),
            ("bodySmall", small.to_string()),
            ("caption", f32::from(face.caption).to_string()),
            ("lineBoxRatio", TEXT_LINE_HEIGHT_FACTOR.to_string()),
            ("rowPaddingX", metrics.pad.to_string()),
            ("rowPaddingY", padding.to_string()),
            ("gap", metrics.gap.to_string()),
            ("hairline", metrics.hairline.to_string()),
            ("rowHeight", row.to_string()),
            ("iconSize", metrics.slot.to_string()),
            ("markSize", (small * MARK_RATIO).to_string()),
            (
                "stateMarkSize",
                (small * STATE_MARK_RATIO).round().to_string(),
            ),
            (
                "heroMarkSize",
                (small * HERO_MARK_RATIO).round().to_string(),
            ),
            ("strokeWidth", STROKE_WIDTH.into()),
            ("railRowHeight", rail_row.to_string()),
            ("railIconSize", self.rail_icon_size().to_string()),
            ("chromeHeight", metrics.header.to_string()),
            (
                "chromeMarkSize",
                (base * CHROME_MARK_RATIO).round().to_string(),
            ),
            ("columnMode", column(Column::Mode)),
            ("columnSize", column(Column::Size)),
            ("columnDate", column(Column::Date)),
            (
                "columnPickerDate",
                (PICKER_DATE_BOARD_WIDTH * small / BOARD_BODY_SMALL)
                    .round()
                    .to_string(),
            ),
            ("columnKind", column(Column::Kind)),
            ("menuWidth", space(MENU_WIDTH_SPACE).to_string()),
            ("cornerRadius", self.corner.to_string()),
            ("previewFraction", PREVIEW_FRACTION.into()),
            ("gridIconSize", (metrics.slot * 2.).to_string()),
            ("gridMinCellWidth", space(GRID_MIN_CELL_SPACE).to_string()),
            ("settingsPanelWidth", panel.to_string()),
            (
                "settingsRailWidth",
                (panel - pane - 2. * f64::from(metrics.hairline)).to_string(),
            ),
            ("settingsPaneWidth", pane.to_string()),
            (
                "settingsRailPaddingY",
                (SETTINGS_RAIL_PADDING_BOARD_Y * small / BOARD_BODY_SMALL)
                    .round()
                    .to_string(),
            ),
        ];
        Reply::value(
            values
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}
