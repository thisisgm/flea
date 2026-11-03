//! Review regressions drive actual key routing, painted cells and sandboxed decoders.
use super::super::input::Key;
use super::*;
use std::os::unix::fs::PermissionsExt;

pub(super) fn navigation(root: &Path) {
    fixture(root);
    let (mut m, mut w) = open(root);
    preview(&mut m, &mut w, Protocol::None);
    choose(&mut m, &mut w, 1, 2);
    let tile = combine::geometry(&m).tiles[1];
    pointer(&mut m, &mut w, tile.x + 1, tile.y + 1, false, false);
    assert_eq!(m.combining.card.as_ref().unwrap().image, 1);
    for key in [Key::named("Up", ""), Key::character('k', "")] {
        press(&mut m, &mut w, key);
        assert_eq!(
            m.combining.card.as_ref().unwrap().image,
            1,
            "a missing preceding row must preserve the focused source"
        );
    }
    press(&mut m, &mut w, Key::character('[', ""));
    assert!(m.combining.card.as_ref().unwrap().paths[0].ends_with("b.png"));
    assert_eq!(m.combining.card.as_ref().unwrap().sizes[0], (40, 80));
}

fn numeral(m: &Model, digit: char) -> (usize, usize) {
    let g = combine::geometry(m);
    let cells: Vec<_> = frame(m)
        .iter()
        .enumerate()
        .filter(|(y, _)| (g.preview.y..g.preview.y + g.preview.height).contains(y))
        .flat_map(|(y, line)| {
            line.chars().enumerate().filter_map(move |(x, c)| {
                (c == digit && (g.preview.x..g.preview.x + g.preview.width).contains(&x))
                    .then_some((x + 1, y + 1))
            })
        })
        .collect();
    assert_eq!(cells.len(), 1, "one painted source numeral {digit}");
    cells[0]
}

pub(super) fn pointer_targets(root: &Path) {
    fixture(root);
    let (mut m, mut w) = open(root);
    preview(&mut m, &mut w, Protocol::None);
    choose(&mut m, &mut w, 4, 0);
    let second = numeral(&m, '2');
    pointer(&mut m, &mut w, second.0, second.1, false, false);
    assert_eq!(
        m.combining.card.as_ref().unwrap().image,
        1,
        "clicking the visible second numeral must select its source"
    );
    let third = numeral(&m, '3');
    pointer(&mut m, &mut w, third.0, third.1, true, false);
    pointer(&mut m, &mut w, third.0, third.1, false, true);
    let paths = &m.combining.card.as_ref().unwrap().paths;
    assert!(paths[0].ends_with("a.png"));
    assert!(paths[1].ends_with("c.png"));
    assert!(paths[2].ends_with("b.png"));
}

pub(super) fn short_terminal(root: &Path) {
    fixture(root);
    for (columns, lines) in [(80, 17), (130, 16)] {
        let (mut m, mut w) = open(root);
        preview(&mut m, &mut w, Protocol::None);
        m.columns = columns;
        m.resize_body(super::super::render::body_height(lines));
        for _ in 0..12 {
            if m.combining.card.as_ref().unwrap().focus == 6 {
                break;
            }
            press(&mut m, &mut w, Key::named("Tab", ""));
        }
        assert_eq!(m.combining.card.as_ref().unwrap().focus, 6);
        let name = m.combining.card.as_ref().unwrap().name.clone();
        let painted = frame(&m).join("\n");
        assert!(
            painted.contains("NAME"),
            "focused Name label must remain visible"
        );
        assert!(
            painted.contains(&name),
            "focused filename must remain visible"
        );
        for c in "review.png".chars() {
            press(&mut m, &mut w, Key::character(c, ""));
        }
        assert!(frame(&m).join("\n").contains("review.png"));
        m.resize_body(super::super::render::body_height(31));
        assert!(frame(&m).join("\n").contains("review.png"));
    }
}

pub(super) fn oversized_columns(root: &Path) {
    fixture(root);
    let (mut m, mut w) = open(root);
    preview(&mut m, &mut w, Protocol::None);
    choose(&mut m, &mut w, 1, 2);
    choose(&mut m, &mut w, 2, 0);
    let columns = (isize::MAX as usize + 1).to_string();
    for c in columns.chars() {
        press(&mut m, &mut w, Key::character(c, ""));
    }
    assert_eq!(m.combining.card.as_ref().unwrap().columns, columns);
    for _ in 0..12 {
        if m.combining.card.as_ref().unwrap().focus == 0 {
            break;
        }
        press(&mut m, &mut w, Key::named("Tab", ""));
    }
    assert_eq!(m.combining.card.as_ref().unwrap().focus, 0);
    let paths = m.combining.card.as_ref().unwrap().paths.clone();
    press(&mut m, &mut w, Key::named("Up", ""));
    assert_eq!(m.combining.card.as_ref().unwrap().paths, paths);
    press(&mut m, &mut w, Key::named("Return", ""));
    assert!(m.combining.run.is_none());
    assert!(!m.combining.card.as_ref().unwrap().error.is_empty());
}

pub(super) fn source_revision(root: &Path) {
    fixture(root);
    let replacement = root.join("replacement.png");
    tool(&[
        "magick",
        "-size",
        "40x80",
        "xc:cyan",
        replacement.to_str().unwrap(),
    ]);
    let (mut m, mut w) = open(root);
    let wrapper = root.join("bin/bwrap");
    let script = r#"#!/bin/sh
set -eu
root=${FLEA_TUI_COMBINE_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
set +e
/usr/bin/bwrap "$@"
status=$?
set -e
if test -f "$root/replacement.png"; then
  mv "$root/replacement.png" "$root/files/a.png"
  printf 'source replaced after first held decoder\n' > "$root/replaced"
fi
exit "$status"
"#;
    std::fs::write(&wrapper, script).unwrap();
    std::fs::set_permissions(wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let graphics = preview(&mut m, &mut w, Protocol::Kitty);
    assert!(
        root.join("replaced").is_file(),
        "the controlled revision race must execute"
    );
    let output = root.join("revision-preview.png");
    std::fs::write(&output, graphics_tests::kitty_png(&graphics.transmit)).unwrap();
    let f = m.combining.card.as_ref().unwrap();
    let g = combine::geometry(&m);
    let tile = f
        .composition()
        .unwrap()
        .fitted(
            ((g.preview.width - 2) * 8) as f64,
            ((g.preview.height - 2) * 16) as f64,
            0.,
        )
        .tiles[0];
    let color = tool(&[
        "magick",
        output.to_str().unwrap(),
        "-crop",
        &format!(
            "1x1+{}+{}",
            (tile.x + tile.width / 2.) as usize,
            (tile.y + tile.height / 2.) as usize
        ),
        "+repage",
        "-depth",
        "8",
        "rgb:-",
    ]);
    assert!(
        (f.sizes[0] == (100, 60) && color == [255, 0, 0])
            || (f.sizes[0] == (40, 80) && color == [0, 255, 255]),
        "dimensions and actual native pixels must describe one revision: {:?}, {color:?}",
        f.sizes[0]
    );
}
