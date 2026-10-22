//! The media surface: a sandboxed decoder helper behind one engine seam, its player and its strip.
pub mod engine;
pub mod glyphs;
pub mod helper;
pub mod ink;
#[cfg(feature = "bench-seam")]
pub mod kit;
pub mod mpris;
pub mod player;
pub mod sandbox;
pub mod sound;
pub mod strip;
pub mod surface;
#[cfg(test)]
mod tests;
pub mod view;

/// Runs the decoder helper when the process was started as one, returning its exit status.
pub fn dispatch(args: &[String]) -> Option<i32> {
    #[cfg(feature = "bench-seam")]
    if let Some(status) = kit::dispatch(args) {
        return Some(status);
    }
    helper::dispatch(args)
}
