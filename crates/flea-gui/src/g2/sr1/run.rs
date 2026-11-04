//! A result's typed run and location caption: the run washed at rest and bold inside the cursor's highlight.
use flea::ui_logic_sr1::{self as core, Scope};
use gpui::{Font, FontWeight, Hsla, LineLayout, Pixels, TextRun};

// ui.py .mrun: the run's wash is the accent at 14%, the cursor rung's 18% wash stays the row's.
pub const RUN_WASH: f32 = 0.14;

/// The byte range of the typed run in a label as drawn, so an elided label never washes a cut run.
pub fn run_in(label: &str, query: &str) -> Option<(usize, usize)> {
    core::run(label, query)
}

/// One label's text runs: the typed run in bold inside the cursor's highlight, one plain run otherwise.
pub fn text_runs(
    label: &str,
    run: Option<(usize, usize)>,
    lifted: bool,
    font: &Font,
) -> Vec<TextRun> {
    let plain = |len: usize, font: Font| TextRun {
        len,
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    match run {
        Some((start, end)) if lifted && end > start => {
            let bold = Font {
                weight: FontWeight::BOLD,
                ..font.clone()
            };
            [
                (start, font.clone()),
                (end - start, bold),
                (label.len() - end, font.clone()),
            ]
            .into_iter()
            .filter(|(len, _)| *len > 0)
            .map(|(len, font)| plain(len, font))
            .collect()
        }
        _ => vec![plain(label.len(), font.clone())],
    }
}

/// The wash's span on the shaped label, for a row outside the cursor's highlight.
pub fn wash_span(
    line: &LineLayout,
    run: Option<(usize, usize)>,
    lifted: bool,
) -> Option<(Pixels, Pixels)> {
    let (start, end) = run?;
    (!lifted && end > start).then(|| (line.x_for_index(start), line.x_for_index(end)))
}

/// The folder caption after a result's name: from the scope in one folder, in full as the chrome writes it everywhere.
pub fn location(path: &str, root: &str, home: &str, scope: Scope) -> String {
    let folder = core::location(path);
    match scope {
        Scope::Everywhere if folder.is_empty() => core::tilde(root, home),
        Scope::Everywhere if root == "/" => core::tilde(&format!("/{folder}"), home),
        Scope::Everywhere => core::tilde(&format!("{root}/{folder}"), home),
        Scope::Folder | Scope::Home => folder.to_owned(),
    }
}
