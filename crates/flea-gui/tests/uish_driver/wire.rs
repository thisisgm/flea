//! Control-line fields and the driver's fixed limits, shared by the replay loop and the window host.
use flea::jsondoc::Json;
use std::time::Duration;

// The largest control line the replay reads, in bytes.
pub(super) const LIMIT: u64 = 65_536;
// A replay with no control line for this long has lost its controller.
pub(super) const IDLE_LIMIT: Duration = Duration::from_secs(30);
// A whole replay never runs longer than this.
pub(super) const REPLAY_LIMIT: Duration = Duration::from_secs(300);
// One GPUI test clock step, also the replay loop's poll interval.
pub(super) const STEP: Duration = Duration::from_millis(2);
// An owned window must open its socket or close within this.
pub(super) const WAIT: Duration = Duration::from_secs(10);
// A pointer coordinate beyond this many pixels is a malformed control, not a position.
const COORDINATE_BOUND: f64 = 100_000.;

pub(super) fn object(fields: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub(super) fn text(value: impl ToString) -> Json {
    Json::Str(value.to_string())
}
pub(super) fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}
pub(super) fn word<'a>(value: &'a Json, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("missing string {key}"))
}
pub(super) fn coordinate(value: &Json, key: &str) -> Result<f32, String> {
    value
        .get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite() && n.abs() <= COORDINATE_BOUND)
        .map(|n| n as f32)
        .ok_or_else(|| format!("invalid coordinate {key}"))
}

// The capabilities reply around one raster capture; only the raster is probed, the rest is fixed.
pub(super) fn capabilities(raster: Json) -> Json {
    object([
        ("raster", raster),
        ("window_events", text("available")),
        ("production_ipc", text("available")),
        ("native_input", text("unsupported")),
        ("hyprland_client", text("unsupported")),
        ("wayland_dnd", text("unsupported")),
        ("media_frames", text("unsupported")),
        ("presented_frame_timing", text("unsupported")),
        (
            "executor_clock",
            text(format!(
                "GPUI test clock; advanced in {} ms steps",
                STEP.as_millis()
            )),
        ),
    ])
}
