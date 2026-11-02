use super::server::Answer;
use flea::ipc::{
    registry,
    wire::{Reply, Request},
};

pub struct Surface<'a> {
    pub path: &'a str,
    pub total: usize,
    pub cursor: usize,
    pub selected: &'a [usize],
}

pub trait Adapter {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply>;
}

impl Surface<'_> {
    pub fn dispatch(&self, request: Request) -> Answer {
        self.dispatch_to(request, &[])
    }

    pub fn dispatch_to(&self, request: Request, adapters: &[&dyn Adapter]) -> Answer {
        let Request::Call {
            target,
            function,
            arguments,
        } = request
        else {
            return Answer {
                reply: registry::show(),
                quit: false,
            };
        };
        let function = match registry::validate(&target, &function, &arguments) {
            Ok(function) => function,
            Err(reply) => return Answer { reply, quit: false },
        };
        // A bool parameter accepts 1 and 0 as Quickshell does; readers compare against "true", so spell them out once here.
        let arguments: Vec<String> = function
            .parameters
            .iter()
            .zip(arguments)
            .map(|((_, kind), value)| match (*kind, value.as_str()) {
                ("bool", "1") => "true".into(),
                ("bool", "0") => "false".into(),
                _ => value,
            })
            .collect();
        let adapted = (target == "flea")
            .then(|| {
                adapters
                    .iter()
                    .find_map(|adapter| adapter.call(function.name, &arguments))
            })
            .flatten();
        let reply = match (adapted, target.as_str(), function.name) {
            (Some(reply), _, _) => reply,
            (_, _, "version") => Reply::value(env!("CARGO_PKG_VERSION")),
            (_, "flea", "quit") => Reply {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            (
                _,
                "flea",
                "ready" | "path" | "total" | "cursor" | "selectionCount" | "selectedIndices",
            ) => {
                let value = match function.name {
                    "ready" => "true".into(),
                    "path" => self.path.into(),
                    "total" => self.total.to_string(),
                    "cursor" => self.cursor.to_string(),
                    "selectionCount" => self.selected.len().to_string(),
                    "selectedIndices" => self
                        .selected
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    _ => unreachable!(),
                };
                Reply::value(value)
            }
            _ => registry::refused(function),
        };
        Answer {
            // The chooser's quit is refused here; it reaches a chooser only through `--pid` (G0.10 seam).
            quit: target == "flea" && function.name == "quit",
            reply,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_model_and_never_fakes_a_picker() {
        let surface = Surface {
            path: "/files/a\nb",
            total: 12,
            cursor: 3,
            selected: &[1, 3, 7],
        };
        for (name, expected) in [
            ("ready", "true\n"),
            ("path", "/files/a\nb\n"),
            ("total", "12\n"),
            ("cursor", "3\n"),
            ("selectionCount", "3\n"),
            ("selectedIndices", "1,3,7\n"),
        ] {
            let answer = surface.dispatch(Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: vec![],
            });
            assert_eq!(
                answer.reply,
                Reply {
                    code: 0,
                    stdout: expected.into(),
                    stderr: String::new()
                }
            );
            assert!(!answer.quit);
        }
        let answer = surface.dispatch(Request::Call {
            target: "fleapicker".into(),
            function: "path".into(),
            arguments: vec![],
        });
        assert_eq!(
            answer.reply,
            Reply::error(3, "FLEA_IPC_NOT_PORTED fleapicker.path")
        );
    }

    struct Echo;

    impl Adapter for Echo {
        fn call(&self, _: &str, arguments: &[String]) -> Option<Reply> {
            Some(Reply::value(arguments.join(",")))
        }
    }

    // A bool parameter's 1 and 0 reach every adapter as true and false; an int parameter's 1 stays 1.
    #[test]
    fn fixreaders_bool_arguments_accept_one_and_zero() {
        let surface = Surface {
            path: "/",
            total: 0,
            cursor: 0,
            selected: &[],
        };
        let call = |function: &str, argument: &str| {
            surface
                .dispatch_to(
                    Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: vec![argument.into()],
                    },
                    &[&Echo],
                )
                .reply
        };
        assert_eq!(call("pdfState", "1"), Reply::value("true"));
        assert_eq!(call("pdfState", "0"), Reply::value("false"));
        assert_eq!(call("pdfState", "true"), Reply::value("true"));
        assert_eq!(call("railRowCentre", "1"), Reply::value("1"));
    }

    // Review g2-pk1-r2-2: a main window refuses the chooser's quit, so `flea ipc call fleapicker quit` never closes it.
    #[test]
    fn a_main_window_never_quits_for_the_chooser() {
        let surface = Surface {
            path: "/",
            total: 0,
            cursor: 0,
            selected: &[],
        };
        let call = |target: &str| {
            surface.dispatch(Request::Call {
                target: target.into(),
                function: "quit".into(),
                arguments: vec![],
            })
        };
        let refused = call("fleapicker");
        assert_eq!(
            (refused.reply, refused.quit),
            (
                Reply::error(3, "FLEA_IPC_NOT_PORTED fleapicker.quit"),
                false
            )
        );
        assert!(call("flea").quit, "the window's own quit still quits it");
    }
}
