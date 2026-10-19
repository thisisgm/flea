use super::graphics::{self, Graphics, Protocol};
use super::model::Model;
use super::pane_state::{self, Kind};
use super::still::{Key, Source};
use super::wire::{self, Wire};
use super::{media, pdf, picture};
use std::io;
use std::path::{Path, PathBuf};

// The picture the pane last asked the backend or a decoder for, so one file is asked for once per preview.
#[derive(Default)]
pub struct Shown {
    path: PathBuf,
    generation: usize,
}
impl Shown {
    pub fn forget(&mut self) {
        self.path = PathBuf::new();
    }
}

// One input-loop pass over the preview's decoders and player; answers where a terminal picture is placed, one-based.
pub fn step(model: &mut Model, graphics: &mut Graphics, shown: &mut Shown, wire: &mut Wire, size: (usize, usize), overlay: bool, frame: &mut String) -> io::Result<(usize, usize)> {
    let visible = (model.preview_visible || model.quicklook) && model.selected.len() < 2;
    let current = model.current_path().filter(|_| super::prompts::previewed(model));
    let content = pane_state::content(model);
    let preview_allowed = model.preview_loaded && current.as_ref() == Some(&model.preview_path);
    let row = model.rows.get(&model.cursor).cloned();
    let kind = row.as_ref().map(pane_state::kind);
    let is_pdf = kind == Some(Kind::Pdf);
    let is_media = matches!(kind, Some(Kind::Audio | Kind::Video));
    let place = picture::place(model);
    let geometry = (place.cells.0, place.cells.1, place.left, place.top);
    let cell = model.preview_state.cell;
    let pixels = cell.map(|cell| (geometry.0 * cell.0, geometry.1 * cell.1)).filter(|pixels| pixels.0 > 0 && pixels.1 > 0);
    let page_pixels = cell.map(|cell| (place.room.0 * cell.0, place.room.1 * cell.1)).filter(|pixels| pixels.0 > 0 && pixels.1 > 0);
    let graphics_ready = pixels.is_some() || graphics.protocol == Protocol::None;
    let pixel_extent = pixels.unwrap_or((0, 0));
    if !visible || !preview_allowed || !is_media || overlay
        || model.player.as_ref().is_some_and(|p| Some(&p.path) != content.as_ref() || p.geometry != geometry || p.pixels != pixel_extent)
    {
        if let Some(player) = model.player.take() {
            if !player.at_rest() {
                model.playback = Some((player.path.clone(), player.position, player.paused));
            }
        }
    }
    if !visible || !preview_allowed || !graphics_ready || !is_pdf || model.pdf.as_ref().is_some_and(|p| Some(&p.path) != content.as_ref()) {
        model.pdf = None;
    }
    if let (true, Some(path)) = (visible && preview_allowed, content.clone()) {
        // The probe fixes the picture's box, so the player starts once, in the box it keeps.
        if is_media && !overlay && model.player.is_none() && graphics_ready && model.media_probe.is_some() && model.preview_failed.as_ref() != Some(&path) {
            let resume = media::resume_for(model.playback.as_ref(), &path);
            // A plain terminal shows nothing until play; in kitty with shared memory a video's poster stands in for mpv's paused frame.
            let rests = resume.is_none()
                && (graphics.protocol == Protocol::None
                    || (graphics.protocol == Protocol::Kitty && model.capabilities.shared_memory && kind == Some(Kind::Video) && Path::new(graphics::POSTER).exists()));
            if rests {
                model.player = Some(media::Player::resting(&path, graphics.protocol, model.capabilities.shared_memory, geometry, size, pixel_extent));
            } else {
                match media::Player::start(&path, graphics.protocol, model.capabilities.shared_memory, geometry, size, pixel_extent) {
                    Ok(mut player) => {
                        player.resume = resume;
                        model.player = Some(player);
                    }
                    Err(e) => {
                        model.fail(format!("Media preview: {}", crate::error::io_message(&e)));
                        model.preview_failed = Some(path.clone());
                    }
                }
            }
        }
        if is_pdf && model.pdf.is_none() && graphics_ready {
            let still = model.preview_state.colors.is_some() && super::chafa::installed();
            model.pdf = Some(pdf::Pdf::new(path.clone(), graphics.protocol, place.room.0, place.room.1, page_pixels.unwrap_or((0, 0)), &model.preview_state.ground, still));
        }
        if path != shown.path || shown.generation != model.preview_generation {
            if let Some(index) = model.thumb_index {
                wire.send(vec![("c", wire::word("thumbcancel")), ("rows", crate::jsondoc::Json::Arr(vec![wire::number(index)]))])?;
            }
            graphics.clear();
            model.image_file = None;
            model.thumb_index = None;
            shown.path = path;
            shown.generation = model.preview_generation;
            if let Some(row) = row.as_ref().filter(|_| !is_media && !is_pdf) {
                if pane_state::pictured(row) {
                    model.image_file = Some(shown.path.clone());
                } else if row.thumbnail && kind != Some(Kind::Image) {
                    model.thumb_index = Some(model.cursor);
                    wire.send(vec![("c", wire::word("thumb")), ("rows", crate::jsondoc::Json::Arr(vec![wire::number(model.cursor)]))])?;
                }
            }
        }
    } else {
        if let Some(index) = model.thumb_index.take() {
            wire.send(vec![("c", wire::word("thumbcancel")), ("rows", crate::jsondoc::Json::Arr(vec![wire::number(index)]))])?;
        }
        graphics.clear();
        model.image_file = None;
        shown.forget();
    }
    if let (Some(path), Some(pixels), Some(_)) = (&model.image_file, pixels, model.media_probe) {
        graphics.request(path.clone(), geometry.0, geometry.1, pixels);
    }
    still(model, graphics.protocol, visible && preview_allowed, place.room);
    if let Some(player) = &mut model.player {
        if let Some((w, h, milliseconds)) = model.media_probe {
            player.probe(milliseconds);
            if let Some(pixels) = pixels.filter(|_| player.at_rest()) {
                graphics.request_poster(player.path.clone(), geometry.0, geometry.1, pixels, graphics::poster_side((w, h), pixels));
            }
        }
        // The poster stays the TUI's own image until mpv's first frame replaces it, or for good if none comes.
        if !player.at_rest() && !player.handing_off() && graphics.holds(&player.path) {
            graphics.release();
        }
        player.poster_up = player.at_rest() && graphics.shows(&player.path);
    }
    if visible && graphics.protocol != Protocol::None && pixels.is_none() && (is_media || is_pdf || model.image_file.is_some()) && model.preview_failed != current {
        model.fail("Inline preview unavailable: terminal did not report pixel dimensions".into());
        model.preview_failed = current.clone();
    }
    if graphics.accept() {
        frame.clear();
    }
    if !graphics.error.is_empty() {
        model.fail(format!("Image preview: {}", std::mem::take(&mut graphics.error)));
    }
    if let Some(player) = &mut model.player {
        player.poll();
        if !player.error.is_empty() {
            model.preview_failed = Some(player.path.clone());
        }
    }
    if model.player.as_ref().is_some_and(|p| !p.error.is_empty()) {
        let error = model.player.take().unwrap().error.clone();
        model.fail(error);
    }
    if let Some(pdf) = &mut model.pdf {
        let pixels = page_pixels.unwrap_or((0, 0));
        if (pdf.columns, pdf.rows) != place.room || pdf.pixels != pixels {
            (pdf.columns, pdf.rows, pdf.pixels) = (place.room.0, place.room.1, pixels);
            pdf.refresh();
        }
        if pdf.poll() {
            frame.clear();
        }
        let error = std::mem::take(&mut pdf.error);
        if !error.is_empty() {
            model.fail(error);
        }
    }
    Ok((geometry.2, geometry.3))
}

// The half-block picture in a terminal without graphics: an image, a video's first frame or a page, through chafa.
fn still(model: &mut Model, protocol: Protocol, shown: bool, room: (usize, usize)) {
    let (path, generation, probe) = (pane_state::content(model), model.preview_generation, model.media_probe);
    let (row, pdf, state) = (model.rows.get(&model.cursor), &model.pdf, &mut model.preview_state);
    let (Some(colors), Some(path), Some(row), true) = (state.colors, path, row, shown && protocol == Protocol::None) else {
        state.still.clear();
        return;
    };
    let key = |page| Key { path: path.clone(), generation, page, cells: room, colors };
    match pane_state::kind(row) {
        Kind::Image if pane_state::pictured(row) => state.still.want(key((0, 0, 0)), || Some(Source::File(path.clone()))),
        Kind::Video if Path::new(graphics::POSTER).exists() && probe.is_some() => {
            let side = graphics::poster_side(probe.map_or((0, 0), |(w, h, _)| (w, h)), (room.0 * 8, room.1 * 16));
            state.still.want(key((0, 0, 0)), || Some(Source::Poster(path.clone(), side)));
        }
        Kind::Pdf => match pdf.as_ref().filter(|pdf| !pdf.png.is_empty()) {
            Some(pdf) => state.still.want(key((pdf.revision, 0, 0)), || Some(Source::Bytes(pdf.png.clone()))),
            None => state.still.clear(),
        },
        _ => state.still.clear(),
    }
    state.still.poll();
    let error = std::mem::take(&mut state.still.error);
    if !error.is_empty() {
        model.fail(format!("Image preview: {error}"));
    }
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
