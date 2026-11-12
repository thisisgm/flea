//! Opted-in picker listing replies carry the request generation, including failures and initial rows.
use std::{
    fmt,
    io::{self, Write},
};

pub(crate) struct Writer<'a, W: Write> {
    inner: &'a mut W,
    generation: Option<usize>,
}

impl<'a, W: Write> Writer<'a, W> {
    pub(crate) fn new(inner: &'a mut W, generation: Option<usize>) -> Self {
        Self { inner, generation }
    }
}

impl<W: Write> Write for Writer<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }

    // Sample input: {"t":"rows","start":0,"rows":[]}\n, written as {"t":"rows","start":0,"rows":[],"pickerGen":7}\n
    // Listing emitters format one complete JSON frame per write, so other callers keep their original bytes.
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        let Some(generation) = self.generation else {
            return self.inner.write_fmt(args);
        };
        let mut line = args.to_string();
        let closing = if line.ends_with("}\n") {
            Some(line.len() - 2)
        } else if line.ends_with('}') {
            Some(line.len() - 1)
        } else {
            None
        };
        if let Some(closing) = closing {
            line.insert_str(closing, &format!(",\"pickerGen\":{generation}"));
        }
        self.inner.write_all(line.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untagged_requests_keep_their_original_frames() {
        let mut bytes = Vec::new();
        writeln!(
            Writer::new(&mut bytes, None),
            "{{\"t\":\"listed\",\"n\":{}}}",
            2
        )
        .unwrap();
        assert_eq!(bytes, b"{\"t\":\"listed\",\"n\":2}\n");
    }

    #[test]
    fn listing_rows_and_errors_echo_the_same_generation() {
        let mut bytes = Vec::new();
        let mut writer = Writer::new(&mut bytes, Some(17));
        for line in [
            r#"{"t":"listed","n":1}"#,
            r#"{"t":"rows","rows":[{"n":"x}"}]}"#,
            r#"{"t":"error","where":"scan","msg":"denied"}"#,
        ] {
            writeln!(writer, "{line}").unwrap();
        }
        let text = String::from_utf8(bytes).unwrap();
        for line in text.lines() {
            assert_eq!(crate::json::field_usize(line, "pickerGen"), Some(17));
            assert!(crate::jsondoc::parse(line).is_ok());
        }
    }

    #[test]
    fn actual_list_success_and_scan_failure_echo_the_request_generation() {
        use crate::backend::{
            dirsizeworker, events,
            iomount::ListErr,
            listwork::{self, Done, Gate, Job, Landed, Started},
            state::{State, Tables},
            testdir::TestDir,
            thumbs::Pool,
            watch::Watch,
            watchpoll::Poller,
        };
        use std::sync::Arc;
        let dir = TestDir::new("picker-request-generation");
        dir.file("a}.txt", "a");
        let path = dir.path().to_string_lossy().into_owned();
        let (tx, _rx) = events::channel();
        let mut state = State::new(dirsizeworker::Worker::new(tx.clone()));
        let tables = Tables::load();
        let (thumbs_tx, _thumbs_rx) = std::sync::mpsc::channel();
        let pool = Pool::new(
            1,
            thumbs_tx,
            dir.join("thumbs"),
            Arc::clone(&tables.aliases),
            Arc::clone(&tables.thumbs),
        );
        let mut watch = Watch::start(tx.clone());
        let poller = Poller::new(tx.clone());
        let mut gate = Gate::new(tx);
        let job = |path, generation| Job {
            path,
            first: 10,
            line: format!(r#"{{"pickerGen":{generation}}}"#),
            hidden: false,
            want_changed: false,
            session: None,
        };
        let mut bytes = Vec::new();
        let Started::Inline(landed) = listwork::start(
            &mut bytes,
            &mut state,
            &pool,
            &mut watch,
            &mut gate,
            job(path, 17),
            Arc::clone(&tables.mime),
        ) else {
            panic!("local scan runs inline")
        };
        assert!(listwork::finish(
            &mut bytes, &mut state, &tables, &pool, &mut watch, &poller, *landed
        ));
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text
            .lines()
            .all(|line| crate::json::field_usize(line, "pickerGen") == Some(17)));
        let missing = dir.join("gone").to_string_lossy().into_owned();
        let failed = Landed {
            job: job(missing.clone(), 18),
            done: Done {
                result: Err(ListErr {
                    error: crate::error::FleaError {
                        where_: "scan".into(),
                        path: missing,
                        msg: "denied".into(),
                    },
                    mode: 0,
                    watch_wd: -1,
                }),
            },
        };
        let mut bytes = Vec::new();
        assert!(!listwork::finish(
            &mut bytes, &mut state, &tables, &pool, &mut watch, &poller, failed
        ));
        assert_eq!(
            crate::json::field_usize(&String::from_utf8(bytes).unwrap(), "pickerGen"),
            Some(18)
        );
    }
}
