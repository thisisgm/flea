//! Pure rules of the document previews: PDF page layout and timing, Quick Look's PDF controls, CSV and TSV records,
//! code runs and the Markdown block model; the TUI can share them.
pub mod code;
pub mod csv;
pub mod markdown;
pub mod pdf;
pub mod viewer;
// The web part: the code view, HTML, font and archive pages, and the refusal classes they share.
pub mod archive;
pub mod code_text;
pub mod code_view;
pub mod contrast;
pub mod font;
pub mod html;
pub mod refusal;
