//! Passive native GIF diagnostics. A paint attempt is distinct from a presented compositor frame.
use super::{Frame, SelectionPreview};
use flea::jsondoc::{self, Json};

pub(super) fn state(view: Option<&SelectionPreview>, frame: Option<&Frame>) -> String {
    let Some((view, frame)) = view.zip(frame).filter(|(_, frame)| {
        frame
            .subject
            .row
            .name
            .to_ascii_lowercase()
            .ends_with(".gif")
    }) else {
        return "{}".into();
    };
    let observed = view.frames.borrow();
    let painted = observed
        .animation
        .as_ref()
        .filter(|paint| paint.generation == frame.generation);
    let number = |value: u64| Json::Num(value.to_string());
    jsondoc::render(&Json::Obj(vec![
        ("path".into(), Json::Str(frame.subject.path())),
        ("generation".into(), number(frame.generation)),
        ("eligible".into(), Json::Bool(frame.animated_gif())),
        (
            "reportedFrames".into(),
            number(frame.facts.as_ref().map_or(0, |meta| meta.frames).into()),
        ),
        (
            "loadedFrames".into(),
            number(
                frame
                    .animation
                    .as_ref()
                    .map_or(0, |image| image.frame_count()) as u64,
            ),
        ),
        (
            "sourceRequested".into(),
            Json::Bool(frame.animation_source.is_some()),
        ),
        (
            "paintedIndex".into(),
            painted
                .filter(|paint| paint.error.is_none())
                .map_or(Json::Null, |paint| number(paint.index as u64)),
        ),
        (
            "paintedDraw".into(),
            painted
                .and_then(|paint| paint.draw)
                .map_or(Json::Null, number),
        ),
        (
            "paintError".into(),
            painted
                .and_then(|paint| paint.error.clone())
                .map_or(Json::Null, Json::Str),
        ),
    ]))
}
