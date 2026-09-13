use crate::jsondoc::{self, Json};
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

pub struct Wire {
    child: Child,
    input: ChildStdin,
    pub events: Receiver<Result<Json, String>>,
}
impl Wire {
    pub fn start() -> io::Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--backend")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("backend stdin unavailable"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("backend stdout unavailable"))?;
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let result = line
                    .map_err(|e| e.to_string())
                    .and_then(|line| jsondoc::parse(&line));
                if tx.send(result).is_err() {
                    return;
                }
            }
            let _ = tx.send(Err("The listing backend stopped".into()));
        });
        Ok(Self {
            child,
            input,
            events,
        })
    }
    pub fn send(&mut self, fields: Vec<(&str, Json)>) -> io::Result<()> {
        let document = Json::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect());
        let text = jsondoc::render(&document).replace('\n', "");
        writeln!(self.input, "{}", text)?;
        self.input.flush()
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        let _ = self.send(vec![("c", word("quit"))]);
        // A requested shutdown must not orphan a backend; its own quit drains operation workers.
        let _ = self.child.wait();
    }
}
pub fn word(value: &str) -> Json {
    Json::Str(value.into())
}
pub fn number(value: usize) -> Json {
    Json::Num(value.to_string())
}
pub fn text<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap_or("")
}
pub fn count(value: &Json, key: &str) -> usize {
    value
        .get(key)
        .and_then(Json::as_f64)
        .unwrap_or(0.0)
        .max(0.0) as usize
}
pub fn flag(value: &Json, key: &str) -> bool {
    value.get(key).and_then(Json::as_bool).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::model::{Model, Row};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn hidden_reload_restores_cursor_after_the_older_window_reply() {
        let quit = jsondoc::render(&Json::Obj(vec![("c".into(), word("quit"))])).replace('\n', "");
        // Echo actual Wire::send output; honoring Quit also lets Wire::drop reap the child during a panic.
        let mut child = Command::new("sh").args(["-c",
            r#"while IFS= read -r line; do [ "$line" = "$1" ] && exit 0; printf '%s\n' "$line"; done"#,
            "flea-tui-wire-test", &quit]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = line
                    .map_err(|error| error.to_string())
                    .and_then(|line| jsondoc::parse(&line));
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut wire = Wire {
            child,
            input,
            events,
        };
        let next = |wire: &Wire| {
            wire.events
                .recv_timeout(Duration::from_secs(2))
                .expect("TUI did not request the expected listing window")
                .unwrap()
        };
        let row = |name: &str| Json::Obj(vec![("n".into(), word(name))]);
        let rows = |start, names: &[&str]| {
            Json::Obj(vec![
                ("t".into(), word("rows")),
                ("start".into(), number(start)),
                (
                    "rows".into(),
                    Json::Arr(names.iter().map(|name| row(name)).collect()),
                ),
            ])
        };
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.total = 6;
        model.cursor = 3;
        model.top = 2;
        model.rows.insert(3, Row::parse(&row("charlie.txt"), &[]));
        model.open(model.path.clone(), &mut wire).unwrap();
        assert_eq!(text(&next(&wire), "c"), "list");

        // List emits listed plus rows(0); handling listed already queued window(3) and locate(charlie).
        model
            .receive(
                jsondoc::parse(r#"{"t":"listed","n":5}"#).unwrap(),
                &mut wire,
            )
            .unwrap();
        assert_eq!(text(&next(&wire), "c"), "peek");
        let older_window = next(&wire);
        assert_eq!(text(&older_window, "c"), "window");
        assert_eq!(count(&older_window, "start"), 3);
        let locate = next(&wire);
        assert_eq!(text(&locate, "c"), "locate");
        assert_eq!(text(&locate, "path"), "/listing/charlie.txt");
        let names = ["amber", "bronze", "charlie.txt", "delta.txt", "echo.txt"];
        model.receive(rows(0, &names), &mut wire).unwrap();
        let restored_window = next(&wire);
        assert_eq!(text(&restored_window, "c"), "window");
        assert_eq!(count(&restored_window, "start"), 2);
        assert_eq!(count(&restored_window, "count"), model.height);

        model.receive(rows(3, &names[3..]), &mut wire).unwrap();
        assert!(
            model.current_path().is_none(),
            "the older reply does not contain the restored cursor"
        );
        model.receive(jsondoc::parse(r#"{"t":"located","directory":"/listing","path":"/listing/charlie.txt","index":2}"#).unwrap(), &mut wire).unwrap();
        assert!(model.restore_path.is_none());
        model
            .receive(
                rows(count(&restored_window, "start"), &names[2..]),
                &mut wire,
            )
            .unwrap();
        assert_eq!((model.cursor, model.top), (2, 2));
        assert_eq!(
            model.current_path(),
            Some(PathBuf::from("/listing/charlie.txt"))
        );
        assert!(model.cursor < model.top + model.height);
        wire.send(vec![("c", word("quit"))]).unwrap();
        assert!(
            wire.child.wait().unwrap().success(),
            "TUI test wire child did not exit cleanly"
        );
        reader.join().unwrap();
        assert!(
            wire.events.try_recv().is_err(),
            "restoration queued an unexpected extra request"
        );
    }
}
