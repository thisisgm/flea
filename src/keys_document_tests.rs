use super::*;

#[test]
fn bfx_keys_document_paging_resolves_in_every_gui_preset() {
    let cases = [
        ("F", "F", SHIFT, "previewPageForward"),
        ("B", "B", SHIFT, "previewPageBack"),
        ("D", "", CONTROL, "pageDown"),
        ("U", "", CONTROL, "pageUp"),
        ("PageDown", "", 0, "pageDown"),
        ("PageUp", "", 0, "pageUp"),
    ];
    for preset in PRESETS {
        for context in [Context::Preview, Context::Pdf] {
            for (key, text, modifiers, action) in cases {
                assert_eq!(
                    lookup(
                        preset,
                        context,
                        Frontend::Gui,
                        Input {
                            key,
                            text,
                            modifiers
                        }
                    ),
                    Match::Action(action),
                    "{preset} {} {key} modifiers={modifiers}",
                    context.name()
                );
            }
        }
        for (text, action) in [("F", "previewPageForward"), ("B", "previewPageBack")] {
            assert_eq!(
                lookup(
                    preset,
                    Context::Listing,
                    Frontend::Gui,
                    Input {
                        text,
                        modifiers: SHIFT,
                        ..Default::default()
                    }
                ),
                Match::Action(action)
            );
        }
        assert_eq!(
            lookup(
                preset,
                Context::Listing,
                Frontend::Gui,
                Input {
                    key: "D",
                    modifiers: CONTROL,
                    ..Default::default()
                }
            ),
            // The Windows preset binds Ctrl+D to trash in the listing; that row must survive.
            Match::Action(if preset == "windows" {
                "trash"
            } else {
                "pageDown"
            }),
            "{preset} listing Ctrl+D keeps its existing action"
        );
    }
}
