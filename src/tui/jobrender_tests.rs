use super::*;
use crate::jsondoc::Json;
use std::time::Duration;

#[test]
fn extraction_status_paints_the_board_bar_and_muted_job_count() {
    let mut model = Model::new(std::path::PathBuf::from("/"), &Json::Null);
    model.transfer = "Extracting 7 of 18 · 3 jobs".into();
    model.job_status.fraction = Some(0.4);
    model.job_status.count = 3;
    let theme = Theme::from_text("");
    let output = footer(&model, &theme, 120, Duration::ZERO);
    assert!(output.contains(&format!("{}Extracting 7 of 18", theme.foreground)));
    assert!(output.contains(&format!("{}━━━━{}", theme.accent, theme.base())));
    assert!(output.contains(&format!("{}──────{}", theme.border, theme.base())));
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
