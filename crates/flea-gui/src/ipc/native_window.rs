//! Native launch observers read the window and its active list without starting producer work.
use super::Adapter;
use crate::{
    list::ListState,
    palette::{ParsedTheme, Theme},
};
use flea::ipc::wire::Reply;
use gpui::{Pixels, Size};

pub struct NativeWindowAdapter<'a> {
    pub list: &'a ListState,
    pub theme: Theme,
    pub parsed: ParsedTheme,
    pub viewport: Size<Pixels>,
}

impl Adapter for NativeWindowAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
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
