use super::*;
use crate::tui::layout::footer;
use crate::jsondoc::Json;
use std::time::Duration;

#[test]
fn extraction_status_paints_the_board_bar_and_muted_job_count() {
    let mut model = Model::new(std::path::PathBuf::from("/"), &Json::Null);
    model.transfer = "Extracting 7 of 18 · 3 jobs".into();
    model.job_status.percent = Some(40);
    model.job_status.id = 12;
    model.job_status.count = 3;
    let theme = Theme::from_text("");
    let output = footer(&model, &theme, 120, Duration::ZERO);
    assert!(output.contains(&format!("{}Extracting 7 of 18", theme.foreground)));
    assert!(output.contains(&format!("{}{}{}", theme.accent, "━".repeat(8), theme.base())));
    assert!(output.contains(&format!("{}{}{}", theme.border, "─".repeat(12), theme.base())));
    assert!(output.contains(&format!("{}40%", theme.meta)));
    assert!(output.contains(&format!("{}· 3 jobs", theme.muted)));
    assert!(!output.contains('░'));
    assert_eq!(text_width(&strip_ansi(&output)), 120);
}

#[test]
fn extraction_unknown_total_remains_indeterminate_without_a_percentage() {
    let mut model = Model::new(std::path::PathBuf::from("/"), &Json::Null);
    model.transfer = "Extracting 7 items".into();
    let output = footer(&model, &Theme::from_text(""), 120, Duration::ZERO);
    assert!(output.contains("Extracting 7 items"));
    assert!(!output.contains('%'));
}

#[test]
fn a_job_without_bytes_never_shows_the_last_legacy_percent() {
    let mut model = Model::new(std::path::PathBuf::from("/"), &Json::Null);
    model.transfer = "Extracting 7 items".into();
    model.progress_percent = Some(57);
    model.job_status.id = 12;
    let output = footer(&model, &Theme::from_text(""), 120, Duration::ZERO);
    assert!(output.contains("Extracting 7 items") && !output.contains('%'), "{output:?}");
    model.job_status.id = 0;
    let output = footer(&model, &Theme::from_text(""), 120, Duration::ZERO);
    assert!(output.contains("57%"), "{output:?}");
}

fn strip_ansi(text: &str) -> String {
    let mut escaped = false;
    text.chars()
        .filter(|&c| {
            if c == '\x1b' {
                escaped = true;
                false
            } else if escaped {
                if c == 'm' {
                    escaped = false;
                }
                false
            } else {
                true
            }
        })
        .collect()
}
