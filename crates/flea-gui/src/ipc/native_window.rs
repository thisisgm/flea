//! Native launch observers read the window and its active list without starting producer work.
use super::Adapter;
use crate::{
    list::{columns::Column, ListState},
    palette::{ParsedTheme, Theme},
};
use flea::ipc::wire::Reply;
use gpui::{Pixels, Size};

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
            "fileRowHeight" => {
                let row = if self.list.drawn.frames == 0 {
                    crate::list::Metrics::for_typeface(&self.list.typeface).row
                } else {
                    self.list.drawn.metrics.row
                };
                row.round().to_string()
            }
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
        let space = |value: f64| (value * base / 12.).round();
        let row = f64::from(metrics.nominal_row);
        let padding = f64::from((metrics.nominal_row - metrics.slot) / 2.);
        let rail_row = (row * 0.78).round();
        let panel = (space(480.) * 9. / 8.).round();
        let pane = (space(350.) * 9. / 8.).round();
        let column = |column| columns.wanted(column, self.list.view.stored).to_string();
        // The ordered 34-key contract is Theme.tokens() and tools/flea-metrics-gate's token table.
        let values = [
            ("family", face.font.family.to_string()),
            ("baseSize", base.to_string()),
            ("body", f32::from(face.body).to_string()),
            ("bodySmall", small.to_string()),
            ("caption", f32::from(face.caption).to_string()),
            ("lineBoxRatio", "1.8".into()),
            ("rowPaddingX", metrics.pad.to_string()),
            ("rowPaddingY", padding.to_string()),
            ("gap", metrics.gap.to_string()),
            ("hairline", metrics.hairline.to_string()),
            ("rowHeight", row.to_string()),
            ("iconSize", metrics.slot.to_string()),
            ("markSize", (small * 1.45).to_string()),
            ("stateMarkSize", (small * 3.1).round().to_string()),
            ("heroMarkSize", (small * 3.7).round().to_string()),
            ("strokeWidth", "2".into()),
            ("railRowHeight", rail_row.to_string()),
            ("railIconSize", (rail_row - 2. * padding).to_string()),
            ("chromeHeight", metrics.header.to_string()),
            ("chromeMarkSize", (base * 1.167).round().to_string()),
            ("columnMode", column(Column::Mode)),
            ("columnSize", column(Column::Size)),
            ("columnDate", column(Column::Date)),
            ("columnPickerDate", (80. * small / 13.).round().to_string()),
            ("columnKind", column(Column::Kind)),
            ("menuWidth", space(220.).to_string()),
            ("cornerRadius", self.corner.to_string()),
            ("previewFraction", "0.82".into()),
            ("gridIconSize", (metrics.slot * 2.).to_string()),
            ("gridMinCellWidth", space(125.).to_string()),
            ("settingsPanelWidth", panel.to_string()),
            (
                "settingsRailWidth",
                (panel - pane - 2. * f64::from(metrics.hairline)).to_string(),
            ),
            ("settingsPaneWidth", pane.to_string()),
            (
                "settingsRailPaddingY",
                (10. * small / 13.).round().to_string(),
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
