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
mod chafa;
mod completion;
mod contents;
#[cfg(test)]
mod contents_tests;
#[cfg(test)]
mod contents_process_tests;
#[cfg(test)]
mod contents_state_tests;
#[cfg(test)]
mod contents_fix_tests;
#[cfg(test)]
mod contents_fix2_tests;
pub(crate) mod cells;
mod filter;
mod finder;
mod parity17;
mod layout;
mod look;
mod pane;
mod pane_state;
mod picture;
mod stage;
mod still;
mod syntax;
mod icons;
mod editor;
mod editor_words;
mod editor_input;
mod sort_actions;
mod empty;
mod prompts;
mod combine;
mod convert;
#[cfg(test)]
mod combine_tests;
#[cfg(test)]
mod combine_process_tests;
mod print;
#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod convert_process_tests;
#[cfg(test)]
mod convert_real_tests;
#[cfg(test)]
mod convert_lifecycle_tests;
#[cfg(test)]
mod convert_review_tests;
#[cfg(test)]
mod print_tests;
#[cfg(test)]
mod print_fix_tests;
#[cfg(test)]
mod print_guard_tests;
#[cfg(test)]
mod print_ui_tests;
#[cfg(test)]
mod print_process_tests;
mod trashview;
pub(crate) mod graphics;
mod input;
mod job;
mod jobstatus;
mod keycaps;
mod keymap;
mod media;
mod model;
mod footer_errors;
mod office;
mod model3d;
mod mpv;
mod mounts;
mod pdf;
mod preview;
mod preview_keys;
mod render;
mod taildrop;
mod terminal;
mod launch;
mod wait;
mod words;
#[cfg(test)]
mod live_language_tests;
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

// The frame's picture bytes, and whether the held thumbnail transmission goes out with this frame.
// Sample state: a page whose bytes are still empty keeps the thumbnail bytes and its transmission.
pub(crate) fn frame_image<'a>(pdf: Option<&'a pdf::Pdf>, thumb: &'a [u8], writing: bool) -> (&'a [u8], bool) {
    let page = pdf.map(|p| p.bytes.as_slice()).filter(|bytes| !bytes.is_empty());
    (page.unwrap_or(thumb), writing && page.is_none())
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
        // A start folder spelled with .. or a trailing slash opens under its own name, as Go to does.
        let path = completion::lexical(&path);
        // The gvfs head start, before the state settle: gio runs beside settle, spawn and the UI build.
        let gvfs = warm::start(&path.to_string_lossy());
        let store = crate::uistore::Store::user().map_err(io::Error::other)?;
        store.settle().map_err(io::Error::other)?;
        let settings = store.read();
        let mut wire = wire::Wire::start(gvfs.as_ref())?;
        // The keyboard group readback runs beside the listing and terminal setup.
        let mut keycaps = keycaps::Keycaps::start();
        let mut model = model::Model::new(path.clone(), &settings);
        if let Some(preset) = &options.keymap { model.preset = preset.clone(); }
        model.choosing = options.chooser_file.is_some();
        model.store = Some(store);
        let mut size = terminal::size();
        model.resize_body(render::body_height(size.1));
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
        keycaps.settle(&mut model, &map, keycaps::FIRST_WAIT);
        let mut style = layout::Live::new(&mut model);
        let started = std::time::Instant::now();
        let mut frame = String::new();
        let mut graphics = graphics::Graphics::new();
        let mut shown = stage::Shown::default();
        let mut menu_tracking = false;
        // The listing, peek and preview hold below owns painted and start.
        let mut hold = hold::Hold::new();
        let mut last_input = std::time::Instant::now();
        let timeout = wait::input_timeout();
        preview::refresh_locale(&mut model);
        render::cards::refresh_locale(&mut model);
        while !model.quit && !terminal.stopped() {
            if model.refresh_frame_locale() {
                model.window(&mut wire)?;
            }
            keycaps.apply(&mut model, &map);
            hold.sync(&model, std::time::Instant::now());
            while let Ok(event) = wire.events.try_recv() {
                match event {
                    Ok(value) => model.receive(value, &mut wire)?,
                    Err(e) => return Err(io::Error::other(e)),
                }
            }
            model.poll_launches();
            if let Some(path) = model.edit_file.take() {
                graphics.clear();
                model.player = None;
                model.pdf = None;
                match external_editor::run(&mut terminal, &path) {
                    Ok(()) => {}
                    Err(error) => {
                        if !terminal.ready() { return Err(error); }
                        model.fail_io(&error);
                    }
                }
                model.refresh(&mut wire)?;
                frame.clear();
                shown.forget();
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
                        model.fail_io(&error);
                        model.menu_action.clear();
                        wire.send(vec![("c", wire::word("menuaction")), ("op", wire::word("close")), ("id", wire::number(model.action_id))])?;
                    }
                }
                frame.clear();
                shown.forget();
                model.capabilities = terminal.decoder.negotiation.detected;
                graphics.protocol = model.capabilities.graphics;
            }
            if model.menu_action == "openWaiting" && model.pending.is_none() {
                model.menu_action = "open".into();
                wire.send(vec![("c", wire::word("menuaction")), ("op", wire::word("validate")), ("id", wire::number(model.action_id)), ("action", wire::word("open"))])?;
            }
            model.resize_body(render::body_height(size.1));
            style.prepare(&mut model, &mut wire)?;
            model.model3d_palette = graphics::palette(&style.theme);
            detail::prepare(&mut model, &mut wire)?;
            convert::poll(&mut model);
            combine::poll(&mut model);
            model.taildrop.poll();
            model.advance_taildrop(&mut wire)?;
            if let Some(result) = model.taildrop.take_sent() {
                match result {
                    Ok(message) => model.say_text(message),
                    Err(error) => model.fail_text(error),
                }
            }
            completion::land(&mut model);
            model.expire_message(std::time::Instant::now());
            let overlay = model.menu || model.sheet || model.editor.is_some() || model.deletion.is_some() || model.prompts.question.is_some() || (model.printing.card.is_some() || model.converting.card.is_some() || model.combining.card.is_some());
            if (model.menu || model.deletion.is_some() || (model.printing.card.is_some() || model.converting.card.is_some() || model.combining.card.is_some())) != menu_tracking {
                menu_tracking = model.menu || model.deletion.is_some() || (model.printing.card.is_some() || model.converting.card.is_some() || model.combining.card.is_some());
                print!("{}", if menu_tracking { "\x1b[?1003h" } else { "\x1b[?1003l\x1b[?1002h" });
                std::io::Write::flush(&mut std::io::stdout())?;
            }
            let cell_pixels = terminal::cell_pixels();
            model.preview_state.cell = cell_pixels;
            let place = stage::step(&mut model, &mut graphics, &mut shown, &mut wire, size, overlay, &mut frame)?;
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
                let writing = changed && (!overlay || model.combining.card.is_some());
                let (image, take) = frame_image(model.pdf.as_ref(), &graphics.bytes, writing);
                let transmit = if take { std::mem::take(&mut graphics.transmit) } else { Vec::new() };
                // Lowercase d=i drops the placements and keeps kitty's stored copy, so a redraw never re-sends the picture.
                if changed && graphics.protocol == graphics::Protocol::Kitty {
                    tmux::write_graphics(&mut std::io::stdout().lock(), b"\x1b_Ga=d,d=i,i=42,q=2\x1b\\")?;
                    std::io::Write::flush(&mut std::io::stdout())?;
                }
                if writing && !image.is_empty() {
                    use std::io::Write;
                    let mut stdout = std::io::stdout().lock();
                    write!(stdout, "\x1b[s\x1b[{};{}H", place.1, place.0)?;
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
            let now = std::time::Instant::now();
            let input_wait = hold.wait_ms(&model, now, started, input_wait);
            if std::mem::take(&mut model.parity17.suspend) { terminal.request_suspend(); }
            let bytes = terminal.read(wire.wake_fd(), wait::next_wait_ms(&model, size, now, input_wait))?;
            if terminal.take_resumed() {
                frame.clear();
                keycaps.resume();
            }
            wire.drain_wake();
            if !bytes.is_empty() {
                last_input = std::time::Instant::now();
            }
            for key in terminal.decode(&bytes, wait::escape_due(bytes.is_empty(), last_input.elapsed(), timeout))? {
                let key = if model.editor.is_some() { std::borrow::Cow::Borrowed(&key) } else { key.for_action() };
                if let Err(e) = actions::key(&mut model, &key, &map, &mut wire) {
                    model.fail_io(&e);
                }
            }
            model.capabilities = terminal.decoder.negotiation.detected;
            let protocol = model.capabilities.graphics;
            if protocol != graphics.protocol {
                graphics.clear();
                model.player = None;
                model.pdf = None;
                graphics.protocol = protocol;
                shown.forget();
            }
            let next = terminal::size();
            if next != size {
                size = next;
                model.resize_body(render::body_height(size.1));
                model.columns = size.0;
                model.window(&mut wire)?;
            }
        }
        let clean = model.quit && !terminal.stopped();
        let directory = model.last_directory.clone();
        let chosen = model.chosen.take();
        let no_cwd = model.no_cwd;
        model.model3d.stop(&mut wire)?;
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

#[cfg(test)]
mod parity17_tests;

#[cfg(test)]
mod cells_hig_tests;

#[cfg(test)]
mod hig_s5_tests;

#[cfg(test)]
mod newfolder_tests;
