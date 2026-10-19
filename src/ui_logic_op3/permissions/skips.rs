//! The several-item skips: the inspect store, the note that names what keeps its mode, and the line after Apply.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip {
    pub path: String,
    pub why: String,
}

/// The several-item inspect store: modes and reasons land in arrival order, in place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Store {
    pub modes: Vec<String>,
    pub reasons: Vec<String>,
    pub skipped: Vec<Skip>,
    pub pending: usize,
}

/// One inspect reply as the store reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inspected {
    Ok { mode: String, reason: String },
    Refused { error: String },
}

impl Store {
    pub fn new(count: usize) -> Self {
        Self {
            modes: vec![String::new(); count],
            reasons: vec![String::new(); count],
            skipped: Vec::new(),
            pending: count,
        }
    }

    /// Answers true once per selection, on the last reply.
    pub fn note(&mut self, at: usize, path: &str, reply: Inspected) -> bool {
        if at >= self.modes.len() {
            return false;
        }
        match reply {
            Inspected::Ok { mode, reason } => {
                self.modes[at] = mode;
                self.reasons[at] = reason;
            }
            Inspected::Refused { error } => {
                let why = if error.is_empty() {
                    FAILED.to_string()
                } else {
                    error
                };
                self.modes[at] = String::new();
                self.reasons[at] = why.clone();
                self.skipped.push(Skip {
                    path: path.to_string(),
                    why,
                });
            }
        }
        self.pending = self.pending.saturating_sub(1);
        self.pending == 0
    }

    /// One note names every row the grid cannot change, reasoned and refused together.
    pub fn note_text(&self, i18n: &I18n, paths: &[String]) -> String {
        let list: Vec<Skip> = paths
            .iter()
            .zip(&self.reasons)
            .filter(|(_, why)| !why.is_empty())
            .map(|(path, why)| Skip {
                path: path.clone(),
                why: why.clone(),
            })
            .collect();
        skip_note(i18n, &list)
    }
}

/// Sample input: one setuid file answers "special.txt keeps its mode because its setuid bit is set.".
pub fn skip_note(i18n: &I18n, skipped: &[Skip]) -> String {
    let Some(first) = skipped.first() else {
        return String::new();
    };
    let kinds: Vec<SkipKind> = skipped.iter().map(|s| skip_kind(&s.why)).collect();
    let same = kinds.iter().all(|k| *k == kinds[0]);
    if skipped.len() == 1 {
        let leaf = leaf_of(&first.path);
        return match (kinds[0], special_label(&first.why)) {
            (SkipKind::Special, Some(special)) => i18n.tr_args(
                "dialogs.file.permissions.keeps_special",
                &[leaf, special.word()],
            ),
            (SkipKind::Owner, _) => i18n.tr_args("dialogs.file.permissions.keeps_owner", &[leaf]),
            _ => closed(&i18n.tr_args("dialogs.file.permissions.keeps_other", &[leaf, &first.why])),
        };
    }
    let names: Vec<&str> = skipped
        .iter()
        .take(SKIP_NAMES_SHOWN)
        .map(|s| leaf_of(&s.path))
        .collect();
    let joined = names.join(", ");
    let shown = if skipped.len() > SKIP_NAMES_SHOWN {
        let more = i18n
            .locale()
            .format_count((skipped.len() - SKIP_NAMES_SHOWN) as i64);
        i18n.tr_args("dialogs.file.permissions.names_more", &[&joined, &more])
    } else {
        joined
    };
    let key = match (same, kinds[0]) {
        (true, SkipKind::Special) => "dialogs.file.permissions.many_special",
        (true, SkipKind::Owner) => "dialogs.file.permissions.many_owner",
        _ => "dialogs.file.permissions.many_other",
    };
    i18n.trn_args(key, skipped.len() as i64, &[&shown])
}

/// The status line after a several-item Apply; the reason was the card note's, said before Apply.
pub fn multi_result(i18n: &I18n, skipped: &[Skip]) -> String {
    match skipped {
        [] => i18n.tr("dialogs.file.permissions.changed"),
        [one] => i18n.tr_args("dialogs.file.permissions.kept_one", &[leaf_of(&one.path)]),
        many => i18n.trn("dialogs.file.permissions.kept_many", many.len() as i64),
    }
}
