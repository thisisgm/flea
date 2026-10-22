//! A code reply's runs: the ink role each run draws in and its text with every tab filled to its stop.

/// The ink names after the foreground, in the order the code reply numbers its roles: 1 to 6 are bat's SGR 31 to 36.
pub const INKS: [&str; 6] = ["red", "green", "yellow", "blue", "magenta", "cyan"];
/// A tab fills to the next multiple of this many columns.
pub const TAB: usize = 8;

/// A role's ink name; 0 and any unknown role are the foreground.
pub fn role_name(role: i64) -> &'static str {
    if (1..=INKS.len() as i64).contains(&role) {
        INKS[role as usize - 1]
    } else {
        "foreground"
    }
}

/// The reply's runs as (role, text) pairs with every tab filled to its stop, counting the columns of the runs before it.
pub fn pairs(runs: &[(i64, &str)]) -> Vec<(i64, String)> {
    let mut column = 0;
    runs.iter()
        .map(|&(role, raw)| {
            let mut text = String::with_capacity(raw.len());
            for c in raw.chars() {
                if c == '\t' {
                    let fill = TAB - column % TAB;
                    text.extend(std::iter::repeat(' ').take(fill));
                    column += fill;
                } else {
                    text.push(c);
                    column += 1;
                }
            }
            (role, text)
        })
        .collect()
}

/// The line's text alone.
pub fn plain(runs: &[(i64, &str)]) -> String {
    pairs(runs).into_iter().map(|(_, text)| text).collect()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
