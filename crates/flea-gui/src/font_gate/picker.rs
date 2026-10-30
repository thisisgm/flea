use super::*;
use crate::g2::pk1::{
    process::{Line, PickerChild},
    Children, Picker,
};

struct Idle(async_channel::Receiver<Line>, async_channel::Sender<Line>);
impl Idle {
    fn new() -> Box<Self> {
        let (sender, receiver) = async_channel::unbounded();
        Box::new(Self(receiver, sender))
    }
    fn listing() -> Box<Self> {
        let child = Self::new();
        for line in [
            r#"{"t":"listed","n":1,"v":1,"w":true,"path":"/files"}"#,
            r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"Font gate picker row.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#,
        ] {
            child.1.try_send(Line::Reply(line.into())).unwrap();
        }
        child
    }
}
impl PickerChild for Idle {
    fn send(&self, _: &str) -> bool {
        !self.1.is_closed()
    }
    fn quit(&self) {}
    fn lines(&self) -> async_channel::Receiver<Line> {
        self.0.clone()
    }
}

pub(super) fn check() -> Vec<String> {
    let mut failed = Vec::new();
    for (name, request) in [
        ("picker-open", r#"{"folder":"/files"}"#),
        (
            "picker-save",
            r#"{"mode":"save","folder":"/files","name":"Font gate save.txt"}"#,
        ),
    ] {
        let (mut cx, recorder) = context();
        recorder.begin();
        let home = std::env::var("HOME").unwrap();
        let handle = cx.add_window(|_, cx| {
            Picker::new(
                flea::ui_logic_pk1::request(request),
                std::path::PathBuf::from(&home).join("picker-reply"),
                &home,
                locale(),
                Children {
                    checks: Idle::new(),
                    listing: Idle::listing(),
                },
                cx,
            )
        });
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        handle
            .update(&mut cx, |picker, window, cx| {
                picker
                    .browse
                    .path_view(cx)
                    .update(cx, |path, cx| path.open(window, cx));
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let mut required = vec!["Home", "Cancel", "/files", "Font gate picker row.txt"];
        if name == "picker-save" {
            required.extend(["Save", "SAVE AS", "Font gate save.txt"]);
        }
        if !assert_runs(name, &recorder, &required) {
            failed.push(name.into());
        }
    }
    failed
}
