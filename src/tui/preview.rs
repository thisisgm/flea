use super::model::Model;
use crate::backend::regfile;
use crate::oflags::O_NOFOLLOW;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver}, Arc};

pub mod words;
use words::Text;

const TEXT_BYTES: u64 = 1024 * 1024;
#[derive(Default)]
pub struct Reader {
    pending: Option<(PathBuf, usize)>,
    running: Option<(usize, Arc<AtomicBool>, Receiver<Result<String, &'static str>>)>,
}
impl Reader {
    fn request(&mut self, path: PathBuf, generation: usize) {
        self.cancel();
        self.pending = Some((path, generation));
        self.start();
    }
    // A read for this generation is still out: queued, or running and not cancelled.
    pub fn outstanding(&self, generation: usize) -> bool {
        self.pending.as_ref().is_some_and(|(_, queued)| *queued == generation)
            || self.running.as_ref().is_some_and(|(running, cancel, _)| *running == generation && !cancel.load(Ordering::Relaxed))
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        if let Some((_, cancel, _)) = &self.running { cancel.store(true, Ordering::Relaxed); }
    }
    fn start(&mut self) {
        if self.running.is_some() { return; }
        let Some((path, generation)) = self.pending.take() else { return };
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(read_text(&path, &stopped)); });
        self.running = Some((generation, cancel, receiver));
    }
    fn poll(&mut self) -> Option<(usize, Result<String, &'static str>)> {
        let (generation, cancel, receiver) = self.running.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("tui.preview.text_reader_stopped"),
        };
        let generation = *generation;
        let accepted = !cancel.load(Ordering::Relaxed);
        self.running = None;
        self.start();
        accepted.then_some((generation, result))
    }
}
impl Drop for Reader {
    fn drop(&mut self) { self.cancel(); }
}
fn read_text(path: &Path, cancel: &AtomicBool) -> Result<String, &'static str> {
    const READ_FAILED: &str = "preview.unreadable";
    let before = path.symlink_metadata().map_err(|_| READ_FAILED)?;
    let mut file = regfile::open_if_regular(path, O_NOFOLLOW).map_err(|_| READ_FAILED)?;
    if !file.metadata().is_ok_and(|after| before.dev() == after.dev() && before.ino() == after.ino()) {
        return Err("tui.preview.changed");
    }
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        if cancel.load(Ordering::Relaxed) { return Err("tui.preview.cancelled"); }
        let count = file.read(&mut chunk).map_err(|_| READ_FAILED)?;
        if count == 0 { break; }
        // Data with a NUL in its first chunk is binary, so the rest of it is never read.
        if bytes.is_empty() && chunk[..count].contains(&0) { return Ok(String::new()); }
        if bytes.len() as u64 + count as u64 > TEXT_BYTES { return Err("preview.tooLarge"); }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.contains(&0) { return Ok(String::new()); }
    let mut body = String::new();
    for c in String::from_utf8_lossy(&bytes).chars() {
        if c == '\t' { body.push_str("        "); }
        else if c == '\n' || super::render::safe(c) { body.push(c); }
    }
    Ok(body)
}
pub fn load(m: &mut Model, force: bool) {
    if m.selected.len() > 1 || (!m.preview_visible && !m.quicklook) {
        m.preview_reader.cancel();
        m.preview_path.clear();
        m.retract_preview_errors(true);
        return;
    }
    let Some(path) = m.current_path().or_else(|| super::trashview::content(m)) else {
        return;
    };
    // A Markdown document Quick Look read rests on its head again once Quick Look closes.
    let read_for_quicklook = !m.quicklook && markdown(&path) && (!m.preview_body.is_empty() || m.preview_reader.outstanding(m.preview_generation));
    if !force && path == m.preview_path && !read_for_quicklook {
        return;
    }
    m.preview_reader.cancel();
    m.retract_preview_errors(path != m.preview_path);
    m.preview_path = path.clone();
    m.preview_loaded = force || m.preview_auto || m.quicklook;
    m.preview_generation = m.preview_generation.wrapping_add(1);
    m.preview_failed = None;
    m.preview_scroll = 0;
    m.preview.clear();
    m.preview_body.clear();
    m.preview_metadata.clear();
    m.media_probe = None;
    m.preview_children = None;
    super::pane_state::reset(&mut m.preview_state);
    words::metadata(m);
    let Some(row) = m.rows.get(&m.cursor) else {
        return;
    };
    m.preview.push(row.name.clone());
    m.preview
        .push(format!("{} · {}", row.kind, m.preview_state.i18n.locale().format_size(row.size as u64)));
    if !m.preview_loaded {
        if !m.hints.load_preview.is_empty() {
            set_text(m, Text::Args("preview.loadHint", vec![Text::Data(super::cells::clean(&m.hints.load_preview))]));
        }
        return;
    }
    if row.directory {
        set_text(m, Text::Key("preview.points.folder"));
        return;
    }
    // Text reads as text, and so does data that holds no NUL; a link reads its target, which the reader opens without following.
    let text = row.icon.starts_with("text") || row.kind.to_lowercase().contains("source");
    if super::pane_state::kind(row) == super::pane_state::Kind::Model3d || !text && super::pane_state::kind(row) != super::pane_state::Kind::Data {
        return;
    }
    // A Markdown document rests on its head in the pane, as v0.3.12 and the TUI boards draw todo.md; Quick Look still reads it.
    if !m.quicklook && markdown(std::path::Path::new(&row.name)) {
        return;
    }
    if row.size as u64 > TEXT_BYTES && row.link.is_empty() {
        if text { set_text(m, Text::Key("preview.tooLarge")); }
        return;
    }
    set_text(m, Text::Key("preview.loading"));
    let content = super::pane_state::content(m).unwrap_or(path);
    m.preview_reader.request(content, m.preview_generation);
}
fn markdown(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}
pub fn poll(m: &mut Model) {
    let Some((generation, result)) = m.preview_reader.poll() else { return };
    if generation != m.preview_generation { return; }
    m.preview.truncate(2);
    match result {
        Ok(body) => { m.preview_state.text = None; m.preview.push(String::new()); m.preview_body = body; }
        Err(error) => set_text(m, Text::Key(error)),
    }
    m.preview_width = 0;
}
pub fn request(m: &mut Model, wire: &mut super::wire::Wire) -> std::io::Result<()> {
    meta(m, wire)?;
    super::syntax::request(m, wire)
}
fn meta(m: &mut Model, wire: &mut super::wire::Wire) -> std::io::Result<()> {
    use super::wire::{number, word};
    use crate::jsondoc::Json;
    if !m.preview_loaded
        || m.preview_requested == m.preview_generation
        || m.selected.len() > 1
        || (!m.preview_visible && !m.quicklook)
    {
        return Ok(());
    }
    let Some(row) = m.rows.get(&m.cursor) else {
        return Ok(());
    };
    if m.current_path().as_ref() != Some(&m.preview_path) {
        return Ok(());
    }
    m.preview_requested = m.preview_generation;
    if super::pane_state::kind(row) == super::pane_state::Kind::Model3d { return Ok(()); }
    if row.directory {
        m.preview_children = Some(m.preview_path.clone());
        wire.send(vec![
            ("c", word("peek")),
            ("details", Json::Bool(true)),
            ("path", word(&m.preview_path.to_string_lossy())),
            ("first", number(m.height)),
            ("hidden", Json::Bool(m.hidden)),
        ])?;
    } else {
        wire.send(vec![
            ("c", word("meta")),
            ("row", number(m.cursor)),
            ("token", number(m.preview_requested)),
            ("text", Json::Bool(false)),
            (
                "media",
                Json::Bool(row.icon.starts_with("audio") || row.icon.starts_with("video")),
            ),
            ("archive", Json::Bool(row.icon.contains("package"))),
        ])?;
    }
    Ok(())
}
pub fn layout(m: &mut Model) {
    super::layout::prepare(m);
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::testdir::TestDir, jsondoc::Json, tui::model::Row};
    #[test]
    fn text_preview_neutralizes_controls_and_refuses_symlink() {
        let d = TestDir::new("tui-preview");
        d.file("note.txt", "hello\x1b[31m\n\u{202e}world");
        let mut m = Model::new(d.path().into(), &Json::Null);
        m.rows.insert(
            0,
            Row {
                name: "note.txt".into(),
                directory: false,
                size: 20,
                mode: 0o100644,
                link: String::new(),
                kind: "Plain text document".into(),
                thumbnail: false,
                modified: 0, birth: None,
                icon: "text-x-generic".into(),
            },
        );
        load(&mut m, true);
        let body = read_text(&d.join("note.txt"), &AtomicBool::new(false)).unwrap();
        assert!(!body.contains('\x1b'));
        assert!(!body.contains('\u{202e}'));
        assert!(body.contains("hello"));
        m.preview_auto = false;
        d.file("next.txt", "next body");
        m.rows.get_mut(&0).unwrap().name = "next.txt".into();
        load(&mut m, false);
        assert!(m.preview_body.is_empty());
        assert!(!m.preview_loaded);
        assert_eq!(m.preview[0], "next.txt");
        load(&mut m, true);
        assert!(m.preview_loaded);
        assert_eq!(read_text(&d.join("next.txt"), &AtomicBool::new(false)).unwrap(), "next body");
        m.preview_visible = false;
        m.rows.get_mut(&0).unwrap().name = "note.txt".into();
        load(&mut m, true);
        assert!(m.preview_path.as_os_str().is_empty());
        assert!(m.preview_reader.pending.is_none());
        m.quicklook = true;
        load(&mut m, true);
        assert_eq!(m.preview_path, d.join("note.txt"));
        std::os::unix::fs::symlink(d.join("note.txt"), d.join("link")).unwrap();
        m.rows.get_mut(&0).unwrap().name = "link".into();
        load(&mut m, true);
        assert_eq!(read_text(&d.join("link"), &AtomicBool::new(false)).unwrap_err(), "preview.unreadable");
    }
    // The hint under an unloaded preview names the key the keymap binds, and says nothing when none is bound.
    #[test]
    fn the_load_hint_names_the_key_the_keymap_binds() {
        let rebound = crate::tui::keymap::Map::parse(r##"
[[preset]]
name = "default"
mods = "ctrl"
key = "P"
action = "loadPreview"
context = "listing"
"##);
        let d = TestDir::new("tui-preview-hint");
        d.file("note.txt", "body");
        let mut m = Model::new(d.path().into(), &Json::Null);
        let row = Row { name: "note.txt".into(), directory: false, size: 4, mode: 0o100644, link: String::new(), kind: "Plain text document".into(), thumbnail: false, modified: 0, birth: None, icon: "text-x-generic".into() };
        m.rows.insert(0, row);
        m.preview_visible = true;
        m.preview_auto = false;
        m.hints = crate::tui::keymap::Hints::new(&crate::tui::keymap::Map::load(), "default");
        load(&mut m, false);
        assert_eq!(m.preview.last().unwrap(), "Ctrl+Space loads it");
        m.hints = crate::tui::keymap::Hints::new(&rebound, "default");
        m.preview_path.clear();
        load(&mut m, false);
        assert_eq!(m.preview.last().unwrap(), "Ctrl+P loads it");
        m.hints.load_preview.clear();
        m.preview_path.clear();
        load(&mut m, false);
        assert!(!m.preview.iter().any(|line| line.contains("load preview")), "{:?}", m.preview);
    }
    // Data without a NUL reads as text and data with one as nothing; a picture is never read; a link reads its target.
    #[test]
    fn data_reads_as_text_and_a_link_reads_its_target() {
        let d = TestDir::new("tui-preview-data");
        d.file("notes.cfg", "key = value");
        d.file("real.txt", "target body");
        std::os::unix::fs::symlink(d.join("real.txt"), d.join("link")).unwrap();
        assert_eq!(read_text(&d.file("blob.bin", "ab\0cd"), &AtomicBool::new(false)).unwrap(), "");
        let mut m = Model::new(d.path().into(), &Json::Null);
        let row = |name: &str, icon: &str, link: &str| Row { name: name.into(), directory: false, size: 11, mode: 0o100644, link: link.into(), kind: "Data".into(), thumbnail: false, modified: 0, birth: None, icon: icon.into() };
        let settle = |m: &mut Model| {
            let started = std::time::Instant::now();
            while m.preview_reader.outstanding(m.preview_generation) {
                poll(m);
                assert!(started.elapsed() < std::time::Duration::from_secs(5), "the text read did not finish");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            poll(m);
        };
        m.rows.insert(0, row("notes.cfg", "application-octet-stream", ""));
        load(&mut m, true);
        assert!(m.preview_reader.outstanding(m.preview_generation), "data was not read as text");
        settle(&mut m);
        assert_eq!(m.preview_body, "key = value");
        m.rows.insert(0, row("photo.png", "image-png", ""));
        load(&mut m, true);
        assert!(!m.preview_reader.outstanding(m.preview_generation), "a picture was read as text");
        m.rows.insert(0, row("link", "text-plain", "real.txt"));
        load(&mut m, true);
        settle(&mut m);
        assert_eq!(m.preview_body, "target body", "{:?}", m.preview);
    }
    #[test]
    fn text_reader_replaces_pending_work_and_discards_stale_results() {
        let sandbox = TestDir::new("tui-text-reader");
        let path = sandbox.file("next.txt", "current body");
        let (tx, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut reader = Reader::default();
        reader.running = Some((1, cancel.clone(), receiver));
        reader.request(path, 2);
        assert!(cancel.load(Ordering::Relaxed));
        tx.send(Ok("stale body".into())).unwrap();
        assert!(reader.poll().is_none());
        let (generation, _, receiver) = reader.running.take().unwrap();
        assert_eq!(generation, 2);
        assert_eq!(receiver.recv_timeout(std::time::Duration::from_secs(5)).unwrap().unwrap(), "current body");
    }
    // Q3: the TUI boards and v0.3.12 rest todo.md on its head in the pane with its 48 B of text unread; Quick Look still reads it.
    #[test]
    fn a_markdown_document_rests_on_its_head_in_the_pane() {
        let d = TestDir::new("tui-preview-markdown");
        d.file("todo.md", "# Todo\n");
        let mut m = crate::tui::render::frame_tests::model();
        m.path = d.path().into();
        let row = m.rows.get_mut(&7).unwrap();
        (row.name, row.icon, row.kind, row.size) = ("todo.md".into(), "text-markdown".into(), "Markdown document".into(), 48);
        let head = vec!["todo.md".to_string(), "Markdown document · 48 B".to_string()];
        load(&mut m, true);
        assert_eq!(m.preview, head, "the pane asked for the document's text");
        for answered in [false, true] {
            m.quicklook = true;
            load(&mut m, true);
            assert_eq!(m.preview[2], m.preview_state.i18n.tr("preview.loading"), "Quick Look asked the reader for the document");
            let started = std::time::Instant::now();
            while answered && m.preview_body.is_empty() {
                poll(&mut m);
                assert!(started.elapsed() < std::time::Duration::from_secs(5), "Quick Look's read did not finish");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // Each case closes Quick Look through one arm: the read still out, or the body it answered with.
            assert_eq!((m.preview_reader.outstanding(m.preview_generation), m.preview_body.is_empty()), (!answered, !answered));
            m.quicklook = false;
            load(&mut m, false);
            assert_eq!((&m.preview, m.preview_body.as_str()), (&head, ""), "Quick Look closed with its read answered: {answered}");
            assert!(!m.preview_reader.outstanding(m.preview_generation), "the pane kept Quick Look's read in flight");
        }
    }
}

// Receives the controller's frozen frame locale; implemented by the preview projection transaction.
pub fn refresh_locale(m: &mut Model) {
    m.preview_state.i18n = m.i18n.clone();
    if let Some(Text::Args("preview.loadHint", values)) = &mut m.preview_state.text {
        *values = vec![Text::Data(super::cells::clean(&m.hints.load_preview))];
    }
    if let Some(text) = m.preview_state.text.clone() { set_text(m, text); }
    if let Some(key) = m.preview_state.refusal {
        m.preview_refusal = m.preview_state.i18n.tr(key);
    }
    if let Some(key) = m.preview_state.unshown_key {
        if let Some(unshown) = &mut m.preview_state.unshown { unshown.sentence = m.preview_state.i18n.tr(key); }
    }
    words::metadata(m);
    if let Some(player) = &mut m.player { player.refresh_locale(&m.preview_state.i18n); }
    m.preview_width = 0;
    super::pane::prepare(m);
}
pub fn set_text(m: &mut Model, text: Text) {
    m.preview.truncate(2);
    m.preview.push(text.render(&m.preview_state.i18n));
    m.preview_state.text = Some(text);
    m.preview_width = 0;
}
// Called after an accepted peek reply, retaining its semantic refusal independently of rendered words.
pub fn peeked(m: &mut Model, value: &crate::jsondoc::Json) {
    m.preview_state.refusal = if !super::wire::flag(value, "failed") { None }
        else if super::wire::count(value, "mode") > 0 && super::wire::count(value, "mode") & 0o444 == 0 { Some("pane.locked") }
        else { Some("pane.unread") };
    m.preview_refusal = m.preview_state.refusal.map(|key| m.preview_state.i18n.tr(key)).unwrap_or_default();
    m.preview_state.text = None;
    m.preview_width = 0;
}

#[cfg(test)]
#[path = "preview_locale_tests.rs"]
mod locale_tests;
