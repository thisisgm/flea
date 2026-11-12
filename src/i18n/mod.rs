//! Shared catalogs and locale formatting; callers drive poll_changed from their own event loop.
mod catalog;
mod document;
mod format;
mod locale;
pub mod plural;

pub use catalog::substitute;
pub use locale::Locale;
use catalog::Catalog;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub omarchy: PathBuf,
    pub language_file: PathBuf,
    // Highest priority first; a partial catalog can fall back through the remaining roots.
    pub catalogs: Vec<PathBuf>,
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn catalog_dirs(override_dir: Option<PathBuf>, exe: Option<&Path>, source: &Path, debug: bool) -> Vec<PathBuf> {
    let mut catalogs: Vec<PathBuf> = override_dir.into_iter().collect();
    if let Some(prefix) = exe.and_then(Path::parent).and_then(Path::parent) {
        catalogs.push(prefix.join("share/flea/i18n"));
    }
    catalogs.extend([PathBuf::from("/usr/share/flea/i18n"), PathBuf::from("/usr/local/share/flea/i18n")]);
    if debug || exe.is_some_and(|path| path.starts_with(source.join("target"))) {
        catalogs.push(source.join("i18n"));
    }
    catalogs.dedup();
    catalogs
}

// Sample input: "de\n" selects de; only the first line of Omarchy's language file is used.
fn language_code(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

impl Default for Paths {
    fn default() -> Self {
        let config = env_path("XDG_CONFIG_HOME").or_else(|| env_path("HOME").map(|p| p.join(".config")));
        let language_file = env_path("OMARCHY_LANGUAGE_FILE").unwrap_or_else(|| {
            config.unwrap_or_else(|| PathBuf::from("/nonexistent")).join("omarchy/language")
        });
        let exe = std::env::current_exe().ok();
        let catalogs = catalog_dirs(env_path("FLEA_I18N"), exe.as_deref(), Path::new(env!("CARGO_MANIFEST_DIR")), cfg!(debug_assertions));
        Self {
            omarchy: env_path("OMARCHY_PATH").unwrap_or_else(|| PathBuf::from("/usr/share/omarchy")),
            language_file, catalogs,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    device: u64, inode: u64, length: u64,
    modified: (i64, i64), changed: (i64, i64),
}

fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some(Stamp {
        device: m.dev(), inode: m.ino(), length: m.len(),
        modified: (m.mtime(), m.mtime_nsec()), changed: (m.ctime(), m.ctime_nsec()),
    })
}

// Omarchy's English word for each omarchy: reference in en/flea.json, drawn when Omarchy ships no catalog for it (4.0.x has no shell strings).
const OMARCHY_ENGLISH: [(&str, &str); 6] = [
    ("shell.common.cancel", "Cancel"),
    ("shell.common.apply", "Apply"),
    ("shell.network.passphrase", "Passphrase"),
    ("shell.dock.open", "Open"),
    ("keybindings.Network", "Network"),
    ("keybindings.Mute", "Mute"),
];

#[derive(Clone)]
pub struct I18n {
    paths: Paths,
    roster: Vec<Locale>,
    locale: Locale,
    english: Catalog,
    translated: Catalog,
    omarchy_english: HashMap<&'static str, Catalog>,
    omarchy_translated: HashMap<&'static str, Catalog>,
    watched: Vec<(PathBuf, Option<Stamp>)>,
    language_text: Option<String>,
    paragraphs: bool,
}

impl Default for I18n {
    fn default() -> Self { Self::load(Paths::default()) }
}

impl I18n {
    pub fn load(paths: Paths) -> Self {
        let mut result = Self {
            paths, roster: Vec::new(), locale: Locale::default(),
            english: Catalog::new(), translated: Catalog::new(),
            omarchy_english: HashMap::new(), omarchy_translated: HashMap::new(), watched: Vec::new(),
            paragraphs: false,
            language_text: None,
        };
        result.reload();
        result
    }

    pub fn language(&self) -> &str { &self.locale.code }
    pub fn locale(&self) -> &Locale { &self.locale }
    pub fn roster(&self) -> &[Locale] { &self.roster }

    /// Every distinct character the roster's own names and this language's catalogs hold, so a font snapshot can resolve each script before it draws.
    pub fn held_text(&self) -> String {
        use crate::jsondoc::Json;
        use std::collections::BTreeSet;
        fn walk(json: &Json, chars: &mut BTreeSet<char>) {
            match json {
                Json::Str(text) => chars.extend(text.chars()),
                Json::Arr(items) => items.iter().for_each(|item| walk(item, chars)),
                Json::Obj(members) => members.iter().for_each(|(_, item)| walk(item, chars)),
                _ => {}
            }
        }
        let mut chars = BTreeSet::new();
        for locale in &self.roster {
            chars.extend(locale.name.chars());
            // Numbers shift every ASCII digit from zero_digit, so all ten are drawn, beside the decimal and group marks.
            chars.extend((0..10).filter_map(|n| char::from_u32(locale.zero_digit as u32 + n)));
            chars.extend(locale.decimal.chars().chain(locale.separator.chars()));
        }
        for catalog in std::iter::once(&self.translated).chain(self.omarchy_translated.values()) {
            catalog.values().for_each(|value| walk(value, &mut chars));
        }
        chars.into_iter().collect()
    }

    pub fn tr(&self, key: &str) -> String {
        let line = self.resolve(key, None).unwrap_or(key);
        self.paragraph(line, line.into())
    }

    /// The GUI shapes each line as one bidi paragraph and asks for the locale's direction; the TUI never does.
    pub fn mark_paragraphs(&mut self) { self.paragraphs = true; }

    // An RTL line opening in Latin outside its placeholders ("Flea نے %1") reads right to left, as an RLM sets.
    fn paragraph(&self, template: &str, line: String) -> String {
        if self.paragraphs && self.locale.right_to_left && opens_latin(template) { format!("\u{200f}{line}") } else { line }
    }

    // MIME owns descriptions unless this language explicitly overrides one; no English fallback here.
    pub fn mime_comment(&self, mime: &str) -> Option<&str> {
        let key = match mime { "text/markdown" => "mime.markdown", "image/png" => "mime.png", _ => return None };
        self.translated.get(key).and_then(crate::jsondoc::Json::as_str).filter(|value| !value.trim().is_empty())
    }

    pub fn tr_args(&self, key: &str, args: &[&str]) -> String {
        substitute(&self.tr(key), args)
    }

    // A key keeps Omarchy's word; RTL text isolates it LRI..PDI (i18n/rtl-policy.json). The TUI binary's copy never calls it: terminals do no bidi.
    #[allow(dead_code)]
    pub fn keycap(&self, keys: &str) -> String {
        if keys.is_empty() || !self.locale.right_to_left {
            return keys.into();
        }
        format!("\u{2066}{keys}\u{2069}")
    }

    // Count supplies %1; additional arguments start at %2.
    pub fn trn(&self, key: &str, count: i64) -> String { self.trn_args(key, count, &[]) }

    pub fn trn_args(&self, key: &str, count: i64, args: &[&str]) -> String {
        let number = self.locale.format_count(count);
        let mut values = vec![number.as_str()];
        values.extend_from_slice(args);
        let line = self.resolve(key, Some(count)).unwrap_or(key);
        self.paragraph(line, substitute(line, &values))
    }

    fn resolve<'a>(&'a self, key: &'a str, count: Option<i64>) -> Option<&'a str> {
        if let Some(reference) = key.strip_prefix("omarchy:") {
            return self.omarchy_word(reference, count);
        }
        let value = catalog::lookup(&self.english, &self.translated, key, self.language(), count)?;
        match value.strip_prefix("omarchy:") {
            Some(reference) => self.omarchy_word(reference, count),
            None => Some(value),
        }
    }

    // Sample input: "shell.common.search" or "menu.apps.label" names a catalog and its key.
    fn omarchy_word(&self, reference: &str, count: Option<i64>) -> Option<&str> {
        let (kind, key) = reference.split_once('.')?;
        catalog::lookup(
            self.omarchy_english.get(kind)?, self.omarchy_translated.get(kind)?,
            key, self.language(), count,
        )
        .or_else(|| OMARCHY_ENGLISH.iter().find(|(name, _)| *name == reference).map(|(_, word)| *word))
    }

    // Includes missing files, so creation, removal, replacement and same-length edits all trigger reload.
    pub fn poll_changed(&mut self) -> bool {
        match self.prepare_changed() {
            Some(prepared) => {
                *self = prepared;
                true
            }
            None => false,
        }
    }

    // Call on a worker: this checks metadata, including the identity of atomically replaced files.
    pub fn sources_unchanged(&self) -> bool {
        self.watched
            .iter()
            .all(|(path, before)| stamp(path) == *before)
            && catalog::read(&self.paths.language_file) == self.language_text
    }

    // Stage a stable replacement without mutating the catalog any current frame still holds.
    pub fn prepare_changed(&self) -> Option<Self> {
        if self.sources_unchanged() {
            return None;
        }
        let mut prepared = self.clone();
        prepared.reload();
        prepared.sources_unchanged().then_some(prepared)
    }

    fn watch(&mut self, path: PathBuf) -> PathBuf {
        self.watched.push((path.clone(), stamp(&path)));
        path
    }

    fn flea_catalog(&mut self, code: &str, mut base: Catalog) -> Catalog {
        for root in self.paths.catalogs.clone().iter().rev() {
            let file = self.watch(root.join(code).join("flea.json"));
            base.extend(catalog::load(&file, false));
        }
        base
    }

    pub fn reload(&mut self) {
        self.watched.clear();
        // Stamp before reading so a concurrent edit remains visible to the next poll.
        let language_file = self.watch(self.paths.language_file.clone());
        let registry = self.watch(self.paths.omarchy.join("default/i18n/locales.json"));
        // Omarchy's roster and language file name the language; without Omarchy, Flea's own
        // catalogs and the session's locale do.
        let registry_text = catalog::read(&registry);
        let omarchy = registry_text.is_some();
        self.roster = match registry_text {
            Some(text) => locale::parse(&text, &registry),
            None => locale::from_catalogs(&self.paths.catalogs),
        };
        self.language_text = catalog::read(&language_file);
        let session;
        let code = match self.language_text.as_deref() {
            Some(text) => language_code(text),
            // On Omarchy a missing language file means English, as it always has.
            None if omarchy => "",
            None => {
                session = locale::pick_from_session(&mut self.roster, |key| std::env::var(key).ok()).unwrap_or_default();
                session.as_str()
            }
        };
        self.locale = self.roster.iter().find(|l| l.code == code).cloned()
            .unwrap_or_else(|| self.roster.iter().find(|l| l.code == "en").unwrap().clone());
        let code = self.locale.code.clone();
        self.english = self.flea_catalog("en", catalog::parse(include_str!("../../i18n/en/flea.json"), false));
        self.translated = self.flea_catalog(&code, Catalog::new());
        self.omarchy_english.clear();
        self.omarchy_translated.clear();
        for kind in catalog::KINDS {
            let root = self.paths.omarchy.join("default/i18n");
            let english = self.watch(root.join("en").join(format!("{kind}.json")));
            let translated = self.watch(root.join(&code).join(format!("{kind}.json")));
            self.omarchy_english.insert(kind, catalog::load(&english, kind == "menu"));
            self.omarchy_translated.insert(kind, catalog::load(&translated, kind == "menu"));
        }
    }
}

// Placeholders and isolates are skipped: their text takes its own direction inside the line.
fn opens_latin(template: &str) -> bool {
    const HEBREW_TO_ARABIC_EXTENDED: std::ops::RangeInclusive<char> = '\u{0590}'..='\u{08ff}';
    const ALPHABETIC_AND_ARABIC_PRESENTATION: std::ops::RangeInclusive<char> = '\u{fb1d}'..='\u{fdff}';
    const ARABIC_PRESENTATION_B: std::ops::RangeInclusive<char> = '\u{fe70}'..='\u{feff}';
    const HISTORIC_RTL: std::ops::RangeInclusive<char> = '\u{10800}'..='\u{10fff}';
    const MENDE_TO_ARABIC_MATH: std::ops::RangeInclusive<char> = '\u{1e800}'..='\u{1efff}';
    let rtl = |c: char| HEBREW_TO_ARABIC_EXTENDED.contains(&c)
        || ALPHABETIC_AND_ARABIC_PRESENTATION.contains(&c) || ARABIC_PRESENTATION_B.contains(&c)
        || HISTORIC_RTL.contains(&c) || MENDE_TO_ARABIC_MATH.contains(&c);
    let (mut first, mut depth, mut chars) = (None, 0usize, template.chars().peekable());
    while let Some(c) = chars.next() {
        match c {
            '%' => while chars.next_if(char::is_ascii_digit).is_some() {},
            '\u{2066}'..='\u{2068}' => depth += 1,
            '\u{2069}' => depth = depth.saturating_sub(1),
            c if depth == 0 && c.is_alphabetic() => match first {
                None if rtl(c) => return false,
                None => first = Some(c),
                Some(_) if rtl(c) => return true,
                Some(_) => {}
            },
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod test_catalogs;
#[cfg(test)]
mod plural_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod adv_c_tests;
#[cfg(test)]
mod gap64_tests;

#[cfg(test)]
mod opr_reds_tests;
#[cfg(test)]
mod ar_tui_copy_tests;
