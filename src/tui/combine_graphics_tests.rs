//! Protocol output is checked as an actual bitmap and as the numbered fallback.
use super::*;

pub(super) fn run(root: &Path) {
    fixture(root);
    let (mut m, mut w) = open(root);
    let graphics = preview(&mut m, &mut w, Protocol::None);
    assert!(graphics.bytes.is_empty() && graphics.transmit.is_empty());
    let rows = frame(&m);
    let g = combine::geometry(&m);
    for (i, rect) in g.tiles.iter().enumerate() {
        assert_eq!(
            rows[rect.y].chars().nth(rect.x),
            char::from_digit((i + 1) as u32, 10)
        );
        assert_eq!(rows[rect.y + 1].chars().nth(rect.x), Some('│'));
    }
    for protocol in [Protocol::Kitty, Protocol::Sixel] {
        let graphics = preview(&mut m, &mut w, protocol);
        if protocol == Protocol::Kitty {
            assert!(graphics.transmit.starts_with(b"\x1b_Ga=t,f=100"));
            let png = kitty_png(&graphics.transmit);
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            let output = root.join("kitty-preview.png");
            std::fs::write(&output, png).unwrap();
            let g = combine::geometry(&m);
            assert_eq!(
                identify(&output, "%m %w %h"),
                format!(
                    "PNG {} {}",
                    (g.preview.width - 2) * 8,
                    (g.preview.height - 2) * 16
                )
            );
            let tile = m
                .combining
                .card
                .as_ref()
                .unwrap()
                .composition()
                .unwrap()
                .fitted(
                    ((g.preview.width - 2) * 8) as f64,
                    ((g.preview.height - 2) * 16) as f64,
                    0.,
                )
                .tiles[0];
            let pixel = identify(
                &output,
                &format!(
                    "%[pixel:p{{{},{}}}]",
                    (tile.x + tile.width / 2.) as usize,
                    (tile.y + tile.height / 2.) as usize
                ),
            );
            assert!(
                pixel.contains("255,0,0"),
                "native first-source pixels: {pixel}"
            );
        } else {
            assert!(graphics.bytes.starts_with(b"\x1bP") && graphics.bytes.len() > 100);
        }
    }
    for (columns, height) in [(70, 25), (130, 27), (170, 45)] {
        m.columns = columns;
        m.height = height;
        let g = combine::geometry(&m);
        for b in &g.tiles {
            assert!(
                b.x + b.width <= g.preview.x + g.preview.width
                    && b.y + b.height <= g.preview.y + g.preview.height
            );
        }
    }
}

// Kitty chunk sample: ESC_Ga=t,f=100,i=42,q=2,m=0;iVBOR...ESC\.
pub(super) fn kitty_png(bytes: &[u8]) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    text.split("\x1b_G")
        .skip(1)
        .flat_map(|chunk| {
            let payload = chunk
                .split_once(';')
                .unwrap()
                .1
                .split("\x1b\\")
                .next()
                .unwrap();
            crate::tui::graphics::unbase64(payload.as_bytes()).expect("real native PNG chunk")
        })
        .collect()
}
