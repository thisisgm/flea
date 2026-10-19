mod capabilities;
#[cfg(test)]
mod action_key_tests;
pub(crate) mod cwd;
mod external_editor;
mod parity;
mod tmux;
mod actions;
mod calendar;
mod detail;
mod batch;
#[cfg(test)]
mod chafa;
mod completion;
mod cells;
mod filter;
mod layout;
mod icons;
mod editor;
mod empty;
mod prompts;
mod trashview;
pub(crate) mod graphics;
mod input;
mod job;
mod jobstatus;
mod keymap;
mod media;
mod model;
mod mpv;
mod mounts;
mod pdf;
mod preview;
mod render;
mod taildrop;
mod terminal;
mod launch;
mod wait;
mod hold;
#[cfg(test)]
#[path = "hold_tests.rs"]
mod hold_tests;
// src/gui.rs reads the background hex from here, so the OEM key precedence has one parser.
pub(crate) mod theme;
mod warm;
mod wire;
#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;

use std::io;
use std::path::PathBuf;
extern "C" {
    fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char;
}

pub fn run(path: Option<&str>, select: Option<&str>, options: &cwd::Options) -> i32 {
    // Before any thread starts: a picture job's thread would otherwise open an arena that keeps a few hundred KB.
    crate::heap::one_arena();
    let result = (|| -> io::Result<i32> {
        // LC_CTYPE makes wcwidth use the terminal's inherited locale without changing numeric formatting.
        unsafe {
            setlocale(0, c"".as_ptr());
        }
        let path = path.map(PathBuf::from).unwrap_or(std::env::current_dir()?);
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        // The gvfs head start, before the state settle: gio runs beside settle, spawn and the UI build.
        let gvfs = warm::start(&path.to_string_lossy());
        let store = crate::uistore::Store::user().map_err(io::Error::other)?;
        store.settle().map_err(io::Error::other)?;
        let settings = store.read();
        let mut wire = wire::Wire::start(gvfs.as_ref())?;
        let mut model = model::Model::new(path.clone(), &settings);
        if let Some(preset) = &options.keymap { model.preset = preset.clone(); }
        model.choosing = options.chooser_file.is_some();
        model.store = Some(store);
        let mut size = terminal::size();
        model.height = render::body_height(size.1).max(1);
        model.columns = size.0;
        model.restore_path = select.map(PathBuf::from);
        // The listing goes out before the terminal is entered, so gio runs beside raw mode, keymap and theme.
        model.open(path, &mut wire)?;
        let mut terminal = terminal::Terminal::enter()?;
        // Rebound after the terminal so it drops first on every exit: mpv's relay writes before the screen is restored.
        let mut model = model;
        model.capabilities = terminal.decoder.negotiation.detected;
        let map = keymap::Map::load();
        model.hints = keymap::Hints::new(&map, &model.preset);
        let mut style = layout::Live::new(&mut model);
        let started = std::time::Instant::now();
        let mut frame = String::new();
        let mut graphics = graphics::Graphics::new();
        let mut thumbnail = PathBuf::new();
        let mut preview_generation = 0;
        let mut menu_tracking = false;
        // The listing, peek and preview hold below owns painted and start.
        let mut hold = hold::Hold::new();
        let mut last_input = std::time::Instant::now();
        let timeout = wait::input_timeout();
        while !model.quit && !terminal.stopped() {
            hold.sync(&model, std::time::Instant::now());
            while let Ok(event) = wire.events.try_recv() {
                match event {
                    Ok(value) => model.receive(value, &mut wire)?,
                    Err(e) => return Err(io::Error::other(e)),
                }
            }
            let mut launch_errors = Vec::new();
            model.launches.retain_mut(|(name, child)| match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() { launch_errors.push(format!("{} could not finish ({})", name, status)); }
                    false
                }
                Ok(None) => true,
                Err(error) => { launch_errors.push(format!("Could not observe {}: {}", name, crate::error::io_message(&error))); false }
            });
            for error in launch_errors { model.fail(error); }
            if let Some(path) = model.edit_file.take() {
                graphics.clear();
                model.player = None;
                model.pdf = None;
                match external_editor::run(&mut terminal, &path) {
                    Ok(()) => {}
                    Err(error) => {
                        if !terminal.ready() { return Err(error); }
                        model.fail(crate::error::io_message(&error));
                    }
                }
                model.refresh(&mut wire)?;
                frame.clear();
                thumbnail.clear();
                model.capabilities = terminal.decoder.negotiation.detected;
                graphics.protocol = model.capabilities.graphics;
            }
            if model.menu_action == "bulkEditor" {
                graphics.clear();
                model.player = None;
                model.pdf = None;
                let mut batch = model.bulk.take().unwrap();
                match batch.edit(&mut terminal) {
                    Ok(()) => {
                        model.bulk = Some(batch);
                        model.next_rename(&mut wire)?;
                    }
                    Err(error) => {
                        if !terminal.ready() { if terminal.stopped() { break; } return Err(error); }
                        model.fail(crate::error::io_message(&error));
                        model.menu_action.clear();
                        wire.send(vec![("c", wire::word("menuaction")), ("op", wire::word("close")), ("id", wire::number(model.action_id))])?;
                    }
                }
                frame.clear();
                thumbnail.clear();
                model.capabilities = terminal.decoder.negotiation.detected;
                graphics.protocol = model.capabilities.graphics;
            }
            if model.menu_action == "openWaiting" && model.pending.is_none() {
                model.menu_action = "open".into();
                wire.send(vec![("c", wire::word("menuaction")), ("op", wire::word("validate")), ("id", wire::number(model.action_id)), ("action", wire::word("open"))])?;
            }
            style.prepare(&mut model, &mut wire)?;
            detail::prepare(&mut model, &mut wire)?;
            model.taildrop.poll();
            model.advance_taildrop(&mut wire)?;
            if let Some(result) = model.taildrop.sent.take() {
                match result {
                    Ok(message) => model.say(message),
                    Err(error) => model.fail(error),
                }
            }
            completion::land(&mut model);
            model.expire_message(std::time::Instant::now());
            let visible = (model.preview_visible || model.quicklook) && model.selected.len() < 2;
            let overlay = model.menu || model.sheet || model.editor.is_some() || model.deletion.is_some();
            if (model.menu || model.deletion.is_some()) != menu_tracking {
                menu_tracking = model.menu || model.deletion.is_some();
                print!("{}", if menu_tracking { "\x1b[?1003h" } else { "\x1b[?1003l\x1b[?1002h" });
                std::io::Write::flush(&mut std::io::stdout())?;
            }
            let current = model.current_path().filter(|_| prompts::previewed(&model));
            let preview_allowed =
                model.preview_loaded && current.as_ref() == Some(&model.preview_path);
            const PREVIEW_RESERVED_ROWS: usize = 5;
            let is_pdf = model.rows.get(&model.cursor).is_some_and(model::Row::is_pdf);
            let reserved = PREVIEW_RESERVED_ROWS + model.preview_metadata.len().min(3)
                + usize::from(is_pdf);
            let (preview_start, preview_width) = render::preview_area(size.0, model.quicklook);
            let geometry = (preview_width, model.height.saturating_sub(reserved), preview_start + 1, render::BODY_ROW + 2);
            let cell_pixels = terminal::cell_pixels();
            let pixels = cell_pixels
                .map(|cell| (geometry.0 * cell.0, geometry.1 * cell.1))
                .filter(|pixels| pixels.0 > 0 && pixels.1 > 0);
            let graphics_ready = pixels.is_some() || graphics.protocol == graphics::Protocol::None;
            let pixel_extent = pixels.unwrap_or((0, 0));
            let kind = model
                .rows
                .get(&model.cursor)
                .map(|r| r.icon.to_lowercase())
                .unwrap_or_default();
            let is_media = kind.contains("audio") || kind.contains("video");
            if !visible
                || !preview_allowed
                || !is_media
                || overlay
                || model.player.as_ref().is_some_and(|p| {
                    Some(&p.path) != current.as_ref()
                        || p.geometry != geometry
                        || p.pixels != pixel_extent
                })
            {
                if let Some(player) = model.player.take() {
                    if !player.at_rest() {
                        model.playback = Some((player.path.clone(), player.position, player.paused));
                    }
                }
            }
            if !visible
                || !preview_allowed
                || !graphics_ready
                || !is_pdf
                || model
                    .pdf
                    .as_ref()
                    .is_some_and(|p| Some(&p.path) != current.as_ref())
            {
                model.pdf = None;
            }
            if visible && preview_allowed {
                if let Some(path) = current.clone() {
                    if is_media
                        && !overlay
                        && model.player.is_none()
                        && graphics_ready
                        && model.preview_failed.as_ref() != Some(&path)
                    {
                        let resume = media::resume_for(model.playback.as_ref(), &path);
                        // A plain terminal shows nothing until play; in kitty with shared memory a video's poster stands in for mpv's paused frame.
                        let rests = resume.is_none()
                            && (graphics.protocol == graphics::Protocol::None
                                || (graphics.protocol == graphics::Protocol::Kitty
                                    && model.capabilities.shared_memory
                                    && kind.contains("video")
                                    && std::path::Path::new(graphics::POSTER).exists()));
                        if rests {
                            model.player = Some(media::Player::resting(&path, graphics.protocol, model.capabilities.shared_memory, geometry, size, pixel_extent));
                        } else {
                            match media::Player::start(
                                &path,
                                graphics.protocol,
                                model.capabilities.shared_memory,
                                geometry,
                                size,
                                pixel_extent,
                            ) {
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
                        model.pdf = Some(pdf::Pdf::new(
                            path.clone(),
                            graphics.protocol,
                            geometry.0,
                            geometry.1,
                            pixel_extent,
                        ));
                    }
                    if path != thumbnail || preview_generation != model.preview_generation {
                        if let Some(index) = model.thumb_index {
                            wire.send(vec![
                                ("c", wire::word("thumbcancel")),
                                ("rows", crate::jsondoc::Json::Arr(vec![wire::number(index)])),
                            ])?;
                        }
                        graphics.clear();
                        model.image_file = None;
                        model.thumb_index = None;
                        thumbnail = path;
                        preview_generation = model.preview_generation;
                        if !is_media
                            && !is_pdf
                            && model.rows.get(&model.cursor).is_some_and(|r| r.thumbnail)
                        {
                            if kind.starts_with("image") {
                                model.image_file = Some(thumbnail.clone());
                            } else {
                                model.thumb_index = Some(model.cursor);
                                wire.send(vec![
                                    ("c", wire::word("thumb")),
                                    (
                                        "rows",
                                        crate::jsondoc::Json::Arr(vec![wire::number(model.cursor)]),
                                    ),
                                ])?;
                            }
                        }
                    }
                }
            } else {
                if let Some(index) = model.thumb_index.take() {
                    wire.send(vec![
                        ("c", wire::word("thumbcancel")),
                        ("rows", crate::jsondoc::Json::Arr(vec![wire::number(index)])),
                    ])?;
                }
                graphics.clear();
                model.image_file = None;
                thumbnail = PathBuf::new();
            }
            if let (Some(path), Some(pixels)) = (&model.image_file, pixels) {
                graphics.request(path.clone(), geometry.0, geometry.1, pixels);
            }
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
            if visible && graphics.protocol != graphics::Protocol::None && pixels.is_none() && (is_media || is_pdf || model.image_file.is_some()) && model.preview_failed != current {
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
                if (pdf.columns, pdf.rows) != (geometry.0, geometry.1) || pdf.pixels != pixel_extent
                {
                    pdf.columns = geometry.0;
                    pdf.rows = geometry.1;
                    pdf.pixels = pixel_extent;
                    pdf.refresh();
                }
                if pdf.poll() {
                    frame.clear();
                }
            }
            if let Some(pdf) = &mut model.pdf {
                let error = std::mem::take(&mut pdf.error);
                if !error.is_empty() { model.fail(error); }
            }
            // Keep the previous frame until rows, peek and the first screen's preview body land, at most the slow-source cap.
            let now = std::time::Instant::now();
            let held = hold.hold(&model, now, started);
            model.held = held;
            if !held {
                let changed = render::draw(
                    &model,
                    &style.theme,
                    &map,
                    size.0,
                    size.1,
                    cell_pixels,
                    started.elapsed(),
                    &mut frame,
                )?;
                let writing = changed && !overlay;
                let transmit = if writing && model.pdf.is_none() { std::mem::take(&mut graphics.transmit) } else { Vec::new() };
                let image = model
                    .pdf
                    .as_ref()
                    .map(|p| p.bytes.as_slice())
                    .unwrap_or(&graphics.bytes);
                // Lowercase d=i drops the placements and keeps kitty's stored copy, so a redraw never re-sends the picture.
                if changed && graphics.protocol == graphics::Protocol::Kitty {
                    tmux::write_graphics(&mut std::io::stdout().lock(), b"\x1b_Ga=d,d=i,i=42,q=2\x1b\\")?;
                    std::io::Write::flush(&mut std::io::stdout())?;
                }
                if writing && !image.is_empty() {
                    use std::io::Write;
                    let mut stdout = std::io::stdout().lock();
                    write!(stdout, "\x1b[s\x1b[{};{}H", geometry.3, geometry.2)?;
                    graphics::send(&mut stdout, transmit)?;
                    tmux::write_graphics(&mut stdout, image)?;
                    write!(stdout, "\x1b[u")?;
                    stdout.flush()?;
                }
                hold.mark_painted();
            }
            // A held frame's deadline may cut the input wait short, never lengthen it.
            let input_wait = wait::poll_wait_ms(terminal.decoder.waiting(), last_input.elapsed(), timeout);
            let input_wait = terminal.decoder.negotiation.wait_ms(std::time::Instant::now(), input_wait);
            let bytes = terminal.read(wire.wake_fd(), hold.wait_ms(&model, std::time::Instant::now(), started, input_wait))?;
            if terminal.take_resumed() { frame.clear(); }
            wire.drain_wake();
            if !bytes.is_empty() {
                last_input = std::time::Instant::now();
            }
            for key in terminal.decode(&bytes, wait::escape_due(bytes.is_empty(), last_input.elapsed(), timeout))? {
                let key = if model.editor.is_some() { std::borrow::Cow::Borrowed(&key) } else { key.for_action() };
                if let Err(e) = actions::key(&mut model, &key, &map, &mut wire) {
                    model.fail(crate::error::io_message(&e));
                }
            }
            model.capabilities = terminal.decoder.negotiation.detected;
            let protocol = model.capabilities.graphics;
            if protocol != graphics.protocol {
                graphics.clear();
                model.player = None;
                model.pdf = None;
                graphics.protocol = protocol;
                thumbnail = PathBuf::new();
            }
            let next = terminal::size();
            if next != size {
                size = next;
                model.height = render::body_height(size.1).max(1);
                model.columns = size.0;
                model.window(&mut wire)?;
            }
        }
        let clean = model.quit && !terminal.stopped();
        let directory = model.last_directory.clone();
        let chosen = model.chosen.take();
        let no_cwd = model.no_cwd;
        drop(graphics);
        drop(model);
        drop(terminal);
        options.finish(directory.as_deref(), chosen.as_deref(), clean, no_cwd)
    })();
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("flea: terminal interface: {}", crate::error::io_message(&e));
            2
        }
    }
}
