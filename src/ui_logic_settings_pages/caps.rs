// Section headings in capitals as each language writes them: Turkish dotted I, Greek capitals without accents.

// Sample input: heading("Τοποθεσίες", "el") is "ΤΟΠΟΘΕΣΙΕΣ"; a script without case comes back as it went in.
pub fn heading(text: &str, language: &str) -> String {
    let base = language.split(['-', '_']).next().unwrap_or("");
    let turkic = matches!(base, "tr" | "az");
    let greek = base == "el";
    let mut out = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            'i' if turkic => out.push('İ'),
            'ı' if turkic => out.push('I'),
            // A lowercase vowel with both marks capitalises to three code points, so it maps to its capital whole.
            'ΐ' if greek => out.push('Ϊ'),
            'ΰ' if greek => out.push('Ϋ'),
            _ if greek => out.extend(letter.to_uppercase().map(unaccented).filter(|mark| *mark != '\u{301}')),
            _ => out.extend(letter.to_uppercase()),
        }
    }
    out
}

// Greek capitals drop the tonos; a vowel whose accent sat over a diaeresis keeps the diaeresis.
fn unaccented(letter: char) -> char {
    match letter {
        'Ά' => 'Α',
        'Έ' => 'Ε',
        'Ή' => 'Η',
        'Ί' => 'Ι',
        'Ό' => 'Ο',
        'Ύ' => 'Υ',
        'Ώ' => 'Ω',
        'ΐ' => 'Ϊ',
        'ΰ' => 'Ϋ',
        other => other,
    }
}

#[cfg(test)]
#[path = "caps_tests.rs"]
mod tests;
