use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Comments {
    english: Option<String>,
    translated: HashMap<String, String>,
}

impl Comments {
    // Sample input: <comment xml:lang="ja">PNG 画像</comment><comment>PNG image</comment>.
    pub(super) fn parse(mut text: &str) -> Self {
        let mut result = Self::default();
        while let Some(start) = text.find("<comment") {
            text = &text[start + "<comment".len()..];
            let Some(end) = text.find('>') else { break };
            let tag = &text[..end];
            text = &text[end + 1..];
            if !tag.is_empty() && !tag.starts_with(char::is_whitespace) {
                continue;
            }
            if tag.trim_end().ends_with('/') {
                continue;
            }
            let Some(close) = text.find("</comment>") else {
                break;
            };
            let value = &text[..close];
            if let Some(language) = language(tag) {
                if !value.trim().is_empty() {
                    result
                        .translated
                        .entry(language.replace('_', "-"))
                        .or_insert_with(|| decode(value));
                }
            } else if !tag.contains("xml:lang") && result.english.is_none() {
                result.english = Some(value.into());
            }
            text = &text[close + "</comment>".len()..];
        }
        result
    }

    pub(super) fn select(&self, code: &str) -> Option<String> {
        if code == "en" {
            return self.english.clone();
        }
        self.translation(code).or_else(|| self.english.clone())
    }

    pub(super) fn translation(&self, code: &str) -> Option<String> {
        let code = code.replace('_', "-");
        let tag = match code.as_str() {
            "zh-CN" => "zh-Hans-CN",
            "zh-TW" => "zh-Hant-TW",
            "sr" => "sr-Cyrl",
            "no" => "nb",
            _ => &code,
        };
        self.translated
            .get(&code)
            .or_else(|| self.translated.get(tag))
            .or_else(|| {
                code.split_once('-')
                    .and_then(|(base, _)| self.translated.get(base))
            })
            .cloned()
    }
}

// Sample input: xml:lang="pt-BR" names the comment's locale.
fn language(tag: &str) -> Option<&str> {
    let (_, rest) = tag.split_once("xml:lang")?;
    let value = rest.trim_start().strip_prefix('=')?.trim_start();
    let quote = value.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    value[1..].split_once(quote).map(|(language, _)| language)
}

// Sample input: "image &amp; drawing" becomes "image & drawing".
fn decode(mut text: &str) -> String {
    let mut result = String::new();
    while let Some(start) = text.find('&') {
        result.push_str(&text[..start]);
        text = &text[start..];
        let Some(end) = text.find(';') else { break };
        let entity = &text[1..end];
        let character = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|n| u32::from_str_radix(n, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                .and_then(char::from_u32),
        };
        match character {
            Some(c) => result.push(c),
            None => result.push_str(&text[..=end]),
        }
        text = &text[end + 1..];
    }
    result.push_str(text);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_lang_comments_select_exact_then_base_then_english() {
        let comments = Comments::parse(concat!(
            "<comment xml:lang='fr'>image &amp; dessin</comment>",
            "<comment xml:lang='pt'>imagem</comment><comment xml:lang='pt-BR'>imagem BR</comment>",
            "<comment xml:lang='zh-Hant-TW'>PNG &#x5716;&#20687;</comment>",
            "<comment xml:lang='zh-Hans-CN'>PNG 图像</comment>",
            "<comment xml:lang='sr-Cyrl'>ПНГ слика</comment><comment>PNG &amp; image</comment>",
        ));
        for (code, expected) in [
            ("fr", "image & dessin"),
            ("pt-BR", "imagem BR"),
            ("pt_BR", "imagem BR"),
            ("pt-PT", "imagem"),
            ("zh-TW", "PNG 圖像"),
            ("zh-CN", "PNG 图像"),
            ("sr", "ПНГ слика"),
            ("en", "PNG &amp; image"),
            ("fil", "PNG &amp; image"),
        ] {
            assert_eq!(comments.select(code).as_deref(), Some(expected), "{code}");
        }
    }

    #[test]
    fn kind_lang_empty_missing_and_unrelated_comments_do_not_replace_english() {
        let comments = Comments::parse("<commentary>wrong</commentary><comment xml:lang='ja'> </comment><comment>Data</comment>");
        assert_eq!(comments.select("ja").as_deref(), Some("Data"));
        assert_eq!(Comments::parse("").select("ja"), None);
        assert_eq!(
            Comments::parse("<comment xml:lang='ja'>画像</comment>").select("en"),
            None
        );
    }

    #[test]
    fn kind_lang_self_closing_translation_keeps_untranslated_fallback() {
        let comments = Comments::parse("<comment xml:lang='ja'/><comment>PNG image</comment>");
        assert_eq!(comments.select("en").as_deref(), Some("PNG image"));
        assert_eq!(comments.select("ja").as_deref(), Some("PNG image"));
    }
}
