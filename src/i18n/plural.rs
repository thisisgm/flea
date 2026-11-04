// Whole-count CLDR cardinal categories, matching Omarchy's default/i18n/plural.jq.
pub fn category(code: &str, count: i64) -> &'static str {
    let n = count.unsigned_abs();
    let n10 = n % 10;
    let n100 = n % 100;
    let millions = n != 0 && n % 1_000_000 == 0;
    match code {
        "ja" | "ko" | "zh-CN" | "zh-TW" | "th" | "vi" | "id" | "ms" => "other",
        "hi" | "bn" | "fa" | "si" if n <= 1 => "one",
        "hi" | "bn" | "fa" | "si" => "other",
        "fr" | "pt-BR" if n <= 1 => "one",
        "fr" | "pt-BR" | "ca" | "es-MX" | "it" | "pt" if millions => "many",
        "fil" => if [4, 6, 9].contains(&n10) { "other" } else { "one" },
        "is" => if n10 == 1 && n100 != 11 { "one" } else { "other" },
        "cs" | "sk" => match n {
            1 => "one",
            2..=4 => "few",
            _ => "other",
        },
        "pl" if n == 1 => "one",
        "ru" | "uk" | "hr" | "sr" if n10 == 1 && n100 != 11 => "one",
        "pl" | "ru" | "uk" | "hr" | "sr"
            if (2..=4).contains(&n10) && !(12..=14).contains(&n100) => "few",
        "pl" | "ru" | "uk" => "many",
        "hr" | "sr" => "other",
        "sl" => match n100 {
            1 => "one",
            2 => "two",
            3..=4 => "few",
            _ => "other",
        },
        "lt" if (11..=19).contains(&n100) => "other",
        "lt" if n10 == 1 => "one",
        "lt" if n10 >= 2 => "few",
        "lt" => "other",
        "lv" if n10 == 0 || (11..=19).contains(&n100) => "zero",
        "lv" if n10 == 1 => "one",
        "lv" => "other",
        "ro" if n == 1 => "one",
        "ro" if n == 0 || (1..=19).contains(&n100) => "few",
        "ro" => "other",
        "ar" => match n {
            0 => "zero",
            1 => "one",
            2 => "two",
            _ if (3..=10).contains(&n100) => "few",
            _ if n100 >= 11 => "many",
            _ => "other",
        },
        "he" if n == 2 => "two",
        "ga" => match n {
            1 => "one",
            2 => "two",
            3..=6 => "few",
            7..=10 => "many",
            _ => "other",
        },
        _ if n == 1 => "one",
        _ => "other",
    }
}
