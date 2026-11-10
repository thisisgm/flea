//! A bounded record of a mark successfully submitted by the current row painter.
use flea::jsondoc::Json;
use gpui::{Bounds, Hsla, Pixels, SharedString};

pub(crate) struct Painted {
    pub index: usize,
    pub key: SharedString,
    pub data: &'static [u8],
    pub ink: Hsla,
    pub slot: Bounds<Pixels>,
    pub label_x: f32,
    pub cursor: bool,
    pub hovered: bool,
    pub cut: bool,
    pub opacity: f32,
}

impl Painted {
    pub(crate) fn json(&self, name: &str) -> Json {
        let slug = self.key.strip_prefix("flea-lang:").unwrap_or("");
        let built = !slug.is_empty();
        // Sample input: <svg viewBox="0 0 24 24"><path d="M4 7V2h10l6 6v14H17"/></svg>.
        let outline = std::str::from_utf8(self.data)
            .ok()
            .and_then(|svg| svg.split_once("<path d=\""))
            .and_then(|(_, tail)| tail.split_once('"'))
            .map_or("", |(path, _)| path);
        let ink = crate::ipc::rows::colour(self.ink);
        crate::visual::object([
            ("name", Json::Str(name.into())),
            ("slug", Json::Str(slug.into())),
            ("built", Json::Bool(built)),
            ("markVisible", Json::Bool(built)),
            ("glyphVisible", Json::Bool(!built)),
            ("ink", Json::Str(ink.clone())),
            ("glyphInk", Json::Str(ink)),
            ("alpha", Json::Num(self.opacity.to_string())),
            (
                "markSize",
                Json::Num(f32::from(self.slot.size.width).to_string()),
            ),
            (
                "slot",
                Json::Arr(
                    [
                        self.slot.origin.x,
                        self.slot.origin.y,
                        self.slot.size.width,
                        self.slot.size.height,
                    ]
                    .into_iter()
                    .map(|v| Json::Num(f32::from(v).to_string()))
                    .collect(),
                ),
            ),
            ("labelX", Json::Num(self.label_x.to_string())),
            ("page", Json::Str(if built { outline } else { "" }.into())),
            ("cursor", Json::Bool(self.cursor)),
            ("hovered", Json::Bool(self.hovered)),
            (
                "clip",
                Json::Str(if self.cut { "scissors" } else { "" }.into()),
            ),
            ("thumb", Json::Bool(false)),
        ])
    }
}
