// The converter's argv. The preview's PDF is the document's first pages only, under its own key; Print and Convert keep the whole document under the plain key.
use super::*;
use flea::bytename::ExactText as _;

// Sample output: soffice --headless --norestore --nolockcheck -env:UserInstallation=file:///tmp/flea-office-profile --convert-to pdf --outdir /out /in.docx
#[cfg(test)]
pub fn argv(program: &str, input: &Path, outdir: &Path) -> Vec<String> {
    convert_argv(program, "pdf", input, outdir)
}

pub(super) fn convert_argv(
    program: &str,
    target: &str,
    input: &Path,
    outdir: &Path,
) -> Vec<String> {
    let profile = format!("-env:UserInstallation={PROFILE_URL}");
    let fixed = [
        program,
        "--headless",
        "--norestore",
        "--nolockcheck",
        &profile,
        "--convert-to",
        target,
        "--outdir",
    ];
    let mut a: Vec<String> = fixed.iter().map(|s| s.to_string()).collect();
    a.push(outdir.exact_text().into_owned());
    a.push(input.exact_text().into_owned());
    a
}

// The pages a preview converts; a preview PDF that reaches this many may be cut, and the viewer says so ("30+").
pub const PREVIEW_PAGES: usize = 30;
// The preview PDF's key suffix names the cut, so a different PREVIEW_PAGES never serves an older cut.
pub const HEAD_SUFFIX: &str = ".head30";
const SPREADSHEETS: [&str; 3] = [
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.ms-excel",
];
const PRESENTATIONS: [&str; 3] = [
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.oasis.opendocument.presentation",
    "application/vnd.ms-powerpoint",
];

// A page range is an option of one module's own PDF export filter, so the preview names the filter of the document's group.
pub(super) fn export_filter(name: &str, mime: &Db, aliases: &Aliases) -> &'static str {
    match mime.lookup(name).map(|m| aliases.canonical(m)) {
        Some(m) if SPREADSHEETS.contains(&m) => "calc_pdf_Export",
        Some(m) if PRESENTATIONS.contains(&m) => "impress_pdf_Export",
        _ => "writer_pdf_Export",
    }
}

// Sample output: pdf:writer_pdf_Export:{"PageRange":{"type":"string","value":"1-30"}}
pub(super) fn head_target(filter: &str) -> String {
    format!(r#"pdf:{filter}:{{"PageRange":{{"type":"string","value":"1-{PREVIEW_PAGES}"}}}}"#)
}

// A published preview PDF, told by its own name: <key>.head30.pdf.
pub(super) fn is_head(pdf: &Path) -> bool {
    pdf.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.ends_with(HEAD_SUFFIX))
}

// The cache names prune counts as ours beside the PDFs and failure markers: head<pages>.pdf and head<pages>.txt, any cut.
pub(super) fn head_extension(ext: &str) -> bool {
    ext.strip_prefix("head")
        .and_then(|e| e.split_once('.'))
        .is_some_and(|(n, x)| {
            !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && matches!(x, "pdf" | "txt")
        })
}
