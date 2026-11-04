//! The two backend lines the path bar asks with and the two answers, parsed on the backend reader thread.
use flea::{
    json::{escape, string_array},
    jsondoc::Json,
};

/// A peek's answer for Tab: only directories complete, so only their names are kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Peeked {
    pub path: String,
    pub hidden: bool,
    pub failed: bool,
    pub mode: u32,
    pub dirs: Vec<String>,
}

/// One jump answer: each source's folders that exist now, in that source's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Jumped {
    pub id: u64,
    pub favourites: Vec<String>,
    pub zoxide: Vec<String>,
    pub recent: Vec<String>,
    pub frecency: Vec<(String, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Peeked(Peeked),
    Jumped(Jumped),
}

// Tab completes dotfiles first, so its peek opts out of the listing's hidden-last order.
pub fn peek_line(dir: &str, first: usize, hidden: bool) -> String {
    format!(
        "{{\"c\":\"peek\",\"path\":\"{}\",\"first\":{first},\"hidden\":{hidden},\"hiddenLast\":false}}",
        escape(dir)
    )
}

/// Whether a line the bar sends is a jump ask, the one kind whose reply the window counts.
pub fn is_jump_line(line: &str) -> bool {
    line.starts_with("{\"c\":\"jump\"")
}

// ranking names the provisional ask a whole ask follows, or 0.
pub fn jump_line(id: u64, ranking: u64, favourites: &[String], recent: &[String]) -> String {
    format!(
        "{{\"c\":\"jump\",\"id\":{id},\"ranking\":{ranking},\"favourites\":{},\"recent\":{}}}",
        string_array(favourites),
        string_array(recent)
    )
}

fn strings(document: &Json, key: &str) -> Vec<String> {
    document
        .get(key)
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn number(document: &Json, key: &str) -> Option<u64> {
    match document.get(key) {
        Some(Json::Num(value)) => value.parse().ok(),
        _ => None,
    }
}

// Sample input: {"t":"peeked","path":"/srv","hidden":false,"rows":[{"n":"www","d":true}]}.
pub fn answer(document: &Json) -> Option<Answer> {
    match document.get("t")?.as_str()? {
        "peeked" => Some(Answer::Peeked(Peeked {
            path: document.get("path")?.as_str()?.to_owned(),
            hidden: document
                .get("hidden")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            failed: document
                .get("failed")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            mode: number(document, "mode")
                .and_then(|mode| u32::try_from(mode).ok())
                .unwrap_or(0),
            dirs: document
                .get("rows")
                .and_then(Json::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter(|row| row.get("d").and_then(Json::as_bool) == Some(true))
                        .filter_map(|row| row.get("n")?.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        "jumped" => Some(Answer::Jumped(Jumped {
            id: number(document, "id")?,
            favourites: strings(document, "favourites"),
            zoxide: strings(document, "zoxide"),
            recent: strings(document, "recent"),
            frecency: document
                .get("frecency")
                .and_then(Json::as_object)
                .map(|pairs| {
                    pairs
                        .iter()
                        .filter_map(|(path, score)| Some((path.clone(), score.as_f64()?)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        _ => None,
    }
}
