//! The ShareBrowser read-back readers on the IPC seam, as ui/Ipc.qml answers shareBrowser*.
use super::view::Probe;
use crate::ipc::Adapter;
use flea::ipc::wire::Reply;
use flea::json::escape;
use flea::ui_logic_sd1::share::ShareBrowser;
use gpui::Bounds;

pub struct BrowserAdapter<'a> {
    pub browser: &'a ShareBrowser,
    pub probe: &'a Probe,
    // The window's offset, so a rect is in window pixels as the QML reader gave it.
    pub origin: (f32, f32),
}

impl BrowserAdapter<'_> {
    // The overlay's rect while it is drawn, as "x y w h"; empty while shut, as rectOf() of a hidden item reads.
    fn rect(&self) -> String {
        match (self.browser.active, self.probe.bounds) {
            (true, Some(bounds)) => rect_text(bounds, self.origin),
            _ => String::new(),
        }
    }
}

fn rect_text(bounds: Bounds<gpui::Pixels>, origin: (f32, f32)) -> String {
    format!(
        "{} {} {} {}",
        (f32::from(bounds.origin.x) + origin.0).round(),
        (f32::from(bounds.origin.y) + origin.1).round(),
        f32::from(bounds.size.width).round(),
        f32::from(bounds.size.height).round()
    )
}

impl Adapter for BrowserAdapter<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        let value = match function {
            "shareBrowserOpen" => self.browser.active.to_string(),
            "shareBrowserEntries" => self.browser.entries_text(),
            "shareBrowserCursor" => self.browser.cursor_ipc().to_string(),
            "shareBrowserRect" => self.rect(),
            "shareBrowserState" => format!(
                "{{\"active\":{},\"baseUri\":\"{}\",\"cursor\":{},\"rect\":\"{}\"}}",
                self.browser.active,
                escape(&self.browser.base_uri),
                self.browser.cursor_ipc(),
                self.rect()
            ),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
