//! The legacy script listing, ported line for line from `ui/js/Scripts.js`; sample input, one name a line: convert-to-webp, ocr.sh.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub id: String,
    pub label: String,
    pub path: String,
}

// What find printed becomes the submenu rows, sorted by name, each label the file name without its extension.
pub fn parse(text: &str, directory: &str) -> Vec<Script> {
    let mut names: Vec<&str> = text
        .split('\n')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|name| {
            let dot = name.rfind('.').unwrap_or(0);
            Script {
                id: name.into(),
                label: if dot > 0 {
                    name[..dot].into()
                } else {
                    name.into()
                },
                path: format!("{directory}/{name}"),
            }
        })
        .collect()
}

// Whole sentences with %1 for a value, filled so each translates as one.
pub const EXITED_KEY: &str = "actions.script.exited";
pub const GONE_KEY: &str = "actions.script.gone";

/// The script's own report: its last stderr line, or the name and status it exited with when it said nothing, so the sentence can name both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Line(String),
    Exited { name: String, code: i32 },
}

pub fn failure(name: &str, stderr: &str, code: i32) -> Failure {
    match stderr
        .split('\n')
        .map(str::trim)
        .rfind(|line| !line.is_empty())
    {
        Some(line) => Failure::Line(format!("{name} \u{00b7} {line}")),
        None => Failure::Exited {
            name: name.into(),
            code,
        },
    }
}

// The oracle's sentence for a failure: the last stderr line, or the catalog's exited template with the name and the status it exited with.
pub fn sentence(failure: &Failure, i18n: &crate::i18n::I18n) -> String {
    match failure {
        Failure::Line(line) => line.clone(),
        Failure::Exited { name, code } => {
            i18n.tr_args(EXITED_KEY, &[name, &format!("{code}")])
        }
    }
}
