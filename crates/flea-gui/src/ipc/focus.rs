//! Focus, band and scrollbar readers observe the live owner and its most recently painted viewport.
use super::{
    rows::{colour, rect_of},
    Adapter,
};
use crate::input::PaneInput;
use flea::{ipc::wire::Reply, jsondoc::Json};
use gpui::{Bounds, Pixels};

pub struct FocusAdapter<'a> {
    pub view: Option<&'a str>,
    pub input: &'a PaneInput,
}
fn number(value: f32) -> Json {
    Json::Num(value.to_string())
}
fn point(value: (f32, f32)) -> Json {
    Json::Obj(vec![
        ("x".into(), number(value.0)),
        ("y".into(), number(value.1)),
    ])
}
fn rect(bounds: Bounds<Pixels>) -> String {
    rect_of(
        bounds.left().into(),
        bounds.top().into(),
        bounds.size.width.into(),
        bounds.size.height.into(),
    )
}
impl Adapter for FocusAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            "focusView" => return self.view.map(Reply::value),
            "viewContentY" => self.input.drawn?.frame.scroll.round().to_string(),
            "selectedFill" => colour(self.input.drawn?.fill),
            "selectionBandState" => {
                let band = self.input.band.as_ref();
                let mut fields = vec![
                    ("tracking".into(), Json::Bool(band.is_some())),
                    (
                        "active".into(),
                        Json::Bool(band.is_some_and(|band| band.active)),
                    ),
                ];
                if let Some(band) = band {
                    fields.extend([
                        ("columns".into(), number(self.input.frame?.columns as f32)),
                        ("contentY".into(), number(self.input.frame?.scroll)),
                        ("scrollRate".into(), number(crate::input::EDGE_RATE)),
                        ("anchor".into(), point(band.anchor)),
                        ("end".into(), point(band.end)),
                    ]);
                }
                flea::jsondoc::render(&Json::Obj(fields))
                    .trim_end()
                    .to_owned()
            }
            "scrollbarState" => {
                let drawn = self.input.drawn?;
                let frame = drawn.frame;
                let bar = crate::input::scroll::Bar::new(frame);
                flea::jsondoc::render(&Json::Obj(vec![
                    ("visible".into(), Json::Bool(drawn.visible)),
                    ("shown".into(), Json::Bool(drawn.shown)),
                    ("knob".into(), number(drawn.knob.size.width.into())),
                    ("rect".into(), Json::Str(rect(drawn.lane))),
                    ("knobRect".into(), Json::Str(rect(drawn.knob))),
                    ("handle".into(), number(bar.handle)),
                    ("offset".into(), number(bar.offset)),
                    ("content".into(), number(frame.content)),
                    ("viewport".into(), number(frame.viewport())),
                ]))
                .trim_end()
                .to_owned()
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
