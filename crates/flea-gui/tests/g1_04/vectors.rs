use super::*;
use std::io::Write;

#[test]
fn export_oracle_vectors_for_all_presets_contexts_frontends_and_masks() {
    let root = root();
    let output =
        std::env::var_os("G1_04_VECTORS").map_or_else(|| root.join("vectors.jsonl"), PathBuf::from);
    let parent = output.parent().unwrap();
    assert!(output.is_absolute() && parent.join(".flea-test-sandbox").is_file());
    assert!(parent
        .canonicalize()
        .unwrap()
        .starts_with(scratch().canonicalize().unwrap()));
    let mut file = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let mut samples: Vec<_> = keys::PRESET_KEYS
        .iter()
        .chain(keys::SHARED_KEYS)
        .map(|row| {
            (
                if row.mods == "text" { "" } else { row.key },
                if row.mods == "text" { row.key } else { "" },
            )
        })
        .collect();
    samples.extend(keys::DIGITS.iter().map(|row| ("", row.text)));
    samples.extend([
        ("G", "g"),
        ("G", "G"),
        ("J", "\u{0436}"),
        ("Q", "q"),
        ("F3", ""),
        ("", "\u{754c}"),
    ]);
    samples.sort_unstable();
    samples.dedup();
    let mut count = 0;
    let quote = |value: &str| format!("\"{}\"", flea::json::escape(value));
    for preset in keys::PRESETS {
        for context in Context::ALL {
            for frontend in [Frontend::Gui, Frontend::Tui] {
                for &(key, text) in &samples {
                    for modifiers in 0..16 {
                        let action = keys::lookup(
                            preset,
                            context,
                            frontend,
                            Input {
                                key,
                                text,
                                modifiers,
                            },
                        )
                        .action();
                        writeln!(file, "{{\"kind\":\"key\",\"preset\":{},\"context\":{},\"frontend\":{},\"key\":{},\"text\":{},\"mods\":{},\"action\":{}}}",
                            quote(preset), quote(context.name()), quote(frontend.name()), quote(key), quote(text), modifiers, quote(action)).unwrap();
                        count += 1;
                    }
                }
                if context == Context::Listing {
                    let sheet = keys::presentation::sheet(
                        preset,
                        frontend,
                        &crate::test_catalogs::english(),
                    );
                    for (index, row) in sheet.iter().enumerate() {
                        writeln!(file, "{{\"kind\":\"sheet\",\"preset\":{},\"frontend\":{},\"index\":{},\"action\":{},\"keys\":{},\"context\":{}}}",
                            quote(preset), quote(frontend.name()), index, quote(row.action), quote(&row.keys), quote(row.context)).unwrap();
                    }
                    writeln!(
                        file,
                        "{{\"kind\":\"sheetend\",\"preset\":{},\"frontend\":{},\"count\":{}}}",
                        quote(preset),
                        quote(frontend.name()),
                        sheet.len()
                    )
                    .unwrap();
                }
            }
        }
        for group in keys::SHEET_GROUPS {
            for action in group.actions {
                writeln!(
                    file,
                    "{{\"kind\":\"hint\",\"preset\":{},\"action\":{},\"hint\":{}}}",
                    quote(preset),
                    quote(action),
                    quote(keys::presentation::hint_for(preset, action))
                )
                .unwrap();
            }
        }
    }
    assert!(count > 50_000, "empty or incomplete vector matrix: {count}");
    println!("G1_04 vectors: {count} key decisions plus all sheet and hint rows");
}
