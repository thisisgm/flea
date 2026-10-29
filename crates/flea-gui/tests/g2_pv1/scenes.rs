//! The boards' own subjects, scene by scene (g2.pv1.<board>.primary), with the strings each board draws.
use super::repo;
use flea::i18n::I18n;
use flea::ui_logic_pv1::{compare, kinds::State, meta::CompareState, rect::Shape};
use flea_gui::g2::pv1::{glyph::Glyph, Body, Control, Subject};

const FILE: u32 = 0o100644;
const LINK: u32 = 0o120777;

fn shape(kind: State, w: f64, h: f64) -> Shape {
    Shape {
        kind,
        known: w > 0.,
        w,
        h,
        orient: 1,
    }
}

fn picture() -> std::path::PathBuf {
    repo("docs/images/columns.png")
}

pub const PATCH: &str = "--- /w/notes-a.md\n+++ /w/notes-b.md\n@@ -9,12 +9,12 @@\n ## Previews\n \n Each kind asks for what it shows.\n\
-Thumbnails never load on phones.\n-The map holds every row.\n-A taller preview costs nothing.\n-Ports keep their slots.\n-Marks ship one size.\n-Facts fold into two lines.\n\
+Thumbnails stay cache-only on phones.\n+The map holds the visible rows.\n+A taller preview costs what a shorter one did.\n+Ports keep their slots while scrolling.\n+Marks share one optical size.\n+Facts fold into one line.\n \
Only the visible lines are asked for.\n A newer request cancels an older one.\n The frame runs to the column bottom.\n";

fn subject(name: &str, icon: &str, mode: u32, state: State, shape: Shape, body: Body) -> Subject {
    Subject {
        name: name.into(),
        icon: icon.into(),
        mode,
        state,
        shape,
        body,
    }
}

/// g2.pv1.ql-text.primary
pub fn ql_text(i18n: &I18n) -> Subject {
    let lines = [
        "Visual gate notes",
        "",
        "This file is the fixture's plain text sample.",
        "Line four keeps the preview busy.",
    ];
    let count = i18n.trn("preview.lines", 4);
    let body = Body::Text {
        lines: lines.iter().map(|l| l.to_string()).collect(),
        count,
    };
    subject(
        "notes.txt",
        "text-x-generic",
        FILE,
        State::Text,
        Shape::unknown(State::Text),
        body,
    )
}

/// g2.pv1.ql-image.primary
pub fn ql_image() -> Subject {
    let body = Body::Image {
        path: picture(),
        w: 800.,
        h: 600.,
    };
    subject(
        "photo.png",
        "image-x-generic",
        FILE,
        State::Image,
        shape(State::Image, 800., 600.),
        body,
    )
}

/// An 800x600 vector picture (#336): the same logical fit as ql-image, rasterized at physical pixels.
pub fn ql_svg() -> Subject {
    let body = Body::Image {
        path: repo("docs/images/glyphs.svg"),
        w: 800.,
        h: 600.,
    };
    subject(
        "mark.svg",
        "image-x-generic",
        FILE,
        State::Image,
        shape(State::Image, 800., 600.),
        body,
    )
}

/// g2.pv1.ql-symlink.primary
pub fn ql_symlink(i18n: &I18n) -> Subject {
    let body = Body::Link {
        path: picture(),
        w: 800.,
        h: 600.,
        target: i18n.tr_args("preview.link_target", &["photo.png"]),
    };
    subject(
        "link.png",
        "image-x-generic",
        LINK,
        State::Symlink,
        Shape::unknown(State::Symlink),
        body,
    )
}

/// g2.pv1.ql-raw.primary
pub fn ql_raw(i18n: &I18n) -> Subject {
    let caption = i18n.tr_args("preview.embedded", &["1620", "1080"]);
    let body = Body::Raw {
        path: picture(),
        w: 1620.,
        h: 1080.,
        caption,
    };
    subject(
        "L1000921.DNG",
        "image-x-generic",
        FILE,
        State::Image,
        Shape::unknown(State::Unsupported),
        body,
    )
}

/// g2.pv1.ql-error.primary: the PDF strip's zoom out and zoom in disabled, fit and close live.
pub fn ql_error(i18n: &I18n) -> Subject {
    let controls = vec![
        Control {
            glyph: Glyph::Minus,
            enabled: false,
        },
        Control {
            glyph: Glyph::Plus,
            enabled: false,
        },
        Control {
            glyph: Glyph::Maximize,
            enabled: true,
        },
    ];
    let body = Body::cannot_show(i18n, State::Pdf, false, true, controls)
        .expect("a failed read is refused");
    subject(
        "broken.pdf",
        "x-office-document",
        FILE,
        State::Pdf,
        Shape::unknown(State::Pdf),
        body,
    )
}

/// g2.pv1.ql-unsupported.primary
pub fn ql_unsupported(i18n: &I18n) -> Subject {
    let body = Body::cannot_show(i18n, State::Unsupported, false, false, Vec::new())
        .expect("no preview kind");
    subject(
        "letter.odt",
        "x-office-document",
        FILE,
        State::Unsupported,
        Shape::unknown(State::Unsupported),
        body,
    )
}

/// g2.pv1.compare.primary
pub fn compare_diff(i18n: &I18n) -> Subject {
    let rows = compare::rows(PATCH);
    let title = compare::title(
        "notes-a.md",
        "notes-b.md",
        Some(compare::changed(&rows)),
        i18n,
    );
    let body = Body::Diff {
        title,
        copy: i18n.tr("preview.compare.copy_patch"),
        rows,
    };
    subject(
        "notes-a.md",
        "text-x-generic",
        FILE,
        State::Text,
        Shape::unknown(State::Unsupported),
        body,
    )
}

fn refused(name: &str, other: &str, icon: &str, state: CompareState, i18n: &I18n) -> Subject {
    let title = compare::title(name, other, None, i18n);
    let body = Body::Refused {
        title,
        sentence: compare::sentence(&state, i18n),
    };
    subject(
        name,
        icon,
        FILE,
        State::Text,
        Shape::unknown(State::Unsupported),
        body,
    )
}

/// g2.pv1.compare-binary.primary
pub fn compare_binary(i18n: &I18n) -> Subject {
    refused(
        "shot-a.png",
        "shot-b.png",
        "image-x-generic",
        CompareState::Binary,
        i18n,
    )
}

/// g2.pv1.compare-identical.primary
pub fn compare_identical(i18n: &I18n) -> Subject {
    refused(
        "notes-a.md",
        "notes-b.md",
        "text-x-generic",
        CompareState::Identical,
        i18n,
    )
}

/// g2.pv1.compare-large.primary
pub fn compare_large(i18n: &I18n) -> Subject {
    refused(
        "dump-a.log",
        "dump-b.log",
        "text-x-generic",
        CompareState::Over,
        i18n,
    )
}
