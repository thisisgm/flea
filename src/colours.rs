//! Dependency-free reader for the Omarchy colours.toml palette, shared by the TUI and the GUI.

// A colour value is '#' then six hex digits, so any digits after the first six are ignored.
const COLOUR_BYTES: usize = 7;

// Sample input: accent = "#a9b665"; a value shorter than '#' plus six hex digits is skipped.
pub fn entries(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value
                .trim()
                .trim_start_matches(['"', '\''])
                .get(..COLOUR_BYTES)?;
            let hex = value.strip_prefix('#')?;
            hex.bytes()
                .all(|byte| byte.is_ascii_hexdigit())
                .then_some((key.trim(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::entries;

    #[test]
    fn quotes_width_and_order_follow_the_palette_rules() {
        let text = "  a = '#112233'\nb=\"#abcdef\"\nc=#445566ff\nd='#12345'\ne='#zz0000'\nf='oops'\na='#010203'";
        assert_eq!(
            entries(text),
            vec![
                ("a", "#112233"),
                ("b", "#abcdef"),
                ("c", "#445566"),
                ("a", "#010203")
            ]
        );
    }
}
