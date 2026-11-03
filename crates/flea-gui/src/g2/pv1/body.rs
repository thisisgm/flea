//! What Quick Look shows for one subject, each kind as its board draws it; every string is already translated.
use super::{doc::View, glyph::Glyph};
use flea::i18n::I18n;
use flea::ui_logic_pv1::{
    compare::DiffRow,
    kinds::{self, State},
    pixels::is_vector_name,
    rect::Shape,
    swap::js_round,
};
use std::path::{Path, PathBuf};

/// A bar control left of Close, as the PDF strip lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub glyph: Glyph,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// ql-text: the lines under the bar; count is the bar's muted tail, "4 lines".
    Text { lines: Vec<String>, count: String },
    /// ql-image: the picture alone on the background, fit to the card.
    Image { path: PathBuf, w: f64, h: f64 },
    /// ql-symlink: the target's picture under a bar naming the link and "→ target" in the metadata ink.
    Link {
        path: PathBuf,
        w: f64,
        h: f64,
        target: String,
    },
    /// ql-raw: the embedded picture over its muted caption band.
    Raw {
        path: PathBuf,
        w: f64,
        h: f64,
        caption: String,
    },
    /// ql-unsupported (calm, framed card, no bar) or ql-error (the kind's bar and controls, the error mark).
    CannotShow {
        sentence: String,
        error: bool,
        controls: Vec<Control>,
    },
    /// compare: the title, Copy Patch and the patch rows.
    Diff {
        title: String,
        copy: String,
        rows: Vec<DiffRow>,
    },
    /// compare-binary, compare-identical, compare-large: the title and why there is no patch.
    Refused { title: String, sentence: String },
    /// Every other board kind: the view the preview column mounts for it, sized to the card.
    Doc(super::doc::Doc),
}

impl Body {
    /// The can't-show card for a refused preview: a failed read keeps the kind's bar, its strip and the error mark.
    pub fn cannot_show(
        i18n: &I18n,
        state: State,
        too_large: bool,
        failed: bool,
        controls: Vec<Control>,
    ) -> Option<Self> {
        let key = kinds::cannot_show_key(state, too_large, failed)?;
        Some(Self::CannotShow {
            sentence: i18n.tr(key),
            error: !too_large && (failed || state == State::Error),
            controls,
        })
    }

    pub fn has_bar(&self) -> bool {
        match self {
            Self::Doc(doc) => doc.has_bar(),
            _ => !matches!(
                self,
                Self::Image { .. } | Self::Raw { .. } | Self::CannotShow { error: false, .. }
            ),
        }
    }

    /// The text, unsupported and compare cards are framed surfaces; a picture or a document pane fills a bare background.
    pub fn framed(&self, cx: &gpui::App) -> bool {
        if let Self::Doc(doc) = self {
            return doc.framed(cx);
        }
        matches!(
            self,
            Self::Text { .. }
                | Self::Diff { .. }
                | Self::Refused { .. }
                | Self::CannotShow { error: false, .. }
        )
    }

    pub fn doc(&self) -> Option<&super::doc::Doc> {
        match self {
            Self::Doc(doc) => Some(doc),
            _ => None,
        }
    }

    pub fn is_compare(&self) -> bool {
        matches!(self, Self::Diff { .. } | Self::Refused { .. })
    }

    pub fn offers_open_with(&self) -> bool {
        matches!(self, Self::CannotShow { .. })
    }

    pub fn controls(&self) -> &[Control] {
        match self {
            Self::CannotShow { controls, .. } => controls,
            _ => &[],
        }
    }
}

/// The file Quick Look is open on: its name, the icon and mode its kind mark comes from, and its body.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub name: String,
    pub icon: String,
    pub mode: u32,
    pub state: State,
    pub shape: Shape,
    pub body: Body,
}

impl Subject {
    /// QuickLookBar's language mark, and CompareView's own mark for a comparison.
    pub fn mark(&self) -> Glyph {
        self.mark_with_languages(true)
    }

    /// The model view wears the cube; comparisons and other files keep their Markdown, language or kind mark.
    pub fn mark_with_languages(&self, languages: bool) -> Glyph {
        if self
            .body
            .doc()
            .is_some_and(|doc| matches!(doc.view, View::Model(_)))
        {
            return Glyph::Cube;
        }
        if self.body.is_compare() && flea::ui_logic_pv1::kinds::is_markdown(&self.name) {
            return Glyph::Markdown;
        }
        if self.body.is_compare() {
            return Glyph::kind(&self.icon, self.mode);
        }
        Glyph::Row(crate::row::marks::Mark::for_file(
            &self.name,
            self.mode & 0o170000 == 0o040000,
            self.mode,
            &self.icon,
            languages,
        ))
    }
}

impl super::QuickLook {
    pub fn set_languages(&mut self, languages: bool, cx: &mut gpui::Context<Self>) {
        if self.languages != languages {
            self.languages = languages;
            cx.notify();
        }
    }

    pub fn header_mark(&self) -> Option<Glyph> {
        self.subject
            .as_ref()
            .map(|subject| subject.mark_with_languages(self.languages))
    }
}

/// A picture's rect inside its room, in room coordinates: a raster is never enlarged, a vector fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn scale(room_w: f64, room_h: f64, w: f64, h: f64, vector: bool) -> Option<(f64, f64)> {
    if w.is_nan() || h.is_nan() || w <= 0. || h <= 0. {
        return None;
    }
    // A raster is never enlarged; a vector fills its room along the limiting axis (v0.3.12).
    if !vector && w <= room_w && h <= room_h {
        return Some((w, h));
    }
    // Multiply before dividing, so a whole fit stays whole: 800x600 in 171 rows is 228, never 227.99.
    Some(if room_w * h <= room_h * w {
        (room_w, h * room_w / w)
    } else {
        (w * room_h / h, room_h)
    })
}

/// The interim and RAW picture: the size rounds, then the offset; ql-image 800x600 lands 765x574 at 69,0.
pub fn fit_rounded(room_w: f64, room_h: f64, w: f64, h: f64, vector: bool) -> Option<Fit> {
    let (fw, fh) = scale(room_w, room_h, w, h, vector)?;
    let (w, h) = (js_round(fw), js_round(fh));
    Some(Fit {
        x: js_round((room_w - w) / 2.),
        y: js_round((room_h - h) / 2.),
        w,
        h,
    })
}

/// A raster at its own size, `w`x`h` logical, centred on whole pixels; none once it outgrows its room.
pub fn fit_own(room_w: f64, room_h: f64, w: f64, h: f64) -> Option<Fit> {
    (w > 0. && h > 0. && w <= room_w && h <= room_h).then(|| Fit {
        x: js_round((room_w - w) / 2.),
        y: js_round((room_h - h) / 2.),
        w,
        h,
    })
}

/// The rounded fit for a picture file: a vector path fills its room, a raster path keeps its pixels.
pub fn fit_rounded_path(room_w: f64, room_h: f64, w: f64, h: f64, path: &Path) -> Option<Fit> {
    let vector = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_vector_name);
    fit_rounded(room_w, room_h, w, h, vector)
}

/// The painted fit for a picture file: a vector path fills its room, a raster path keeps its pixels.
pub fn fit_painted_path(room_w: f64, room_h: f64, w: f64, h: f64, path: &Path) -> Option<Fit> {
    let vector = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_vector_name);
    fit_painted(room_w, room_h, w, h, vector)
}

/// A painted aspect fit, whole pixels toward the origin; ql-symlink 800x600 lands 729x547 at 86,0, a column RAW 256x171 at 9,0.
pub fn fit_painted(room_w: f64, room_h: f64, w: f64, h: f64, vector: bool) -> Option<Fit> {
    let (fw, fh) = scale(room_w, room_h, w, h, vector)?;
    let (w, h) = (fw.floor(), fh.floor());
    Some(Fit {
        x: ((room_w - w) / 2.).floor(),
        y: ((room_h - h) / 2.).floor(),
        w,
        h,
    })
}
