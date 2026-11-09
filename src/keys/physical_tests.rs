use super::*;

fn sample(key: &str, us: Option<&str>, text: &str, shifted: &str) -> Sample {
    Sample {
        key: key.into(),
        us: us.map(String::from),
        text: (!text.is_empty()).then(|| text.into()),
        shifted: (!shifted.is_empty()).then(|| shifted.into()),
    }
}

/// A slice of the Swedish group as GPUI samples it: å on the US `[`, the extra ISO key, a dead key, named keys.
fn swedish() -> Vec<Sample> {
    vec![
        sample("escape", None, "", ""),
        sample("1", Some("1"), "1", "!"),
        sample("+", Some("-"), "+", "?"),
        sample("=", Some("="), "", ""),
        sample("q", Some("q"), "q", "Q"),
        sample("å", Some("["), "å", "Å"),
        sample("'", Some("\\"), "'", "*"),
        sample(",", Some(","), ",", ";"),
        sample("alt_l", None, "", ""),
        sample("iso_level3_shift", None, "", ""),
        sample("<", None, "<", ">"),
        sample("f2", None, "", ""),
        sample("enter", None, "", ""),
    ]
}

/// The US group: xkb's `pc` symbols still give it the key beside left Shift, typing what Shift and comma type.
fn us() -> Vec<Sample> {
    vec![
        sample(",", Some(","), ",", "<"),
        sample("alt_l", None, "", ""),
        sample("<", None, "<", ">"),
        sample("alt_r", None, "", ""),
    ]
}

fn cap(caps: &[Cap], id: &str) -> Cap {
    *caps
        .iter()
        .find(|cap| cap.id == id)
        .expect("a cap the board draws")
}

#[test]
fn a_key_of_its_own_beside_left_shift_makes_an_iso_board() {
    assert!(is_iso(&swedish()));
    assert!(!is_iso(&us()), "US types < on Shift and comma already");
    let without: Vec<_> = swedish().into_iter().filter(|s| s.key != "<").collect();
    assert!(!is_iso(&without));
    let iso = caps(true);
    assert!(iso.iter().any(|cap| cap.id == "LSGT"));
    assert_eq!(iso.iter().filter(|cap| cap.id == "RTRN").count(), 2);
    let ansi = caps(false);
    assert!(!ansi.iter().any(|cap| cap.id == "LSGT"));
    assert_eq!(cap(&ansi, "BKSL").row, 2);
}

#[test]
fn right_alt_is_alt_only_where_the_group_names_two_alts() {
    assert!(right_alt_is_alt(&us()));
    assert!(!right_alt_is_alt(&swedish()));
}

#[test]
fn every_row_of_the_main_block_ends_at_fifteen_units() {
    for iso in [true, false] {
        let caps = caps(iso);
        for row in 0..ROWS {
            let end = caps
                .iter()
                .filter(|cap| cap.row == row && cap.x < 15.)
                .map(|cap| cap.x + cap.w)
                .fold(0., f32::max);
            assert_eq!(end, 15., "row {row}, iso {iso}");
        }
        let widest = caps.iter().map(|cap| cap.x + cap.w).fold(0., f32::max);
        assert_eq!(widest, WIDTH);
    }
}

#[test]
fn caps_never_overlap_on_a_row() {
    for iso in [true, false] {
        let caps = caps(iso);
        for a in &caps {
            for b in &caps {
                if std::ptr::eq(a, b) || a.row != b.row {
                    continue;
                }
                assert!(
                    a.x + a.w <= b.x + 1e-4 || b.x + b.w <= a.x + 1e-4,
                    "{} and {}",
                    a.id,
                    b.id
                );
            }
        }
    }
}

#[test]
fn the_navigation_block_carries_every_paging_key() {
    let caps = caps(true);
    for (id, key) in [
        ("INS", "insert"),
        ("HOME", "home"),
        ("PGUP", "pageup"),
        ("DELE", "delete"),
        ("END", "end"),
        ("PGDN", "pagedown"),
    ] {
        assert_eq!(cap(&caps, id).key, key);
    }
}

#[test]
fn printing_caps_take_the_layouts_legends_by_position() {
    let samples = swedish();
    let caps = caps(true);
    let legend = |id: &str| legends(&cap(&caps, id), &samples);
    assert_eq!(legend("AD11"), ("Å".into(), String::new()));
    assert_eq!(legend("AD01"), ("Q".into(), String::new()));
    assert_eq!(legend("AE01"), ("1".into(), "!".into()));
    assert_eq!(legend("AE11"), ("+".into(), "?".into()));
    assert_eq!(legend("BKSL"), ("'".into(), "*".into()));
    assert_eq!(legend("LSGT"), ("<".into(), ">".into()));
    // A dead key types nothing alone, and a position the group leaves empty, both draw blank rather than a guess.
    assert_eq!(legend("AE12"), (String::new(), String::new()));
    assert_eq!(legend("AB10"), (String::new(), String::new()));
}

#[test]
fn named_caps_read_their_names_and_find_their_samples() {
    let samples = swedish();
    let caps = caps(true);
    assert_eq!(legends(&cap(&caps, "FK02"), &samples).0, "F2");
    assert_eq!(sample_of(&cap(&caps, "FK02"), &samples), Some(11));
    assert_eq!(sample_of(&cap(&caps, "ESC"), &samples), Some(0));
    assert_eq!(sample_of(&cap(&caps, "CAPS"), &samples), None);
}

#[test]
fn modifier_caps_name_their_bits_and_bind_nothing() {
    let caps = caps(true);
    assert_eq!(modifier(&cap(&caps, "LCTL")), crate::keys::CONTROL);
    assert_eq!(modifier(&cap(&caps, "RTSH")), crate::keys::SHIFT);
    assert_eq!(modifier(&cap(&caps, "RWIN")), crate::keys::SUPER);
    assert_eq!(modifier(&cap(&caps, "RALT")), 0);
    assert!(inert(&cap(&caps, "RALT")));
    assert!(inert(&cap(&caps, "PAUS")));
    assert!(!inert(&cap(&caps, "AD01")));
}
