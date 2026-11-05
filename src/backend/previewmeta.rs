//! Path-based preview facts. The preview window never changes the browser listing.
use super::{archive::Formats, metareq, opsreq::OpMsg};
use crate::jsondoc::{self, Json};
use std::{
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{mpsc::Sender, Arc},
};

pub fn spawn(line: &str, formats: Arc<Formats>, tx: Sender<OpMsg>) {
    let Ok(doc) = jsondoc::parse(line) else {
        return;
    };
    let Some(path) = doc
        .get("path")
        .and_then(Json::as_str)
        .filter(|p| p.starts_with('/'))
    else {
        return;
    };
    let path = PathBuf::from(path);
    let count = |key| doc.get(key).and_then(Json::as_f64).unwrap_or(0.) as usize;
    let row = count("row");
    let token = count("token");
    let media = doc.get("media").and_then(Json::as_bool) == Some(true);
    let archive = doc.get("archive").and_then(Json::as_bool) == Some(true);
    std::thread::spawn(move || {
        let mut meta = metareq::read(&path, false, false, archive.then_some(formats.as_ref()));
        let probe = if media {
            super::mediaprobe::probe(&path)
        } else {
            Default::default()
        };
        if media {
            meta.duration_ms = probe.duration_ms;
            meta.sample_rate = probe.sample_rate;
            if meta.width == 0 {
                meta.width = probe.width;
                meta.height = probe.height;
            }
        }
        let mut line = metareq::meta_reply(row, &meta, token);
        line.pop();
        line.push_str(&format!(
            ",\"fps\":{},\"bitrate\":{}}}",
            probe.frame_rate, probe.bit_rate
        ));
        if let Ok(stat) = path.metadata() {
            line.pop();
            line.push_str(&format!(
                ",\"size\":{},\"mtime\":{},\"mode\":{}}}",
                stat.len(),
                stat.mtime(),
                stat.mode()
            ));
        }
        let _ = tx.send(OpMsg::Meta { line });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_port_path_facts_do_not_depend_on_the_browser_listing() {
        let root = super::super::testdir::TestDir::new("custom-preview-meta");
        let path = root.file("file with spaces.txt", "hello");
        let (tx, rx) = std::sync::mpsc::channel();
        let line = format!(
            r#"{{"path":"{}","row":17,"token":91}}"#,
            crate::json::escape(&path.to_string_lossy())
        );
        spawn(&line, Arc::new(Formats::from_tools(false, false)), tx);
        let OpMsg::Meta { line } = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()
        else {
            panic!("meta reply")
        };
        let reply = jsondoc::parse(&line).unwrap();
        assert_eq!(reply.get("token").and_then(Json::as_f64), Some(91.));
        assert_eq!(reply.get("row").and_then(Json::as_f64), Some(17.));
        assert_eq!(reply.get("size").and_then(Json::as_f64), Some(5.));
    }
}
