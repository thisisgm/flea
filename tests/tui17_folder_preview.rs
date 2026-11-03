use std::process::Command;

fn run(mode: &str) {
    let output = Command::new("python3")
        .args([
            "-B",
            "-c",
            include_str!("tui17_folder_preview.py"),
            env!("CARGO_BIN_EXE_flea"),
            mode,
        ])
        .env("PYTHONPATH", concat!(env!("CARGO_MANIFEST_DIR"), "/tests"))
        .output()
        .unwrap();
    print!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn folder_preview_protocol_carries_returned_child_sizes_and_modes() {
    run("protocol");
}
#[test]
fn folder_preview_actual_tui_shows_the_three_file_sizes_at_130_columns() {
    run("tui");
}

#[test]
fn unsupported_document_actual_tui_draws_the_alert_sentence_and_open_with() {
    run("unsupported");
}
#[test]
fn filter_typing_actual_tui_keeps_match_backgrounds_and_uses_bold() {
    run("filter");
}

#[test]
fn new_find_and_held_copy_replies_survive_the_real_backend_wire() {
    run("parity_protocol");
}
#[test]
fn properties_actual_tui_uses_the_mime_description() {
    run("properties");
}
#[test]
fn binary_file_actual_tui_uses_the_binary_glyph_in_both_panes() {
    run("blob");
}
