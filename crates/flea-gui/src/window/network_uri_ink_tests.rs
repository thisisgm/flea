//! The Add a Network Share card's Mounts as line: the typed share, with the address in the foreground after the muted words.
use super::*;

#[gpui::test]
fn fix_nettype_mounts_as_line_reads_the_typed_share(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    cx.simulate_input("nas");
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("media");
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("gm");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(view.model.form.uri().as_deref(), Some("smb://gm@nas/media"));
        let drawn = view.drawn.borrow();
        let line: Vec<_> = drawn
            .text
            .iter()
            .filter(|text| text.field.is_none() && text.rect.x == 17. && text.rect.y == 523.)
            .map(|text| (text.source.to_string(), text.ink))
            .collect();
        let p = view.palette;
        assert_eq!(
            line,
            [
                ("Mounts as smb://gm@nas/media".to_owned(), p.muted),
                ("smb://gm@nas/media".to_owned(), p.foreground),
            ],
            "the Mounts as line under the fields carries the typed share"
        );
        // board network-typed: the words stay muted and the address draws in the foreground, cut from one shaped line.
        let glyphs = |at: usize| {
            drawn
                .text
                .iter()
                .filter(|text| text.rect.y == 523.)
                .nth(at)
                .map(|text| {
                    text.run
                        .line()
                        .runs
                        .iter()
                        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index))
                        .collect::<Vec<_>>()
                })
        };
        let (words, address) = (glyphs(0).unwrap(), glyphs(1).unwrap());
        assert!(!words.is_empty() && words.iter().all(|&at| at < "Mounts as ".len()));
        assert!(!address.is_empty() && address.iter().all(|&at| at >= "Mounts as ".len()));
    });
}
