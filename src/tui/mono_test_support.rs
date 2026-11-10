use super::*;

pub(super) const PALETTE: &str = render::cards::tests::PALETTE;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Attribute {
    pub(super) reverse: bool,
    pub(super) underline: bool,
}

// Sample: "\x1b[4;12H\x1b[0;7;38;2;9;9;9mOpen" moves to row 4 column 12, then paints reversed text.
// Replay attributes at the cells actually painted, including resets inside controls.
pub(super) fn attributes(text: &str, columns: usize, lines: usize) -> Vec<Vec<Attribute>> {
    let mut cells = vec![vec![Attribute::default(); columns]; lines];
    let (mut x, mut y, mut ink) = (0, 0, Attribute::default());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.next() == Some('[') {
            let mut args = String::new();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    if code == 'H' {
                        let mut numbers = args
                            .split(';')
                            .map(|s| s.parse::<usize>().unwrap_or(1).saturating_sub(1));
                        y = numbers.next().unwrap_or(0);
                        x = numbers.next().unwrap_or(0);
                    } else if code == 'm' {
                        let codes: Vec<_> = args.split(';').collect();
                        let mut i = 0;
                        while i < codes.len() {
                            match codes[i] {
                                "" | "0" => ink = Attribute::default(),
                                "4" => ink.underline = true,
                                "7" => ink.reverse = true,
                                "24" => ink.underline = false,
                                "27" => ink.reverse = false,
                                "38" | "48" => {
                                    i += if codes.get(i + 1) == Some(&"2") { 4 } else { 2 };
                                }
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                    break;
                }
                args.push(code);
            }
        } else if y < lines && x < columns {
            let width = crate::tui::cells::width(c);
            if width > 0 {
                cells[y][x] = ink;
                x += width;
            }
        }
    }
    cells
}

pub(super) fn frame(m: &Model, theme: &Theme) -> String {
    render::frame(
        m,
        theme,
        &Map::load(),
        m.columns,
        m.height + 4,
        Duration::ZERO,
    )
}

pub(super) fn menu_model() -> Model {
    let mut m = model();
    m.menu = true;
    m.menu_ready = true;
    m.menu_count = 1;
    m.menu_path = m.path.join("notes.txt");
    m.menu_view.formats = vec!["zip".into(), "tar.gz".into()];
    m.menu_view.apps = Some(vec![
        ("first".into(), "First".into(), true),
        ("second".into(), "Second".into(), false),
    ]);
    m.taildrop.peers = ["east", "west"]
        .iter()
        .map(|name| crate::tui::taildrop::Peer {
            id: (*name).into(),
            label: (*name).into(),
            address: "100.64.0.1".into(),
        })
        .collect();
    m
}

pub(super) fn opened(kind: &str) -> Model {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    if kind == "combine" {
        m.selected = [7, 8].into_iter().collect();
        m.rows.get_mut(&7).unwrap().name = "photo.png".into();
        m.rows.get_mut(&8).unwrap().name = "sunset.png".into();
        combine::open(&mut m, &mut w, false).unwrap();
        let f = m.combining.card.as_mut().unwrap();
        f.paths = vec!["/fx/photo.png".into(), "/fx/sunset.png".into()];
        f.sizes = vec![(100, 100), (100, 100)];
        f.focus = 0;
    } else if kind == "convert" {
        m.rows.get_mut(&7).unwrap().name = "photo.png".into();
        convert::open(&mut m, &mut w, false).unwrap();
    } else {
        print::open(&mut m, &mut w, false).unwrap();
        let f = m.printing.card.as_mut().unwrap();
        f.destinations = vec![
            flea::ui_logic_print_convert::printing::Destination {
                name: "First".into(),
                default: true,
            },
            flea::ui_logic_print_convert::printing::Destination {
                name: "Second".into(),
                default: false,
            },
        ];
        f.dropdown = true;
        f.focus = 0;
    }
    wire::finish(w, reader);
    m
}
