//! Omarchy field headings follow the language; scripts without case keep their ordinary spacing.
pub fn heading(text: &str, language: &str) -> (String, bool) {
    let language = language.split(['-', '_']).next().unwrap_or(language);
    if matches!(
        language,
        "ja" | "zh" | "ko" | "ar" | "fa" | "he" | "ur" | "th"
    ) {
        return (text.into(), false);
    }
    let text = if matches!(language, "tr" | "az") {
        text.replace('i', "İ")
    } else {
        text.into()
    };
    let text = if language == "el" {
        text.chars()
            .map(|c| match c {
                'ά' | 'Ά' => 'Α',
                'έ' | 'Έ' => 'Ε',
                'ή' | 'Ή' => 'Η',
                'ί' | 'Ί' => 'Ι',
                'ό' | 'Ό' => 'Ο',
                'ύ' | 'Ύ' => 'Υ',
                'ώ' | 'Ώ' => 'Ω',
                c => c,
            })
            .collect::<String>()
    } else {
        text
    };
    let has_case = text
        .chars()
        .any(|c| c.to_lowercase().to_string() != c.to_uppercase().to_string());
    (text.to_uppercase(), has_case)
}
