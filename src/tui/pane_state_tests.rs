use super::*;
use crate::tui::render::frame_tests::{entry, model};

fn row(name: &str, icon: &str, link: &str) -> Row {
    let mut row = entry(name, false, 1);
    (row.icon, row.link) = (icon.into(), link.into());
    row
}

fn reply(text: &str) -> Json {
    crate::jsondoc::parse(text).unwrap()
}

#[test]
fn kinds_follow_the_listing_and_a_link_takes_its_target() {
    assert_eq!(kind(&entry("Docs", true, 0)), Kind::Folder);
    assert_eq!(kind(&row("paper.PDF", "text-x-generic", "")), Kind::Pdf);
    assert_eq!(kind(&row("paper", "text-x-generic", "../shelf/paper.pdf")), Kind::Pdf);
    assert_eq!(kind(&row("photo.png", "image-png", "")), Kind::Image);
    assert_eq!(kind(&row("song.flac", "audio-x-flac", "")), Kind::Audio);
    assert_eq!(kind(&row("clip.mp4", "video-mp4", "")), Kind::Video);
    assert_eq!(kind(&row("bundle.zip", "package-x-generic", "")), Kind::Archive);
    assert_eq!(kind(&row("blob.bin", "application-octet-stream", "")), Kind::Data);
    let mut photo = row("photo.png", "image-png", "");
    assert!(!pictured(&photo), "a picture the listing cannot thumbnail is not decoded");
    photo.thumbnail = true;
    assert!(pictured(&photo));
    assert!(pictured(&row("photo.png", "image-png", "shots/photo.png")));
}

#[test]
fn a_link_reads_its_target_joined_onto_the_listing() {
    let mut m = model();
    m.cursor = 7;
    assert_eq!(content(&m), Some(m.path.join("notes.txt")));
    m.rows.get_mut(&7).unwrap().link = "../shared/./pic.png".into();
    assert_eq!(content(&m).as_deref(), Some(std::path::Path::new("/fx/v/shared/pic.png")));
    m.rows.get_mut(&7).unwrap().link = "/srv/pic.png".into();
    assert_eq!(content(&m).as_deref(), Some(std::path::Path::new("/srv/pic.png")));
}

#[test]
fn an_archive_reply_names_its_count_entries_and_a_lone_entry_size() {
    let mut m = model();
    meta(&mut m, &reply(r#"{"t":"meta","entries":1,"unpacked":156,"names":[{"n":"notes.txt","d":false}]}"#));
    let archive = m.preview_state.archive.as_ref().unwrap();
    assert_eq!(archive.rows.iter().map(|(row, size)| (row.name.as_str(), *size)).collect::<Vec<_>>(), [("notes.txt", Some(156))]);
    assert_eq!(archive_line(&m.preview_state).as_deref(), Some("1 entry · 156 B unpacked"));
    meta(&mut m, &reply(r#"{"t":"meta","entries":3,"unpacked":2048,"names":[{"n":"src","d":true},{"n":"a.txt","d":false,"s":12}]}"#));
    let archive = m.preview_state.archive.as_ref().unwrap();
    assert_eq!(archive.rows.iter().map(|(row, size)| (row.name.as_str(), row.directory, *size)).collect::<Vec<_>>(), [("src", true, None), ("a.txt", false, Some(12))]);
    assert_eq!(archive_line(&m.preview_state).as_deref(), Some("3 entries · 2.0 kB unpacked"));
    reset(&mut m.preview_state);
    meta(&mut m, &reply(r#"{"t":"meta","w":800,"h":600}"#));
    assert!(m.preview_state.archive.is_none() && m.preview_state.unshown.is_none(), "a picture's reply is not an archive");
    meta(&mut m, &reply(r#"{"t":"meta","afailed":true}"#));
    assert!(m.preview_state.archive.is_none());
    let unshown = m.preview_state.unshown.as_ref().unwrap();
    assert_eq!((unshown.sentence.as_str(), unshown.open_with), ("This archive could not be read.", false));
}

#[test]
fn a_new_preview_forgets_the_last_archive_sentence_and_runs() {
    let mut m = model();
    meta(&mut m, &reply(r#"{"t":"meta","entries":2,"unpacked":10,"names":[]}"#));
    m.preview_state.unshown = Some(Unshown { sentence: "x".into(), open_with: true });
    crate::tui::preview::load(&mut m, true);
    assert!(m.preview_state.archive.is_none() && m.preview_state.unshown.is_none());
}
